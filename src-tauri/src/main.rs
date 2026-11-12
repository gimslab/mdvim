#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;

use cli::{CliFileEntry, CursorTarget, LaunchSpec, WaitResultPayload};
use rmpv::{decode::read_value, encode::write_value, Value};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{oneshot, Notify};

struct NvimSession {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_msgid: u64,
    last_lines: Vec<String>,
    last_cursor_row: usize,
    last_cursor_col_byte: usize,
    last_cursor_col_char: usize,
}

#[derive(Serialize, Clone, Debug)]
struct YankEvent {
    regname: String,
    regtype: String, // "v", "V", "\x16"
    regcontents: Vec<String>,
    start_row: usize, // 0-based
    start_col: usize, // 1-based (from col("'["))
    end_row: usize,   // 0-based
    end_col: usize,   // 1-based (from col("']"))
    operator: String,
}

#[derive(Serialize, Clone)]
struct NvimState {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col_byte: usize,
    cursor_col_char: usize,
    mode: String,
    blocking: bool,
    cmdline: String,
    cmdpos: usize,
    last_yank: Option<YankEvent>,
    ex_command: Option<String>,
}

fn is_app_ex_command(cmd: &str) -> bool {
    let c = cmd.trim().trim_start_matches(':').trim();
    matches!(
        c,
        "w" | "w!"
            | "write"
            | "write!"
            | "q"
            | "q!"
            | "quit"
            | "quit!"
            | "qa"
            | "qa!"
            | "qall"
            | "qall!"
            | "wq"
            | "wq!"
            | "wqa"
            | "wqa!"
            | "x"
            | "x!"
            | "xa"
            | "xa!"
            | "xit"
            | "xit!"
            | "exit"
            | "exit!"
            | "bd"
            | "bd!"
            | "bdelete"
            | "bdelete!"
            | "bw"
            | "bw!"
            | "bwipeout"
            | "bwipeout!"
            | "tabc"
            | "tabc!"
            | "tabclose"
            | "tabclose!"
            | "close"
            | "close!"
            | "clo"
            | "clo!"
            | "cq"
            | "cq!"
            | "cquit"
            | "cquit!"
            | "e"
            | "e!"
            | "edit"
            | "edit!"
            | "enew"
            | "enew!"
            | "tabnew"
            | "tabe"
            | "tabedit"
            | "bn"
            | "bnext"
            | "bp"
            | "bprev"
            | "bprevious"
            | "tabn"
            | "tabnext"
            | "tabp"
            | "tabprev"
            | "tabprevious"
    ) || c.starts_with("w ")
        || c.starts_with("w! ")
        || c.starts_with("write ")
        || c.starts_with("write! ")
        || c.starts_with("e ")
        || c.starts_with("edit ")
}

fn find_nvim_path() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("NVIM_PATH") {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            return p;
        }
    }

    #[cfg(target_os = "macos")]
    {
        let candidates = [
            "/opt/homebrew/bin/nvim",
            "/usr/local/bin/nvim",
            "/opt/local/bin/nvim",
            "/usr/bin/nvim",
        ];
        for candidate in candidates {
            let p = std::path::PathBuf::from(candidate);
            if p.exists() {
                return p;
            }
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let home_paths = [
            format!("{}/.local/bin/nvim", home),
            format!("{}/.cargo/bin/nvim", home),
            format!("{}/bin/nvim", home),
        ];
        for candidate in home_paths {
            let p = std::path::PathBuf::from(candidate);
            if p.exists() {
                return p;
            }
        }
    }

    std::path::PathBuf::from("nvim")
}

