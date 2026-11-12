use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum CursorTarget {
    Line(usize),        // 1-based line number (+42)
    LastLine,           // bare +
    Pattern(String),    // +/pattern
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FileArgSpec {
    pub path: PathBuf,
    pub line: Option<usize>, // 1-based line from file:line
    pub col: Option<usize>,  // 1-based col from file:line:col
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ParsedCliArgs {
    pub help: bool,
    pub version: bool,
    pub install_path: bool,
    pub ctl_args: Option<Vec<String>>,
    pub gui_launch_spec: Option<PathBuf>,
    pub readonly: bool,
    pub wait: bool,
    pub stdout_output: bool,
    pub new_window: bool,
    pub view_mode: Option<String>, // "rendered" | "code"
    pub cursor_target: Option<CursorTarget>,
    pub commands: Vec<String>,
    pub read_stdin: bool,
    pub files: Vec<FileArgSpec>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CliFileEntry {
    pub path: Option<String>,
    pub title: Option<String>,
    pub content: String,
    pub mtime_ms: u64,
    pub exists: bool,
    pub is_stdin: bool,
    pub readonly: bool,
    pub view_mode: Option<String>,
    pub target_line: Option<usize>,
    pub target_col: Option<usize>,
    pub jump_last_line: bool,
    pub search_pattern: Option<String>,
    pub commands: Vec<String>,
    pub wait_token: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct LaunchSpec {
    pub entries: Vec<CliFileEntry>,
    pub wait_out_path: Option<PathBuf>,
    pub launch_cwd: Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WaitResultPayload {
    pub aborted: bool,
    pub contents: Vec<String>,
}

/// Lexically normalizes `.` and `..` without requiring the target file to exist on disk.
pub fn normalize_lexical_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                match out.components().next_back() {
                    Some(Component::Normal(_)) => {
                        out.pop();
                    }
                    Some(Component::RootDir) => {}
                    _ => {
                        out.push(comp.as_os_str());
                    }
                }
            }
            _ => out.push(comp.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

/// Expands `~` and resolves relative paths against `base_cwd`.
pub fn resolve_path_from_cwd(raw: &str, base_cwd: &Path) -> PathBuf {
    let trimmed = raw.trim();
    let expanded = if trimmed == "~" {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("~"))
    } else if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            PathBuf::from(home).join(rest)
        } else {
            PathBuf::from(trimmed)
        }
    } else {
        PathBuf::from(trimmed)
    };

    let abs = if expanded.is_absolute() {
        expanded
    } else {
        base_cwd.join(expanded)
    };

    normalize_lexical_path(&abs)
}

/// Parses `file.md`, `file.md:42`, or `file.md:42:10` against `base_cwd`.
/// If the literal path (including colons) exists on disk, keeps it as-is.
pub fn parse_file_arg(raw: &str, base_cwd: &Path) -> FileArgSpec {
    let direct = resolve_path_from_cwd(raw, base_cwd);
    if direct.exists() {
        return FileArgSpec {
            path: direct,
            line: None,
            col: None,
        };
    }

    // Try matching `<path>:<line>:<col>` or `<path>:<line>`
    if let Some((left, right)) = raw.rsplit_once(':') {
        if !left.is_empty() && !left.ends_with('/') && !left.ends_with('\\') {
            if let Ok(num2) = right.parse::<usize>() {
                if num2 > 0 {
                    // Check if `left` also ends with `:<line>`
                    if let Some((file_part, line_part)) = left.rsplit_once(':') {
                        if !file_part.is_empty() {
                            if let Ok(num1) = line_part.parse::<usize>() {
                                if num1 > 0 {
                                    // Avoid splitting Windows drive letters like `C:10`
                                    if !(cfg!(windows) && file_part.len() == 1) {
                                        return FileArgSpec {
                                            path: resolve_path_from_cwd(file_part, base_cwd),
                                            line: Some(num1),
                                            col: Some(num2),
                                        };
                                    }
                                }
                            }
                        }
                    }

                    if !(cfg!(windows) && left.len() == 1) {
                        return FileArgSpec {
                            path: resolve_path_from_cwd(left, base_cwd),
                            line: Some(num2),
                            col: None,
                        };
                    }
                }
            }
        }
    }

    FileArgSpec {
        path: direct,
        line: None,
        col: None,
    }
}

pub fn parse_cli_args(args: &[String], base_cwd: &Path) -> ParsedCliArgs {
    let mut out = ParsedCliArgs::default();
    if args.is_empty() {
        return out;
    }

    if args[0] == "ctl" {
        out.ctl_args = Some(args[1..].to_vec());
        return out;
    }

    let mut i = 0;
    let mut positional_only = false;

    while i < args.len() {
        let arg = &args[i];

        if positional_only {
            out.files.push(parse_file_arg(arg, base_cwd));
            i += 1;
            continue;
        }

        if arg == "--" {
            positional_only = true;
            i += 1;
            continue;
        }

        if arg == "-" {
            out.read_stdin = true;
            i += 1;
            continue;
        }

        match arg.as_str() {
            "-h" | "--help" => {
                out.help = true;
                i += 1;
            }
            "-v" | "-V" | "--version" => {
                out.version = true;
                i += 1;
            }
            "--install-path" => {
                out.install_path = true;
                i += 1;
            }
            "--_gui-launch" => {
                if i + 1 < args.len() {
                    out.gui_launch_spec = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "-R" | "--readonly" => {
                out.readonly = true;
                i += 1;
            }
            "-w" | "--wait" | "-f" | "--nofork" => {
                out.wait = true;
                i += 1;
            }
            "--stdout" => {
                out.stdout_output = true;
                out.wait = true;
                i += 1;
            }
            "--new-window" => {
                out.new_window = true;
                i += 1;
            }
            "-n" => {
                // Standard Vim flag for noswapfile (mdvim uses noswapfile by default)
                i += 1;
            }
            "--code" => {
                out.view_mode = Some("code".to_string());
                i += 1;
            }
            "--rendered" => {
                out.view_mode = Some("rendered".to_string());
                i += 1;
            }
            "--mode" => {
                if i + 1 < args.len() {
                    let m = args[i + 1].to_ascii_lowercase();
                    if m == "code" || m == "rendered" {
                        out.view_mode = Some(m);
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            _ if arg.starts_with("--mode=") => {
                let m = arg.trim_start_matches("--mode=").to_ascii_lowercase();
                if m == "code" || m == "rendered" {
                    out.view_mode = Some(m);
                }
                i += 1;
            }
            "-c" | "--cmd" => {
                if i + 1 < args.len() {
                    out.commands.push(args[i + 1].clone());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "+" => {
                out.cursor_target = Some(CursorTarget::LastLine);
                i += 1;
            }
            _ if arg.starts_with('+') => {
                let rest = &arg[1..];
                if let Some(pat) = rest.strip_prefix('/') {
                    out.cursor_target = Some(CursorTarget::Pattern(pat.to_string()));
                } else if let Ok(line_num) = rest.parse::<usize>() {
                    if line_num > 0 {
                        out.cursor_target = Some(CursorTarget::Line(line_num));
                    } else {
                        out.cursor_target = Some(CursorTarget::Line(1));
                    }
                } else if !rest.is_empty() {
                    out.commands.push(rest.to_string());
                }
                i += 1;
            }
            _ if arg.starts_with('-') => {
                // Ignore unknown flags (e.g., macOS `-psn_0_...` process serial number arg)
                i += 1;
            }
            _ => {
                out.files.push(parse_file_arg(arg, base_cwd));
                i += 1;
            }
        }
    }

    out
}

pub fn print_cli_help() {
    println!(
        "\
mdvim - Cross-platform Markdown & Text Editor with True Embedded Neovim

USAGE:
  mdvim [OPTIONS] [FILES...]
  mdvim [OPTIONS] -
  mdvim ctl <SUBCOMMAND> [ARGS...]

FILE & STREAM ARGUMENTS:
  <FILE>                 Open file(s) in tabs (supports ~/..., relative paths, new files)
  <FILE>:<LINE>[:<COL>]  Open file and jump directly to line (and column)
  -                      Read markdown/text from standard input (stdin)

VIM OPTIONS:
  +<LINE>                Start at line number <LINE> (e.g. mdvim +42 ~/docs/test.md)
  +                      Start at the last line of the document
  +/<PATTERN>            Start at the first line matching <PATTERN>
  -c <COMMAND>           Execute Vim Ex command or mdvim command after loading
  -R, --readonly         Open in read-only mode (disables auto-save and accidental overwrites)
  -n                     No swapfile (enabled by default in mdvim, accepted for Vim compatibility)

VIEW & PROCESS OPTIONS:
  --rendered             Start in WYSIWYG rendered view mode (default)
  --code                 Start in raw markdown code view mode
  --mode <rendered|code> Set initial view mode explicitly
  -w, --wait, -f         Wait until the opened tab(s)/window close before returning to shell
                         (Use `export EDITOR=\"mdvim --wait\"` for git commit, etc.)
  --stdout               Print final document content to stdout upon closing tab (implies --wait)
                         (Also enabled automatically when `-` stdin is piped to non-TTY stdout)
  --new-window           Open in a separate new window instead of reusing an existing window
  --install-path         Install `mdvim` symlink into user PATH (/usr/local/bin or ~/.local/bin)
  -h, --help             Print this help message and exit
  -v, --version          Print version information and exit

AUTOMATION & AI CONTROL SUBCOMMANDS (`mdvim ctl`):
  mdvim ctl ping                     Check if a running mdvim instance is reachable
  mdvim ctl get-state                Print active tab, markdown, cursor, and Vim mode as JSON
  mdvim ctl list-commands            List all registered commands and shortcuts as JSON
  mdvim ctl exec <id> [args-json]    Execute an mdvim command by ID
  mdvim ctl send-keys <keys>         Send raw Vim key sequence to embedded Neovim
  mdvim ctl insert-text <text>       Insert or stream text into active buffer at cursor
  mdvim ctl open <file...>           Open file(s) in the currently running mdvim window

EXAMPLES:
  mdvim ~/docs/test.md
  mdvim +25 ~/docs/test.md
  mdvim +/TODO ./README.md
  mdvim --code -R ~/docs/spec.md
  cat draft.md | mdvim -
  cat draft.md | mdvim - > edited.md
  mdvim --stdout ~/docs/test.md | pbcopy
  GIT_EDITOR=\"mdvim --wait\" git commit"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_file_and_tilde() {
        let cwd = PathBuf::from("/workspace/proj");
        let args = vec!["./notes.md".to_string(), "~/docs/test.md".to_string()];
        let parsed = parse_cli_args(&args, &cwd);
        assert_eq!(parsed.files.len(), 2);
        assert_eq!(parsed.files[0].path, PathBuf::from("/workspace/proj/notes.md"));
        if let Some(home) = std::env::var_os("HOME") {
            assert_eq!(
                parsed.files[1].path,
                PathBuf::from(home).join("docs/test.md")
            );
        }
    }

    #[test]
    fn test_parse_file_with_line_and_col() {
        let cwd = PathBuf::from("/workspace");
        let args = vec![
            "test.md:42".to_string(),
            "other.md:15:8".to_string(),
        ];
        let parsed = parse_cli_args(&args, &cwd);
        assert_eq!(parsed.files.len(), 2);
        assert_eq!(parsed.files[0].path, PathBuf::from("/workspace/test.md"));
        assert_eq!(parsed.files[0].line, Some(42));
        assert_eq!(parsed.files[0].col, None);

        assert_eq!(parsed.files[1].path, PathBuf::from("/workspace/other.md"));
        assert_eq!(parsed.files[1].line, Some(15));
        assert_eq!(parsed.files[1].col, Some(8));
    }

    #[test]
    fn test_parse_vim_flags_and_options() {
        let cwd = PathBuf::from("/workspace");
        let args = vec![
            "+120".to_string(),
            "-R".to_string(),
            "--code".to_string(),
            "--wait".to_string(),
            "-c".to_string(),
            "normal! zz".to_string(),
            "doc.md".to_string(),
        ];
        let parsed = parse_cli_args(&args, &cwd);
        assert_eq!(parsed.cursor_target, Some(CursorTarget::Line(120)));
        assert!(parsed.readonly);
        assert!(parsed.wait);
        assert_eq!(parsed.view_mode.as_deref(), Some("code"));
        assert_eq!(parsed.commands, vec!["normal! zz".to_string()]);
        assert_eq!(parsed.files.len(), 1);
    }

    #[test]
    fn test_parse_pattern_and_last_line_and_stdin() {
        let cwd = PathBuf::from("/workspace");
        let args1 = vec!["+".to_string(), "-".to_string()];
        let parsed1 = parse_cli_args(&args1, &cwd);
        assert_eq!(parsed1.cursor_target, Some(CursorTarget::LastLine));
        assert!(parsed1.read_stdin);

        let args2 = vec!["+/FIXME".to_string(), "--stdout".to_string(), "a.md".to_string()];
        let parsed2 = parse_cli_args(&args2, &cwd);
        assert_eq!(
            parsed2.cursor_target,
            Some(CursorTarget::Pattern("FIXME".to_string()))
        );
        assert!(parsed2.stdout_output);
        assert!(parsed2.wait);
    }

    #[test]
    fn test_parse_ctl_subcommand() {
        let cwd = PathBuf::from("/workspace");
        let args = vec![
            "ctl".to_string(),
            "exec".to_string(),
            "view.toggleMode".to_string(),
        ];
        let parsed = parse_cli_args(&args, &cwd);
        assert_eq!(
            parsed.ctl_args,
            Some(vec!["exec".to_string(), "view.toggleMode".to_string()])
        );
    }
}
