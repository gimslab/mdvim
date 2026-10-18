#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rmpv::{decode::read_value, encode::write_value, Value};
use serde::Serialize;
use std::io::{BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

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
    last_yank: Option<YankEvent>,
}

impl NvimSession {
    fn new(initial_lines: Vec<String>) -> Result<Self, String> {
        let mut child = Command::new("nvim")
            .args(["--embed", "--headless", "-n"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
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
        session.set_lines(&initial_lines)?;

        Ok(session)
    }

    fn call(&mut self, method: &str, params: Vec<Value>) -> Result<Value, String> {
        let msgid = self.next_msgid;
        self.next_msgid += 1;

        let req = Value::Array(vec![
            Value::from(0),
            Value::from(msgid),
            Value::from(method),
            Value::Array(params),
        ]);

        write_value(&mut self.stdin, &req).map_err(|e| format!("Write error: {e}"))?;
        self.stdin.flush().map_err(|e| format!("Flush error: {e}"))?;

        loop {
            let msg = read_value(&mut self.stdout).map_err(|e| format!("Read error: {e}"))?;
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
                last_yank: None,
            });
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
            last_yank,
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
    s.call("nvim_input", vec![Value::from(keys.as_str())])?;
    s.get_state()
}

#[tauri::command]
async fn nvim_paste_text(text: String, state: tauri::State<'_, AppState>) -> Result<NvimState, String> {
    let mut s = state.0.lock().map_err(|e| e.to_string())?;
    let escaped = text.replace('<', "<lt>");
    s.call("nvim_input", vec![Value::from(escaped.as_str())])?;
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

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::oneshot;

static NEXT_REQ_ID: AtomicU64 = AtomicU64::new(1);

struct PendingRequests(Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>);
struct LastReportedState(Mutex<serde_json::Value>);

#[tauri::command]
async fn ctl_report_state(
    state_json: serde_json::Value,
    reported: tauri::State<'_, Arc<LastReportedState>>,
) -> Result<(), String> {
    let mut st = reported.0.lock().map_err(|e| e.to_string())?;
    *st = state_json;
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
) {
    let socket_path = get_ipc_socket_path();
    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

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
                        let req_id = NEXT_REQ_ID.fetch_add(1, Ordering::SeqCst);

                        let response = match method {
                            "get-state" => {
                                let st = rep.0.lock().map(|g| g.clone()).unwrap_or(serde_json::Value::Null);
                                serde_json::json!({ "status": "ok", "state": st })
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
                                    match tokio::time::timeout(std::time::Duration::from_secs(5), rx).await {
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
}

fn main() {
    let initial = vec![String::new()];
    let session = NvimSession::new(initial).expect("Failed to start embedded Neovim");

    let reported_state = Arc::new(LastReportedState(Mutex::new(serde_json::Value::Null)));
    let pending_requests = Arc::new(PendingRequests(Mutex::new(HashMap::new())));

    let rep_for_setup = reported_state.clone();
    let pend_for_setup = pending_requests.clone();

    tauri::Builder::default()
        .manage(AppState(Mutex::new(session)))
        .manage(reported_state.clone())
        .manage(pending_requests.clone())
        .invoke_handler(tauri::generate_handler![
            nvim_get_state,
            nvim_input,
            nvim_paste_text,
            nvim_set_register,
            nvim_set_buffer,
            ctl_report_state,
            ctl_response
        ])
        .setup(move |app| {
            start_ipc_server(app.handle().clone(), rep_for_setup, pend_for_setup);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running mdvim");
}