impl NvimSession {
    fn new(initial_lines: Vec<String>) -> Result<Self, String> {
        let nvim_bin = find_nvim_path();
        let mut child = Command::new(nvim_bin)
            .args(["--embed", "--headless", "-n"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn nvim: {e}"))?;

        let stdin = child.stdin.take().ok_or("No stdin")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("No stdout")?);

        let mut session = Self {
            _child: child,
            stdin,
            stdout,
            next_msgid: 1,
            last_lines: initial_lines.clone(),
            last_cursor_row: 0,
            last_cursor_col_byte: 0,
            last_cursor_col_char: 0,
        };

        session.call(
            "nvim_command",
            vec![Value::from(
                "set noswapfile hidden encoding=utf-8 shortmess+=IsF cmdheight=5",
            )],
        )?;
        session.call(
            "nvim_command",
            vec![Value::from(
                "autocmd TextYankPost * let g:mdvim_last_yank = deepcopy(v:event) | let g:mdvim_last_yank_pos = [line(\"'[\"), col(\"'[\"), line(\"']\"), col(\"']\")]",
            )],
        )?;
        session.call(
            "nvim_command",
            vec![Value::from(
                "nnoremap <silent> ZZ :<C-u>let g:mdvim_ex_cmd = 'wq'<CR> | nnoremap <silent> ZQ :<C-u>let g:mdvim_ex_cmd = 'q!'<CR>",
            )],
        )?;
        session.set_lines(&initial_lines)?;

        Ok(session)
    }

    fn ensure_alive(&mut self) -> Result<(), String> {
        if let Ok(Some(_)) = self._child.try_wait() {
            let lines = self.last_lines.clone();
            *self = Self::new(lines)?;
        }
        Ok(())
    }

    fn call(&mut self, method: &str, params: Vec<Value>) -> Result<Value, String> {
        self.ensure_alive()?;
        let msgid = self.next_msgid;
        self.next_msgid += 1;

        let req = Value::Array(vec![
            Value::from(0),
            Value::from(msgid),
            Value::from(method),
            Value::Array(params),
        ]);

        if let Err(e) = write_value(&mut self.stdin, &req)
            .map_err(|e| e.to_string())
            .and_then(|_| self.stdin.flush().map_err(|e| e.to_string()))
        {
            let lines = self.last_lines.clone();
            if let Ok(new_sess) = Self::new(lines) {
                *self = new_sess;
            }
            return Err(format!("Write/Flush error (session respawned): {e}"));
        }

        loop {
            let msg = match read_value(&mut self.stdout) {
                Ok(m) => m,
                Err(e) => {
                    let lines = self.last_lines.clone();
                    if let Ok(new_sess) = Self::new(lines) {
                        *self = new_sess;
                    }
                    return Err(format!("Read error (session respawned): {e}"));
                }
            };
            if let Value::Array(arr) = msg {
                if arr.len() >= 4 && arr[0].as_u64() == Some(1) && arr[1].as_u64() == Some(msgid) {
                    if !arr[2].is_nil() {
                        return Err(format!("RPC error: {:?}", arr[2]));
                    }
                    return Ok(arr[3].clone());
                }
            }
        }
    }

    fn set_lines(&mut self, lines: &[String]) -> Result<(), String> {
        let val_lines: Vec<Value> = lines.iter().map(|s| Value::from(s.as_str())).collect();
        self.call(
            "nvim_buf_set_lines",
            vec![
                Value::from(0),
                Value::from(0),
                Value::from(-1),
                Value::from(false),
                Value::Array(val_lines),
            ],
        )?;
        self.last_lines = lines.to_vec();
        Ok(())
    }

    fn get_state(&mut self) -> Result<NvimState, String> {
        let mode_val = self.call("nvim_get_mode", vec![])?;
        let mut mode = "n".to_string();
        let mut blocking = false;
        if let Value::Map(map) = mode_val {
            for (k, v) in map {
                if k.as_str() == Some("mode") {
                    mode = v.as_str().unwrap_or("n").to_string();
                } else if k.as_str() == Some("blocking") {
                    blocking = v.as_bool().unwrap_or(false);
                }
            }
        }

        if blocking {
            return Ok(NvimState {
                lines: self.last_lines.clone(),
                cursor_row: self.last_cursor_row,
                cursor_col_byte: self.last_cursor_col_byte,
                cursor_col_char: self.last_cursor_col_char,
                mode,
                blocking: true,
                cmdline: String::new(),
                cmdpos: 0,
                last_yank: None,
                ex_command: None,
            });
        }

        let mut cmdline = String::new();
        let mut cmdpos = 0usize;
        if mode == "c" {
            if let Ok(val) = self.call(
                "nvim_call_function",
                vec![Value::from("getcmdline"), Value::Array(vec![])],
            ) {
                cmdline = val.as_str().unwrap_or("").to_string();
            }
            if let Ok(val) = self.call(
                "nvim_call_function",
                vec![Value::from("getcmdpos"), Value::Array(vec![])],
            ) {
                cmdpos = val.as_u64().unwrap_or(1) as usize;
            }
        }

        let lines_val = self.call(
            "nvim_buf_get_lines",
            vec![
                Value::from(0),
                Value::from(0),
                Value::from(-1),
                Value::from(false),
            ],
        )?;
        let mut lines = Vec::new();
        if let Value::Array(arr) = lines_val {
            for item in arr {
                lines.push(item.as_str().unwrap_or("").to_string());
            }
        }

        let cursor_val = self.call("nvim_win_get_cursor", vec![Value::from(0)])?;
        let mut cursor_row = 0usize;
        let mut cursor_col_byte = 0usize;
        if let Value::Array(arr) = cursor_val {
            if arr.len() >= 2 {
                cursor_row = (arr[0].as_u64().unwrap_or(1) as usize).saturating_sub(1);
                cursor_col_byte = arr[1].as_u64().unwrap_or(0) as usize;
            }
        }

        let line_str = lines.get(cursor_row).map(|s| s.as_str()).unwrap_or("");
        let safe_byte = cursor_col_byte.min(line_str.len());
        let cursor_col_char = line_str
            .get(..safe_byte)
            .map(|sub| sub.chars().count())
            .unwrap_or(0);

        self.last_lines = lines.clone();
        self.last_cursor_row = cursor_row;
        self.last_cursor_col_byte = cursor_col_byte;
        self.last_cursor_col_char = cursor_col_char;

        // Check if ZZ / ZQ recorded an Ex command
        let mut ex_command: Option<String> = None;
        if let Ok(ex_val) = self.call(
            "nvim_eval",
            vec![Value::from("exists('g:mdvim_ex_cmd') ? g:mdvim_ex_cmd : ''")],
        ) {
            let s = ex_val.as_str().unwrap_or("").trim().to_string();
            if !s.is_empty() {
                let _ = self.call("nvim_command", vec![Value::from("unlet! g:mdvim_ex_cmd")]);
                ex_command = Some(s);
            }
        }

        // Check if TextYankPost recorded any yank event
        let yank_val = self.call(
            "nvim_eval",
            vec![Value::from(
                "exists('g:mdvim_last_yank') ? [g:mdvim_last_yank, g:mdvim_last_yank_pos] : []",
            )],
        )?;

        let mut last_yank: Option<YankEvent> = None;
        if let Value::Array(arr) = yank_val {
            if arr.len() >= 2 {
                let mut regname = String::new();
                let mut regtype = "V".to_string();
                let mut regcontents = Vec::new();
                let mut operator = "y".to_string();

                if let Value::Map(map) = &arr[0] {
                    for (k, v) in map {
                        if k.as_str() == Some("regname") {
                            regname = v.as_str().unwrap_or("").to_string();
                        } else if k.as_str() == Some("regtype") {
                            regtype = v.as_str().unwrap_or("V").to_string();
                        } else if k.as_str() == Some("operator") {
                            operator = v.as_str().unwrap_or("y").to_string();
                        } else if k.as_str() == Some("regcontents") {
                            if let Value::Array(c_arr) = v {
                                for line in c_arr {
                                    regcontents.push(line.as_str().unwrap_or("").to_string());
                                }
                            }
                        }
                    }
                }

                let mut start_row = 1usize;
                let mut start_col = 1usize;
                let mut end_row = 1usize;
                let mut end_col = 1usize;
                if let Value::Array(pos_arr) = &arr[1] {
                    if pos_arr.len() >= 4 {
                        start_row = pos_arr[0].as_u64().unwrap_or(1) as usize;
                        start_col = pos_arr[1].as_u64().unwrap_or(1) as usize;
                        end_row = pos_arr[2].as_u64().unwrap_or(1) as usize;
                        end_col = pos_arr[3].as_u64().unwrap_or(1) as usize;
                    }
                }

                // Clear the global variables in Neovim so we don't process it repeatedly
                let _ = self.call(
                    "nvim_command",
                    vec![Value::from("unlet! g:mdvim_last_yank g:mdvim_last_yank_pos")],
                );

                last_yank = Some(YankEvent {
                    regname,
                    regtype,
                    regcontents,
                    start_row: start_row.saturating_sub(1),
                    start_col,
                    end_row: end_row.saturating_sub(1),
                    end_col,
                    operator,
                });
            }
        }

        Ok(NvimState {
            lines,
            cursor_row,
            cursor_col_byte,
            cursor_col_char,
            mode,
            blocking: false,
            cmdline,
            cmdpos,
            last_yank,
            ex_command,
        })
    }
}

struct AppState(Mutex<NvimSession>);

#[tauri::command]
async fn nvim_get_state(state: tauri::State<'_, AppState>) -> Result<NvimState, String> {
    let mut s = state.0.lock().map_err(|e| e.to_string())?;
    s.get_state()
}

#[tauri::command]
async fn nvim_input(keys: String, state: tauri::State<'_, AppState>) -> Result<NvimState, String> {
    let mut s = state.0.lock().map_err(|e| e.to_string())?;

    // Check if a compound Ex command string was sent at once (e.g. ":q!<CR>")
    if keys.starts_with(':') && (keys.ends_with("<CR>") || keys.ends_with("<NL>")) {
        let inner = keys
            .trim_end_matches("<CR>")
            .trim_end_matches("<NL>")
            .trim_start_matches(':')
            .trim();
        if is_app_ex_command(inner) {
            let _ = s.call("nvim_input", vec![Value::from("<C-\\><C-n>")]);
            let mut st = s.get_state()?;
            st.ex_command = Some(inner.to_string());
            return Ok(st);
        }
    }

    // Check if Enter (<CR>) is pressed while in Command-line mode ('c')
    if keys == "<CR>" || keys == "<NL>" || keys == "\r" || keys == "\n" {
        if let Ok(Value::Map(map)) = s.call("nvim_get_mode", vec![]) {
            let mut in_cmd_mode = false;
            for (k, v) in map {
                if k.as_str() == Some("mode") && v.as_str() == Some("c") {
                    in_cmd_mode = true;
                }
            }
            if in_cmd_mode {
                if let Ok(val) = s.call(
                    "nvim_call_function",
                    vec![Value::from("getcmdline"), Value::Array(vec![])],
                ) {
                    let cmd = val
                        .as_str()
                        .unwrap_or("")
                        .trim()
                        .trim_start_matches(':')
                        .trim()
                        .to_string();
                    if is_app_ex_command(&cmd) {
                        // Cancel Neovim's command-line mode without executing `:q!` / `:q` / `:w` in headless Neovim
                        let _ = s.call("nvim_input", vec![Value::from("<C-\\><C-n>")]);
                        let mut st = s.get_state()?;
                        st.ex_command = Some(cmd);
                        return Ok(st);
                    }
                }
            }
        }
    }

    s.call("nvim_input", vec![Value::from(keys.as_str())])?;
    s.get_state()
}

#[tauri::command]
async fn nvim_exec_command(
    command: String,
    state: tauri::State<'_, AppState>,
) -> Result<NvimState, String> {
    let mut s = state.0.lock().map_err(|e| e.to_string())?;
    let trimmed = command.trim().trim_start_matches(':').trim();
    if is_app_ex_command(trimmed) {
        let mut st = s.get_state()?;
        st.ex_command = Some(trimmed.to_string());
        return Ok(st);
    }
    let _ = s.call("nvim_command", vec![Value::from(trimmed)]);
    s.get_state()
}

#[tauri::command]
async fn nvim_paste_text(
    text: String,
    state: tauri::State<'_, AppState>,
) -> Result<NvimState, String> {
    let mut s = state.0.lock().map_err(|e| e.to_string())?;
    // nvim_paste handles multi-line text cleanly across all modes without mangling newlines
    let _ = s.call(
        "nvim_paste",
        vec![
            Value::from(text.as_str()),
            Value::from(true),  // crlf: false (literal)
            Value::from(-1i64), // phase: -1 (all at once)
        ],
    )?;
    s.get_state()
}

#[tauri::command]
async fn nvim_set_register(
    regname: String,
    lines: Vec<String>,
    regtype: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut s = state.0.lock().map_err(|e| e.to_string())?;
    let r_name = if regname.is_empty() { "\"" } else { &regname };
    let val_lines: Vec<Value> = lines.iter().map(|l| Value::from(l.as_str())).collect();
    // Set both the target register and the unnamed register
    let _ = s.call(
        "nvim_call_function",
        vec![
            Value::from("setreg"),
            Value::Array(vec![
                Value::from(r_name),
                Value::Array(val_lines.clone()),
                Value::from(regtype.as_str()),
            ]),
        ],
    );
    if r_name != "\"" {
        let _ = s.call(
            "nvim_call_function",
            vec![
                Value::from("setreg"),
                Value::Array(vec![
                    Value::from("\""),
                    Value::Array(val_lines.clone()),
                    Value::from(regtype.as_str()),
                ]),
            ],
        );
    }
    // Also sync to system clipboard registers '+' and '*'
    for clip in ["+", "*"] {
        let _ = s.call(
            "nvim_call_function",
            vec![
                Value::from("setreg"),
                Value::Array(vec![
                    Value::from(clip),
                    Value::Array(val_lines.clone()),
                    Value::from(regtype.as_str()),
                ]),
            ],
        );
    }
    Ok(())
}

#[tauri::command]
async fn nvim_set_buffer(
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col_char: usize,
    state: tauri::State<'_, AppState>,
) -> Result<NvimState, String> {
    let mut s = state.0.lock().map_err(|e| e.to_string())?;
    s.set_lines(&lines)?;
    let row_1 = (cursor_row + 1).max(1).min(lines.len().max(1));
    let line_str = lines.get(row_1 - 1).map(|l| l.as_str()).unwrap_or("");
    let byte_col: usize = line_str
        .chars()
        .take(cursor_col_char)
        .map(|c| c.len_utf8())
        .sum();
    let _ = s.call(
        "nvim_win_set_cursor",
        vec![
            Value::from(0),
            Value::Array(vec![Value::from(row_1 as u64), Value::from(byte_col as u64)]),
        ],
    );
    s.get_state()
}

static NEXT_REQ_ID: AtomicU64 = AtomicU64::new(1);

struct PendingRequests(Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>);
struct LastReportedState {
    state: Mutex<serde_json::Value>,
    ready: AtomicBool,
    notify: Notify,
}

impl LastReportedState {
    fn new() -> Self {
        Self {
            state: Mutex::new(serde_json::Value::Null),
            ready: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }
}

struct InitialLaunchState {
    entries: Mutex<Vec<CliFileEntry>>,
    launch_cwd: PathBuf,
}

#[derive(Clone, Debug)]
struct WaitTabCompletion {
    content: String,
    aborted: bool,
}

struct WaitManagerInner {
    slots: HashMap<String, (Option<oneshot::Sender<WaitTabCompletion>>, String)>,
    init_tokens: Vec<String>,
    init_results: HashMap<String, WaitTabCompletion>,
    wait_out_path: Option<PathBuf>,
    wait_out_written: bool,
}

struct WaitManager(Mutex<WaitManagerInner>);

impl WaitManager {
    fn new(wait_out_path: Option<PathBuf>) -> Self {
        Self(Mutex::new(WaitManagerInner {
            slots: HashMap::new(),
            init_tokens: Vec::new(),
            init_results: HashMap::new(),
            wait_out_path,
            wait_out_written: false,
        }))
    }

    fn register_token(
        &self,
        token: String,
        fallback_content: String,
        is_init: bool,
    ) -> oneshot::Receiver<WaitTabCompletion> {
        let (tx, rx) = oneshot::channel();
        if let Ok(mut inner) = self.0.lock() {
            if is_init {
                inner.init_tokens.push(token.clone());
            }
            inner
                .slots
                .insert(token, (Some(tx), fallback_content));
        }
        rx
    }

    fn complete_token(&self, token: &str, content: String, aborted: bool) {
        if let Ok(mut inner) = self.0.lock() {
            let completion = WaitTabCompletion {
                content: content.clone(),
                aborted,
            };
            if let Some((mut tx_opt, _)) = inner.slots.remove(token) {
                if let Some(tx) = tx_opt.take() {
                    let _ = tx.send(completion.clone());
                }
            }
            if inner.init_tokens.iter().any(|t| t == token) {
                inner.init_results.insert(token.to_string(), completion);
                Self::maybe_write_init_wait_out(&mut inner);
            }
        }
    }

    fn flush_all(&self, reported: &LastReportedState, aborted: bool) {
        let reported_val = reported
            .state
            .lock()
            .map(|g| g.clone())
            .unwrap_or(serde_json::Value::Null);

        let mut live_by_token: HashMap<String, String> = HashMap::new();
        if let Some(tabs) = reported_val.get("tabs").and_then(|t| t.as_array()) {
            for t in tabs {
                if let (Some(tok), Some(md)) = (
                    t.get("waitToken").and_then(|v| v.as_str()),
                    t.get("rawMarkdown").and_then(|v| v.as_str()),
                ) {
                    live_by_token.insert(tok.to_string(), md.to_string());
                }
            }
        }
        if let Some(active) = reported_val.get("activeTab") {
            if let (Some(tok), Some(md)) = (
                active.get("waitToken").and_then(|v| v.as_str()),
                active.get("rawMarkdown").and_then(|v| v.as_str()),
            ) {
                live_by_token.insert(tok.to_string(), md.to_string());
            }
        }

        if let Ok(mut inner) = self.0.lock() {
            let remaining_keys: Vec<String> = inner.slots.keys().cloned().collect();
            for key in remaining_keys {
                if let Some((mut tx_opt, fallback)) = inner.slots.remove(&key) {
                    let final_content = live_by_token.remove(&key).unwrap_or(fallback);
                    let comp = WaitTabCompletion {
                        content: final_content,
                        aborted,
                    };
                    if let Some(tx) = tx_opt.take() {
                        let _ = tx.send(comp.clone());
                    }
                    if inner.init_tokens.iter().any(|t| t == &key) {
                        inner.init_results.insert(key, comp);
                    }
                }
            }
            Self::maybe_write_init_wait_out(&mut inner);
        }
    }

    fn maybe_write_init_wait_out(inner: &mut WaitManagerInner) {
        if inner.wait_out_written {
            return;
        }
        let Some(ref out_path) = inner.wait_out_path else {
            return;
        };
        if inner.init_tokens.is_empty() {
            return;
        }
        if !inner
            .init_tokens
            .iter()
            .all(|t| inner.init_results.contains_key(t))
        {
            return;
        }

        let mut aborted = false;
        let mut contents = Vec::new();
        for tok in &inner.init_tokens {
            if let Some(res) = inner.init_results.get(tok) {
                if res.aborted {
                    aborted = true;
                }
                contents.push(res.content.clone());
            }
        }

        let payload = WaitResultPayload { aborted, contents };
        if let Ok(json) = serde_json::to_string(&payload) {
            let tmp_path = out_path.with_extension("tmp");
            if std::fs::write(&tmp_path, json).is_ok() {
                let _ = std::fs::rename(&tmp_path, out_path);
                inner.wait_out_written = true;
            }
        }
    }
}

#[tauri::command]
async fn ctl_report_state(
    state_json: serde_json::Value,
    reported: tauri::State<'_, Arc<LastReportedState>>,
) -> Result<(), String> {
    {
        let mut st = reported.state.lock().map_err(|e| e.to_string())?;
        *st = state_json;
    }
    if !reported.ready.swap(true, Ordering::SeqCst) {
        reported.notify.notify_waiters();
    }
    Ok(())
}

#[tauri::command]
async fn ctl_response(
    req_id: u64,
    result: serde_json::Value,
    pending: tauri::State<'_, Arc<PendingRequests>>,
) -> Result<(), String> {
    let mut map = pending.0.lock().map_err(|e| e.to_string())?;
    if let Some(tx) = map.remove(&req_id) {
        let _ = tx.send(result);
    }
    Ok(())
}

#[tauri::command]
async fn app_complete_wait_tab(
    wait_token: String,
    content: String,
    aborted: bool,
    wait_mgr: tauri::State<'_, Arc<WaitManager>>,
) -> Result<(), String> {
    wait_mgr.complete_token(&wait_token, content, aborted);
    Ok(())
}

fn save_dirty_tabs_on_close(reported: &LastReportedState, launch_cwd: &Path) {
    let Ok(mut guard) = reported.state.lock() else {
        return;
    };
    let Some(tabs) = guard.get_mut("tabs").and_then(|t| t.as_array_mut()) else {
        return;
    };
    for tab in tabs.iter_mut() {
        let dirty = tab.get("dirty").and_then(|v| v.as_bool()).unwrap_or(false);
        let readonly = tab.get("readonly").and_then(|v| v.as_bool()).unwrap_or(false);
        let is_stdin = tab.get("isStdin").and_then(|v| v.as_bool()).unwrap_or(false);
        let has_conflict = tab.get("hasConflict").and_then(|v| v.as_bool()).unwrap_or(false);
        let path_opt = tab
            .get("path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        if !dirty || readonly || has_conflict || (is_stdin && path_opt.is_none()) {
            continue;
        }
        let Some(content) = tab
            .get("rawMarkdown")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        else {
            continue;
        };
        let target_path = if let Some(ref p) = path_opt {
            resolve_user_path_with_base(p, launch_cwd)
        } else {
            let title = tab
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("untitled.md");
            find_unique_untitled_path(launch_cwd, title, &[])
        };
        if let Some(parent) = target_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::write(&target_path, &content).is_ok() {
            record_recent_file(&target_path.to_string_lossy());
            if let Some(obj) = tab.as_object_mut() {
                obj.insert("dirty".to_string(), serde_json::Value::Bool(false));
            }
        }
    }
}

#[tauri::command]
async fn app_exit(
    code: i32,
    aborted: Option<bool>,
    app_handle: tauri::AppHandle,
    wait_mgr: tauri::State<'_, Arc<WaitManager>>,
    reported: tauri::State<'_, Arc<LastReportedState>>,
    launch_state: tauri::State<'_, Arc<InitialLaunchState>>,
) -> Result<(), String> {
    let is_aborted = aborted.unwrap_or(code != 0);
    if !is_aborted {
        save_dirty_tabs_on_close(&reported, &launch_state.launch_cwd);
    }
    wait_mgr.flush_all(&reported, is_aborted);
    app_handle.exit(code);
    Ok(())
}

#[tauri::command]
async fn app_install_cli_path() -> Result<String, String> {
    install_cli_symlink()
}

fn get_config_path() -> std::path::PathBuf {
    let config_dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
        })
        .unwrap_or_else(std::env::temp_dir);
    config_dir.join("mdvim").join("config.toml")
}

#[tauri::command]
async fn app_load_config() -> Result<Option<String>, String> {
    let path = get_config_path();
    if path.exists() {
        std::fs::read_to_string(&path)
            .map(Some)
            .map_err(|e| format!("Failed to read config file {:?}: {e}", path))
    } else {
        Ok(None)
    }
}

#[tauri::command]
async fn app_save_config(content: String) -> Result<String, String> {
    let path = get_config_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, content)
        .map(|_| path.to_string_lossy().to_string())
        .map_err(|e| format!("Failed to write config file {:?}: {e}", path))
}

fn detect_default_launch_dir() -> std::path::PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    // When running via `npm run tauri dev`, Cargo runs inside `src-tauri/`,
    // so step up to the workspace root where the user invoked the command.
    if cwd.file_name().and_then(|n| n.to_str()) == Some("src-tauri") {
        if let Some(parent) = cwd.parent() {
            if parent.join("package.json").exists() {
                return parent.to_path_buf();
            }
        }
    }
    cwd
}

fn resolve_user_path_with_base(raw: &str, base_dir: &Path) -> std::path::PathBuf {
    cli::resolve_path_from_cwd(raw, base_dir)
}

fn file_mtime_ms(path: &std::path::Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn get_recent_files_path() -> std::path::PathBuf {
    let config_dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
        })
        .unwrap_or_else(std::env::temp_dir);
    config_dir.join("mdvim").join("recent.json")
}

fn load_recent_files_list() -> Vec<String> {
    let path = get_recent_files_path();
    if let Ok(raw) = std::fs::read_to_string(&path) {
        if let Ok(list) = serde_json::from_str::<Vec<String>>(&raw) {
            return list;
        }
    }
    Vec::new()
}

fn record_recent_file(file_path: &str) {
    let trimmed = file_path.trim();
    if trimmed.is_empty() {
        return;
    }
    let mut list = load_recent_files_list();
    list.retain(|p| p != trimmed);
    list.insert(0, trimmed.to_string());
    list.truncate(15);
    let path = get_recent_files_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(&list) {
        let _ = std::fs::write(&path, json);
    }
}

#[tauri::command]
async fn app_get_recent_files() -> Result<Vec<String>, String> {
    Ok(load_recent_files_list())
}

#[derive(Serialize)]
struct ReadFileResult {
    path: String,
    content: String,
    mtime_ms: u64,
}

#[tauri::command]
async fn app_read_file(
    path: String,
    launch_state: tauri::State<'_, Arc<InitialLaunchState>>,
) -> Result<ReadFileResult, String> {
    let resolved = resolve_user_path_with_base(&path, &launch_state.launch_cwd);
    let (content, mtime_ms) = if resolved.exists() {
        let c = std::fs::read_to_string(&resolved)
            .map_err(|e| format!("Failed to read file {:?}: {e}", resolved))?;
        (c, file_mtime_ms(&resolved))
    } else {
        (String::new(), 0)
    };
    let resolved_str = resolved.to_string_lossy().to_string();
    record_recent_file(&resolved_str);
    Ok(ReadFileResult {
        path: resolved_str,
        content,
        mtime_ms,
    })
}

#[tauri::command]
async fn app_check_file_modified(
    path: String,
    known_mtime_ms: u64,
    launch_state: tauri::State<'_, Arc<InitialLaunchState>>,
) -> Result<Option<ReadFileResult>, String> {
    let resolved = resolve_user_path_with_base(&path, &launch_state.launch_cwd);
    if !resolved.exists() {
        return Ok(None);
    }
    let current_mtime = file_mtime_ms(&resolved);
    if current_mtime == 0 || current_mtime == known_mtime_ms {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&resolved)
        .map_err(|e| format!("Failed to read modified file {:?}: {e}", resolved))?;
    Ok(Some(ReadFileResult {
        path: resolved.to_string_lossy().to_string(),
        content,
        mtime_ms: current_mtime,
    }))
}

#[tauri::command]
async fn app_get_cli_files(
    launch_state: tauri::State<'_, Arc<InitialLaunchState>>,
) -> Result<Vec<CliFileEntry>, String> {
    let mut guard = launch_state.entries.lock().map_err(|e| e.to_string())?;
    for entry in guard.iter() {
        if let Some(ref p) = entry.path {
            record_recent_file(p);
        }
    }
    Ok(std::mem::take(&mut *guard))
}

#[tauri::command]
async fn app_write_file(
    path: String,
    content: String,
    launch_state: tauri::State<'_, Arc<InitialLaunchState>>,
) -> Result<String, String> {
    let resolved = resolve_user_path_with_base(&path, &launch_state.launch_cwd);
    if let Some(parent) = resolved.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&resolved, content)
        .map(|_| {
            let s = resolved.to_string_lossy().to_string();
            record_recent_file(&s);
            s
        })
        .map_err(|e| format!("Failed to write file {:?}: {e}", resolved))
}

fn find_unique_untitled_path(
    base_dir: &std::path::Path,
    desired_name: &str,
    reserved_names: &[String],
) -> std::path::PathBuf {
    let safe_name = desired_name.replace(['/', '\\'], "_");
    let initial = if safe_name.is_empty() {
        "untitled.md".to_string()
    } else {
        safe_name
    };

    let candidate_path = base_dir.join(&initial);
    if !candidate_path.exists() && !reserved_names.iter().any(|r| r == &initial) {
        return candidate_path;
    }

    let path_obj = std::path::Path::new(&initial);
    let ext = path_obj
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("md");
    let raw_stem = path_obj
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled");

    let base_stem = if let Some((prefix, suffix)) = raw_stem.rsplit_once('-') {
        if !prefix.is_empty() && !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            prefix
        } else {
            raw_stem
        }
    } else {
        raw_stem
    };

    let mut idx = 1u64;
    loop {
        let file_name = if ext.is_empty() {
            format!("{}-{}", base_stem, idx)
        } else {
            format!("{}-{}.{}", base_stem, idx, ext)
        };
        let p = base_dir.join(&file_name);
        if !p.exists() && !reserved_names.iter().any(|r| r == &file_name) {
            return p;
        }
        idx += 1;
    }
}

#[tauri::command]
async fn app_next_untitled_name(
    reserved_names: Vec<String>,
    launch_state: tauri::State<'_, Arc<InitialLaunchState>>,
) -> Result<String, String> {
    let base_dir = &launch_state.launch_cwd;
    let target = find_unique_untitled_path(base_dir, "untitled.md", &reserved_names);
    Ok(target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("untitled.md")
        .to_string())
}

#[tauri::command]
async fn app_save_draft(
    name: String,
    content: String,
    launch_state: tauri::State<'_, Arc<InitialLaunchState>>,
) -> Result<String, String> {
    let base_dir = &launch_state.launch_cwd;
    let target = find_unique_untitled_path(base_dir, &name, &[]);
    std::fs::write(&target, content)
        .map(|_| {
            let s = target.to_string_lossy().to_string();
            record_recent_file(&s);
            s
        })
        .map_err(|e| format!("Failed to write draft {:?}: {e}", target))
}

fn get_ipc_socket_path() -> std::path::PathBuf {
    #[cfg(unix)]
    {
        let uid = unsafe { libc::getuid() };
        std::path::PathBuf::from(format!("/tmp/mdvim-{}.sock", uid))
    }
    #[cfg(not(unix))]
    {
        std::env::temp_dir().join("mdvim.sock")
    }
}

fn start_ipc_server(
    app_handle: tauri::AppHandle,
    reported_state: Arc<LastReportedState>,
    pending_requests: Arc<PendingRequests>,
    wait_manager: Arc<WaitManager>,
) -> PathBuf {
    let default_socket = get_ipc_socket_path();
    #[cfg(unix)]
    let socket_path = {
        if default_socket.exists() {
            if std::os::unix::net::UnixStream::connect(&default_socket).is_ok() {
                // Primary instance is active (--new-window case); bind a secondary socket
                let uid = unsafe { libc::getuid() };
                let pid = std::process::id();
                PathBuf::from(format!("/tmp/mdvim-{}-{}.sock", uid, pid))
            } else {
                let _ = std::fs::remove_file(&default_socket);
                default_socket
            }
        } else {
            default_socket
        }
    };
    #[cfg(not(unix))]
    let socket_path = default_socket;

    let bound_socket = socket_path.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime for IPC");

        rt.block_on(async move {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as TokioBufReader};
            #[cfg(unix)]
            use tokio::net::UnixListener;

            #[cfg(unix)]
            let listener = match UnixListener::bind(&socket_path) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Failed to bind IPC socket {:?}: {e}", socket_path);
                    return;
                }
            };

            #[cfg(unix)]
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        eprintln!("IPC accept error: {e}");
                        continue;
                    }
                };

                let app = app_handle.clone();
                let rep = reported_state.clone();
                let pend = pending_requests.clone();
                let wmgr = wait_manager.clone();

                tokio::spawn(async move {
                    let (reader, mut writer) = stream.into_split();
                    let mut lines = TokioBufReader::new(reader).lines();

                    while let Ok(Some(line)) = lines.next_line().await {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        let req: serde_json::Value = match serde_json::from_str(trimmed) {
                            Ok(v) => v,
                            Err(e) => {
                                let err_resp = serde_json::json!({
                                    "status": "error",
                                    "message": format!("Invalid JSON: {e}")
                                });
                                let _ = writer.write_all(format!("{err_resp}\n").as_bytes()).await;
                                continue;
                            }
                        };

                        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
                        if method == "ping" {
                            let pong = serde_json::json!({ "status": "ok", "pong": true });
                            let _ = writer.write_all(format!("{pong}\n").as_bytes()).await;
                            continue;
                        }

                        // Wait until frontend has mounted and reported initial state
                        if !rep.ready.load(Ordering::SeqCst) {
                            let _ = tokio::time::timeout(
                                std::time::Duration::from_secs(10),
                                rep.notify.notified(),
                            )
                            .await;
                        }

                        let req_id = NEXT_REQ_ID.fetch_add(1, Ordering::SeqCst);

                        let response = match method {
                            "focus-window" => {
                                use tauri::Manager;
                                if let Some(win) = app.get_webview_window("main") {
                                    let _ = win.unminimize();
                                    let _ = win.show();
                                    let _ = win.set_focus();
                                }
                                serde_json::json!({ "status": "ok" })
                            }
                            "get-state" => {
                                let st = rep
                                    .state
                                    .lock()
                                    .map(|g| g.clone())
                                    .unwrap_or(serde_json::Value::Null);
                                serde_json::json!({ "status": "ok", "state": st })
                            }
                            "open-files" => {
                                use tauri::{Emitter, Manager};
                                if let Some(win) = app.get_webview_window("main") {
                                    let _ = win.unminimize();
                                    let _ = win.show();
                                    let _ = win.set_focus();
                                }

                                let params = req
                                    .get("params")
                                    .cloned()
                                    .unwrap_or(serde_json::Value::Null);
                                let wait = params
                                    .get("wait")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false);
                                let mut entries: Vec<CliFileEntry> = params
                                    .get("entries")
                                    .cloned()
                                    .and_then(|v| serde_json::from_value(v).ok())
                                    .unwrap_or_default();

                                let mut wait_rxs = Vec::new();
                                if wait {
                                    for (idx, entry) in entries.iter_mut().enumerate() {
                                        let tok = format!("ipc-wait-{}-{}", req_id, idx);
                                        entry.wait_token = Some(tok.clone());
                                        let rx = wmgr.register_token(
                                            tok,
                                            entry.content.clone(),
                                            false,
                                        );
                                        wait_rxs.push(rx);
                                    }
                                }

                                for entry in &entries {
                                    if let Some(ref p) = entry.path {
                                        record_recent_file(p);
                                    }
                                }

                                let (tx, rx) = oneshot::channel();
                                if let Ok(mut map) = pend.0.lock() {
                                    map.insert(req_id, tx);
                                }
                                let payload = serde_json::json!({
                                    "id": req_id,
                                    "method": "open-files",
                                    "params": { "entries": entries }
                                });

                                if let Err(e) = app.emit("ctl-request", payload) {
                                    if let Ok(mut map) = pend.0.lock() {
                                        map.remove(&req_id);
                                    }
                                    serde_json::json!({
                                        "status": "error",
                                        "message": format!("Failed to dispatch open-files to frontend: {e}")
                                    })
                                } else {
                                    let ack = tokio::time::timeout(
                                        std::time::Duration::from_secs(5),
                                        rx,
                                    )
                                    .await;
                                    match ack {
                                        Ok(Ok(_)) => {
                                            if !wait {
                                                serde_json::json!({ "status": "ok" })
                                            } else {
                                                let mut aborted = false;
                                                let mut contents = Vec::new();
                                                for wait_rx in wait_rxs {
                                                    if let Ok(comp) = wait_rx.await {
                                                        if comp.aborted {
                                                            aborted = true;
                                                        }
                                                        contents.push(comp.content);
                                                    }
                                                }
                                                serde_json::json!({
                                                    "status": "ok",
                                                    "aborted": aborted,
                                                    "contents": contents
                                                })
                                            }
                                        }
                                        Ok(Err(_)) => serde_json::json!({
                                            "status": "error",
                                            "message": "Channel closed before open-files response"
                                        }),
                                        Err(_) => {
                                            if let Ok(mut map) = pend.0.lock() {
                                                map.remove(&req_id);
                                            }
                                            serde_json::json!({
                                                "status": "error",
                                                "message": "open-files timed out after 5s"
                                            })
                                        }
                                    }
                                }
                            }
                            "exec" | "list-commands" | "insert-text" | "send-keys" => {
                                use tauri::Emitter;
                                let (tx, rx) = oneshot::channel();
                                {
                                    if let Ok(mut map) = pend.0.lock() {
                                        map.insert(req_id, tx);
                                    }
                                }
                                let payload = serde_json::json!({
                                    "id": req_id,
                                    "method": method,
                                    "params": req.get("params").cloned().unwrap_or(serde_json::Value::Null)
                                });
                                if let Err(e) = app.emit("ctl-request", payload) {
                                    if let Ok(mut map) = pend.0.lock() {
                                        map.remove(&req_id);
                                    }
                                    serde_json::json!({ "status": "error", "message": format!("Failed to dispatch to frontend: {e}") })
                                } else {
                                    match tokio::time::timeout(
                                        std::time::Duration::from_secs(5),
                                        rx,
                                    )
                                    .await
                                    {
                                        Ok(Ok(res)) => res,
                                        Ok(Err(_)) => serde_json::json!({ "status": "error", "message": "Channel closed before response" }),
                                        Err(_) => {
                                            if let Ok(mut map) = pend.0.lock() {
                                                map.remove(&req_id);
                                            }
                                            serde_json::json!({ "status": "error", "message": "Request timed out after 5s" })
                                        }
                                    }
                                }
                            }
                            _ => {
                                serde_json::json!({
                                    "status": "error",
                                    "message": format!("Unknown IPC method: {method}")
                                })
                            }
                        };

                        let _ = writer.write_all(format!("{response}\n").as_bytes()).await;
                    }
                });
            }
        });
    });

    bound_socket
}

/// Opens an http(s)/mailto URL in the system default handler.
#[tauri::command]
fn app_open_url(url: String) -> Result<(), String> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:"))
    {
        return Err("지원하지 않는 URL 스킴입니다 (http/https/mailto만 허용)".into());
    }
    #[cfg(target_os = "windows")]
    let res = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", &url])
        .spawn();
    #[cfg(target_os = "macos")]
    let res = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let res = std::process::Command::new("xdg-open").arg(&url).spawn();
    res.map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(unix)]
fn is_stdin_piped_or_redirected() -> bool {
    unsafe {
        if libc::isatty(libc::STDIN_FILENO) != 0 {
            return false;
        }
        let mut st: libc::stat = std::mem::zeroed();
        if libc::fstat(libc::STDIN_FILENO, &mut st) != 0 {
            return false;
        }
        let fmt = st.st_mode & libc::S_IFMT;
        fmt == libc::S_IFIFO || fmt == libc::S_IFREG
    }
}

#[cfg(not(unix))]
fn is_stdin_piped_or_redirected() -> bool {
    false
}

#[cfg(unix)]
fn is_stdout_piped_or_redirected() -> bool {
    unsafe {
        if libc::isatty(libc::STDOUT_FILENO) != 0 {
            return false;
        }
        let mut st: libc::stat = std::mem::zeroed();
        if libc::fstat(libc::STDOUT_FILENO, &mut st) != 0 {
            return false;
        }
        let fmt = st.st_mode & libc::S_IFMT;
        fmt == libc::S_IFIFO || fmt == libc::S_IFREG
    }
}

#[cfg(not(unix))]
fn is_stdout_piped_or_redirected() -> bool {
    false
}

#[cfg(unix)]
fn is_terminal_session() -> bool {
    unsafe {
        libc::isatty(libc::STDIN_FILENO) != 0
            || libc::isatty(libc::STDOUT_FILENO) != 0
            || libc::isatty(libc::STDERR_FILENO) != 0
            || std::env::var_os("TERM").is_some()
    }
}

#[cfg(not(unix))]
fn is_terminal_session() -> bool {
    false
}

#[cfg(unix)]
fn send_ipc_sync(method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    use std::os::unix::net::UnixStream;
    let sock_path = get_ipc_socket_path();
    let mut stream = UnixStream::connect(&sock_path).map_err(|e| {
        format!(
            "Cannot connect to mdvim at {:?}: {e}\nMake sure mdvim is running.",
            sock_path
        )
    })?;
    let payload = serde_json::json!({
        "method": method,
        "params": params
    });
    let line = format!("{payload}\n");
    stream
        .write_all(line.as_bytes())
        .map_err(|e| format!("Failed to write to IPC socket: {e}"))?;
    stream
        .flush()
        .map_err(|e| format!("Failed to flush IPC socket: {e}"))?;

    let mut reader = BufReader::new(stream);
    let mut resp_line = String::new();
    reader
        .read_line(&mut resp_line)
        .map_err(|e| format!("Failed to read IPC response: {e}"))?;
    let trimmed = resp_line.trim();
    if trimmed.is_empty() {
        return Err("Empty response from mdvim IPC server".into());
    }
    serde_json::from_str(trimmed).map_err(|e| format!("Invalid JSON response from IPC: {e}"))
}

#[cfg(not(unix))]
fn send_ipc_sync(_method: &str, _params: serde_json::Value) -> Result<serde_json::Value, String> {
    Err("Unix domain socket IPC is only supported on Unix platforms".into())
}

fn build_entries_from_parsed(
    parsed: &cli::ParsedCliArgs,
    stdin_content: Option<String>,
) -> Vec<CliFileEntry> {
    let mut entries = Vec::new();
    let (global_line, jump_last_line, search_pattern) = match &parsed.cursor_target {
        Some(CursorTarget::Line(n)) => (Some(*n), false, None),
        Some(CursorTarget::LastLine) => (None, true, None),
        Some(CursorTarget::Pattern(p)) => (None, false, Some(p.clone())),
        None => (None, false, None),
    };

    for f in &parsed.files {
        if f.path.is_dir() {
            continue;
        }
        let exists = f.path.exists();
        let (content, mtime_ms) = if exists {
            (
                std::fs::read_to_string(&f.path).unwrap_or_default(),
                file_mtime_ms(&f.path),
            )
        } else {
            (String::new(), 0)
        };
        let path_str = f.path.to_string_lossy().to_string();
        let title = f
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());

        entries.push(CliFileEntry {
            path: Some(path_str),
            title,
            content,
            mtime_ms,
            exists,
            is_stdin: false,
            readonly: parsed.readonly,
            view_mode: parsed.view_mode.clone(),
            target_line: f.line.or(global_line),
            target_col: f.col,
            jump_last_line,
            search_pattern: search_pattern.clone(),
            commands: parsed.commands.clone(),
            wait_token: None,
        });
    }

    if let Some(stdin_str) = stdin_content {
        entries.push(CliFileEntry {
            path: None,
            title: Some("[stdin]".to_string()),
            content: stdin_str,
            mtime_ms: 0,
            exists: false,
            is_stdin: true,
            readonly: parsed.readonly,
            view_mode: parsed.view_mode.clone(),
            target_line: global_line,
            target_col: None,
            jump_last_line,
            search_pattern,
            commands: parsed.commands.clone(),
            wait_token: None,
        });
    }

    entries
}

fn run_ctl_cli(ctl_args: &[String], caller_cwd: &Path) -> i32 {
    let sub = ctl_args.first().map(|s| s.as_str()).unwrap_or("help");
    match sub {
        "-h" | "--help" | "help" => {
            cli::print_cli_help();
            0
        }
        "ping" => match send_ipc_sync("ping", serde_json::Value::Null) {
            Ok(res) => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&res).unwrap_or_default()
                );
                0
            }
            Err(e) => {
                eprintln!("Error: {e}");
                1
            }
        },
        "get-state" => match send_ipc_sync("get-state", serde_json::Value::Null) {
            Ok(res) => {
                let st = res.get("state").cloned().unwrap_or(serde_json::Value::Null);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&st).unwrap_or_default()
                );
                0
            }
            Err(e) => {
                eprintln!("Error: {e}");
                1
            }
        },
        "list-commands" => match send_ipc_sync("list-commands", serde_json::Value::Null) {
            Ok(res) => {
                if res.get("status").and_then(|s| s.as_str()) == Some("ok") {
                    let cmds = res
                        .get("commands")
                        .cloned()
                        .unwrap_or(serde_json::Value::Array(vec![]));
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&cmds).unwrap_or_default()
                    );
                    0
                } else {
                    eprintln!(
                        "Error: {}",
                        res.get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("Failed to list commands")
                    );
                    1
                }
            }
            Err(e) => {
                eprintln!("Error: {e}");
                1
            }
        },
        "exec" => {
            let Some(cmd_id) = ctl_args.get(1) else {
                eprintln!("Error: command-id required. Usage: mdvim ctl exec <command-id> [args-json]");
                return 1;
            };
            let parsed_args = ctl_args.get(2).map(|raw| {
                serde_json::from_str::<serde_json::Value>(raw)
                    .unwrap_or_else(|_| serde_json::Value::String(raw.clone()))
            });
            let params = serde_json::json!({
                "id": cmd_id,
                "args": parsed_args
            });
            match send_ipc_sync("exec", params) {
                Ok(res) => {
                    if res.get("status").and_then(|s| s.as_str()) == Some("ok") {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&res).unwrap_or_default()
                        );
                        0
                    } else {
                        eprintln!(
                            "Error executing {}: {}",
                            cmd_id,
                            res.get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("Unknown error")
                        );
                        1
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    1
                }
            }
        }
        "send-keys" => {
            let keys = ctl_args.iter().skip(1).cloned().collect::<Vec<_>>().join(" ");
            if keys.is_empty() {
                eprintln!("Error: keys required. Usage: mdvim ctl send-keys <keys>");
                return 1;
            }
            match send_ipc_sync("send-keys", serde_json::json!({ "keys": keys })) {
                Ok(res) => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&res).unwrap_or_default()
                    );
                    0
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    1
                }
            }
        }
        "insert-text" => {
            let raw = ctl_args.iter().skip(1).cloned().collect::<Vec<_>>().join(" ");
            if raw.is_empty() {
                eprintln!("Error: text required. Usage: mdvim ctl insert-text <text>");
                return 1;
            }
            let text = raw
                .replace("\\r\\n", "\n")
                .replace("\\n", "\n")
                .replace("\\t", "\t");
            match send_ipc_sync("insert-text", serde_json::json!({ "text": text })) {
                Ok(res) => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&res).unwrap_or_default()
                    );
                    0
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    1
                }
            }
        }
        "open" => {
            let sub_parsed = cli::parse_cli_args(&ctl_args[1..], caller_cwd);
            let entries = build_entries_from_parsed(&sub_parsed, None);
            if entries.is_empty() {
                eprintln!("Error: file path required. Usage: mdvim ctl open <file...>");
                return 1;
            }
            match send_ipc_sync(
                "open-files",
                serde_json::json!({ "entries": entries, "wait": sub_parsed.wait }),
            ) {
                Ok(res) => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&res).unwrap_or_default()
                    );
                    0
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    1
                }
            }
        }
        "install-path" => match install_cli_symlink() {
            Ok(msg) => {
                println!("{msg}");
                0
            }
            Err(e) => {
                eprintln!("Error: {e}");
                1
            }
        },
        other => {
            eprintln!("Unknown ctl subcommand: {other}");
            cli::print_cli_help();
            1
        }
    }
}

fn install_cli_symlink() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("Failed to locate executable: {e}"))?;

    #[cfg(unix)]
    {
        let mut candidates: Vec<PathBuf> = Vec::new();

        #[cfg(target_os = "macos")]
        {
            if Path::new("/opt/homebrew/bin").is_dir() {
                candidates.push(PathBuf::from("/opt/homebrew/bin/mdvim"));
            }
            candidates.push(PathBuf::from("/usr/local/bin/mdvim"));
            if let Some(home) = std::env::var_os("HOME") {
                candidates.push(PathBuf::from(home).join(".local/bin/mdvim"));
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            if let Some(home) = std::env::var_os("HOME") {
                candidates.push(PathBuf::from(home).join(".local/bin/mdvim"));
            }
            candidates.push(PathBuf::from("/usr/local/bin/mdvim"));
        }

        let mut last_err = String::new();
        for link_path in candidates {
            if link_path == exe {
                return Ok(format!(
                    "'mdvim' binary is already installed directly at {:?}",
                    exe
                ));
            }
            if let Some(parent) = link_path.parent() {
                if !parent.exists() && std::fs::create_dir_all(parent).is_err() {
                    continue;
                }
            }
            if link_path.exists() || link_path.symlink_metadata().is_ok() {
                let _ = std::fs::remove_file(&link_path);
            }
            match std::os::unix::fs::symlink(&exe, &link_path) {
                Ok(()) => {
                    let link_dir = link_path
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let path_env = std::env::var("PATH").unwrap_or_default();
                    let in_path = path_env.split(':').any(|p| p == link_dir);
                    if in_path {
                        return Ok(format!(
                            "Installed CLI symlink: {} -> {}",
                            link_path.display(),
                            exe.display()
                        ));
                    } else {
                        return Ok(format!(
                            "Installed CLI symlink: {} -> {}\nNote: Add `{}` to your PATH (e.g. `export PATH=\"{}:$PATH\"`).",
                            link_path.display(),
                            exe.display(),
                            link_dir,
                            link_dir
                        ));
                    }
                }
                Err(e) => {
                    last_err = format!("{:?}: {e}", link_path);
                }
            }
        }
        Err(format!("Failed to create mdvim symlink ({last_err})"))
    }

    #[cfg(not(unix))]
    {
        Ok(format!("Executable is located at {}", exe.display()))
    }
}

fn print_stdout_contents(contents: &[String]) {
    let mut out = std::io::stdout().lock();
    for (idx, item) in contents.iter().enumerate() {
        if idx > 0 && !contents[idx - 1].ends_with('\n') {
            let _ = out.write_all(b"\n");
        }
        let _ = out.write_all(item.as_bytes());
    }
    if let Some(last) = contents.last() {
        if !last.ends_with('\n') && !last.is_empty() {
            let _ = out.write_all(b"\n");
        }
    }
    let _ = out.flush();
}

fn main() {
    #[cfg(target_os = "macos")]
    {
        if let Ok(path) = std::env::var("PATH") {
            let mut parts: Vec<&str> = path.split(':').collect();
            let extra = ["/opt/homebrew/bin", "/usr/local/bin"];
            let mut prefix = Vec::new();
            for p in extra {
                if !parts.contains(&p) {
                    prefix.push(p);
                }
            }
            if !prefix.is_empty() {
                prefix.extend(parts);
                std::env::set_var("PATH", prefix.join(":"));
            }
        } else {
            std::env::set_var(
                "PATH",
                "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
            );
        }
    }

    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let caller_cwd = detect_default_launch_dir();
    let parsed = cli::parse_cli_args(&raw_args, &caller_cwd);

    if parsed.help {
        cli::print_cli_help();
        std::process::exit(0);
    }

    if parsed.version {
        println!("mdvim {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    if parsed.install_path {
        match install_cli_symlink() {
            Ok(msg) => {
                println!("{msg}");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
    }

    if let Some(ref ctl_args) = parsed.ctl_args {
        let code = run_ctl_cli(ctl_args, &caller_cwd);
        std::process::exit(code);
    }

    // Check if this process is the spawned GUI child process (`--_gui-launch <spec_path>`)
    let launch_spec: LaunchSpec = if let Some(ref spec_path) = parsed.gui_launch_spec {
        let spec = std::fs::read_to_string(spec_path)
            .ok()
            .and_then(|s| serde_json::from_str::<LaunchSpec>(&s).ok())
            .unwrap_or_default();
        let _ = std::fs::remove_file(spec_path);
        spec
    } else {
        // We are in the primary CLI / launcher entry point
        let auto_stdin =
            !parsed.read_stdin && parsed.files.is_empty() && is_stdin_piped_or_redirected();
        let mut stdin_content: Option<String> = None;
        if parsed.read_stdin || auto_stdin {
            let mut buf = String::new();
            if std::io::stdin().read_to_string(&mut buf).is_ok()
                && (parsed.read_stdin || !buf.is_empty())
            {
                stdin_content = Some(buf);
            }
        }

        let has_stdin = stdin_content.is_some();
        let should_output_stdout =
            parsed.stdout_output || (has_stdin && is_stdout_piped_or_redirected());
        let should_wait = parsed.wait || should_output_stdout;

        let mut entries = build_entries_from_parsed(&parsed, stdin_content);

        // If an existing mdvim instance is running (and `--new-window` was not requested),
        // hand off the request over the IPC socket!
        #[cfg(unix)]
        if !parsed.new_window {
            let sock_path = get_ipc_socket_path();
            if sock_path.exists() && std::os::unix::net::UnixStream::connect(&sock_path).is_ok() {
                if entries.is_empty() && !should_wait {
                    let _ = send_ipc_sync("focus-window", serde_json::Value::Null);
                    std::process::exit(0);
                }

                if entries.is_empty() && should_wait {
                    // `mdvim --wait` with no file arguments: create a blank waited tab
                    entries.push(CliFileEntry {
                        path: None,
                        title: None,
                        content: String::new(),
                        mtime_ms: 0,
                        exists: false,
                        is_stdin: has_stdin,
                        readonly: parsed.readonly,
                        view_mode: parsed.view_mode.clone(),
                        target_line: None,
                        target_col: None,
                        jump_last_line: false,
                        search_pattern: None,
                        commands: parsed.commands.clone(),
                        wait_token: None,
                    });
                }

                match send_ipc_sync(
                    "open-files",
                    serde_json::json!({
                        "entries": entries,
                        "wait": should_wait
                    }),
                ) {
                    Ok(res) => {
                        let aborted = res
                            .get("aborted")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        if aborted {
                            std::process::exit(1);
                        }
                        if should_output_stdout {
                            if let Some(arr) = res.get("contents").and_then(|v| v.as_array()) {
                                let texts: Vec<String> = arr
                                    .iter()
                                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                    .collect();
                                print_stdout_contents(&texts);
                            }
                        }
                        std::process::exit(0);
                    }
                    Err(_) => {
                        let _ = std::fs::remove_file(&sock_path);
                    }
                }
            } else if sock_path.exists() {
                let _ = std::fs::remove_file(&sock_path);
            }
        }

        // No running instance found (or `--new-window` requested).
        // If invoked from a terminal (and not `cargo run` / `tauri dev`), spawn the GUI child with
        // stdio redirected to /dev/null so GTK/WebKit logs don't pollute the terminal or stdout pipe.
        let in_cargo_dev = std::env::var_os("CARGO").is_some();
        if should_wait || (!in_cargo_dev && is_terminal_session()) {
            if let Ok(exe) = std::env::current_exe() {
                let pid = std::process::id();
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let spec_path =
                    std::env::temp_dir().join(format!("mdvim-launch-{}-{}.json", pid, ts));
                let wait_out_path = if should_wait {
                    Some(std::env::temp_dir().join(format!("mdvim-wait-{}-{}.json", pid, ts)))
                } else {
                    None
                };

                let spec = LaunchSpec {
                    entries: entries.clone(),
                    wait_out_path: wait_out_path.clone(),
                    launch_cwd: Some(caller_cwd.clone()),
                };

                if let Ok(spec_json) = serde_json::to_string(&spec) {
                    if std::fs::write(&spec_path, spec_json).is_ok() {
                        let mut cmd = Command::new(exe);
                        cmd.arg("--_gui-launch")
                            .arg(&spec_path)
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null());

                        #[cfg(unix)]
                        if !should_wait {
                            use std::os::unix::process::CommandExt;
                            unsafe {
                                cmd.pre_exec(|| {
                                    libc::setsid();
                                    Ok(())
                                });
                            }
                        }

                        if let Ok(mut child) = cmd.spawn() {
                            if let Some(wait_path) = wait_out_path {
                                loop {
                                    if wait_path.exists() {
                                        break;
                                    }
                                    if let Ok(Some(_status)) = child.try_wait() {
                                        break;
                                    }
                                    std::thread::sleep(std::time::Duration::from_millis(30));
                                }

                                let payload = std::fs::read_to_string(&wait_path)
                                    .ok()
                                    .and_then(|s| serde_json::from_str::<WaitResultPayload>(&s).ok());
                                let _ = std::fs::remove_file(&wait_path);

                                if let Some(p) = payload {
                                    if p.aborted {
                                        std::process::exit(1);
                                    }
                                    if should_output_stdout {
                                        print_stdout_contents(&p.contents);
                                    }
                                }
                                std::process::exit(0);
                            } else {
                                // Wait briefly (up to 1500ms) for the child to bind the IPC socket
                                // so back-to-back CLI commands (`mdvim a.md && mdvim b.md`) connect cleanly.
                                let sock = get_ipc_socket_path();
                                for _ in 0..50 {
                                    #[cfg(unix)]
                                    if std::os::unix::net::UnixStream::connect(&sock).is_ok() {
                                        break;
                                    }
                                    #[cfg(not(unix))]
                                    if sock.exists() {
                                        break;
                                    }
                                    if let Ok(Some(_)) = child.try_wait() {
                                        break;
                                    }
                                    std::thread::sleep(std::time::Duration::from_millis(30));
                                }
                                std::process::exit(0);
                            }
                        }
                    }
                }
            }
        }

        LaunchSpec {
            entries,
            wait_out_path: None,
            launch_cwd: Some(caller_cwd),
        }
    };

    let initial = vec![String::new()];
    let session = NvimSession::new(initial).expect("Failed to start embedded Neovim");

    let reported_state = Arc::new(LastReportedState::new());
    let pending_requests = Arc::new(PendingRequests(Mutex::new(HashMap::new())));
    let wait_manager = Arc::new(WaitManager::new(launch_spec.wait_out_path.clone()));

    let mut init_entries = launch_spec.entries;
    if launch_spec.wait_out_path.is_some() {
        if init_entries.is_empty() {
            init_entries.push(CliFileEntry {
                path: None,
                title: None,
                content: String::new(),
                mtime_ms: 0,
                exists: false,
                is_stdin: false,
                readonly: false,
                view_mode: None,
                target_line: None,
                target_col: None,
                jump_last_line: false,
                search_pattern: None,
                commands: Vec::new(),
                wait_token: None,
            });
        }
        for (idx, entry) in init_entries.iter_mut().enumerate() {
            let tok = format!("init-wait-{}", idx);
            entry.wait_token = Some(tok.clone());
            let _ = wait_manager.register_token(tok, entry.content.clone(), true);
        }
    }

    let launch_cwd = launch_spec
        .launch_cwd
        .unwrap_or_else(detect_default_launch_dir);
    let initial_launch_state = Arc::new(InitialLaunchState {
        entries: Mutex::new(init_entries),
        launch_cwd,
    });

    let rep_for_setup = reported_state.clone();
    let pend_for_setup = pending_requests.clone();
    let wmgr_for_setup = wait_manager.clone();

    let rep_for_win = reported_state.clone();
    let wmgr_for_win = wait_manager.clone();
    let init_for_win = initial_launch_state.clone();

    let rep_for_run = reported_state.clone();
    let wmgr_for_run = wait_manager.clone();
    let init_for_run = initial_launch_state.clone();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState(Mutex::new(session)))
        .manage(reported_state.clone())
        .manage(pending_requests.clone())
        .manage(wait_manager.clone())
        .manage(initial_launch_state)
        .invoke_handler(tauri::generate_handler![
            nvim_get_state,
            nvim_input,
            nvim_exec_command,
            nvim_paste_text,
            nvim_set_register,
            nvim_set_buffer,
            ctl_report_state,
            ctl_response,
            app_complete_wait_tab,
            app_exit,
            app_install_cli_path,
            app_load_config,
            app_save_config,
            app_read_file,
            app_write_file,
            app_save_draft,
            app_next_untitled_name,
            app_check_file_modified,
            app_get_cli_files,
            app_get_recent_files,
            app_open_url
        ])
        .setup(move |app| {
            let _bound_socket = start_ipc_server(
                app.handle().clone(),
                rep_for_setup,
                pend_for_setup,
                wmgr_for_setup,
            );
            Ok(())
        })
        .on_window_event(move |window, event| match event {
            tauri::WindowEvent::CloseRequested { .. } => {
                save_dirty_tabs_on_close(&rep_for_win, &init_for_win.launch_cwd);
                wmgr_for_win.flush_all(&rep_for_win, false);
            }
            tauri::WindowEvent::Destroyed => {
                use tauri::Manager;
                save_dirty_tabs_on_close(&rep_for_win, &init_for_win.launch_cwd);
                wmgr_for_win.flush_all(&rep_for_win, false);
                window.app_handle().exit(0);
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("error while building mdvim");

    app.run(move |_app_handle, event| {
        if matches!(
            event,
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
        ) {
            save_dirty_tabs_on_close(&rep_for_run, &init_for_run.launch_cwd);
            wmgr_for_run.flush_all(&rep_for_run, false);
        }
    });
}
