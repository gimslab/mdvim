---
name: mdvim-control
description: "Control and automate the mdvim desktop Markdown & Text editor via Local IPC socket and CLI. Read active buffer/state, execute registered commands, send Neovim key sequences, and stream/insert Markdown content directly."
---

# `mdvim` AI Automation & Control Skill

This skill allows external AI agents (such as Antigravity, Claude Code, Gemini CLI) to inspect, automate, and control a running instance of `mdvim` through its local IPC interface.

## 1. Overview & Connection Architecture

- **Local IPC Socket**:
  - Linux / macOS: `/tmp/mdvim-<UID>.sock` (Unix Domain Socket)
  - Windows: `%TEMP%\mdvim.sock`
- **Protocol**: Single line JSON-RPC newline-delimited messages (`\n`).
- **CLI Bridge**: Native `mdvim ctl <command>` built directly into the `mdvim` binary (or `node bin/mdvim-ctl.js <command>`).

---

## 2. CLI Usage Reference

### 2.0 Opening Files, Stdin/Stdout Pipelines & Vim Flags (`mdvim`)
```bash
# Open file(s) in running window (or launch background window if none running)
mdvim ~/docs/test.md
mdvim file1.md file2.md

# Open at specific line, last line, or search pattern
mdvim +42 ~/docs/test.md
mdvim ~/docs/test.md:42:5
mdvim + ~/docs/test.md
mdvim +/TODO ~/docs/test.md

# Read-only mode, initial view mode, startup Ex command
mdvim -R --code ~/docs/spec.md
mdvim -c "normal! zz" ~/docs/test.md

# Wait for tab/window close (e.g. git commit editor)
mdvim --wait COMMIT_EDITMSG

# Standard input (stdin) and Standard output (stdout) pipeline
cat draft.md | mdvim -
cat draft.md | mdvim - > edited.md
mdvim --stdout ~/docs/test.md | pbcopy
```

### 2.1 Get Editor State (`get-state`)
Inspects active tabs, the raw markdown content, current cursor position, and Vim modal status.

```bash
mdvim ctl get-state
```

**Example JSON Response**:
```json
{
  "activeTab": {
    "id": "tab-1",
    "title": "untitled.md",
    "path": null,
    "viewMode": "rendered",
    "rawMarkdown": "# mdvim 에디터\n\n- 첫 번째 항목\n",
    "dirty": false
  },
  "tabs": [
    {
      "id": "tab-1",
      "title": "untitled.md",
      "path": null,
      "viewMode": "rendered",
      "dirty": false
    }
  ],
  "activeTabIndex": 0,
  "cursor": {
    "row": 0,
    "col": 0
  },
  "mode": "n",
  "blocking": false
}
```

### 2.2 List Available Commands (`list-commands`)
Lists all registered commands in mdvim's unified command registry.

```bash
node bin/mdvim-ctl.js list-commands
```

**Example Output**:
```json
[
  {
    "id": "view.toggleMode",
    "titleKo": "뷰 전환: 렌더링 뷰 ⟷ 코드 뷰 토글",
    "titleEn": "View: Toggle Rendered / Code View",
    "category": "View",
    "defaultShortcut": "Ctrl+E"
  },
  {
    "id": "format.toggleBold",
    "titleKo": "서식: 굵게 (Bold) 토글",
    "titleEn": "Format: Toggle Bold",
    "category": "Format",
    "defaultShortcut": "Ctrl+B"
  },
  {
    "id": "tab.new",
    "titleKo": "탭: 새 문서 탭 열기",
    "titleEn": "Tab: New Tab",
    "category": "Tab",
    "defaultShortcut": "Ctrl+T"
  }
]
```

### 2.3 Execute a Command (`exec`)
Executes any registered command by its ID.

```bash
# Toggle between rendered WYSIWYG and raw code view
node bin/mdvim-ctl.js exec view.toggleMode

# Open a new tab
node bin/mdvim-ctl.js exec tab.new

# Switch to next tab / previous tab
node bin/mdvim-ctl.js exec tab.next
node bin/mdvim-ctl.js exec tab.prev

# Toggle Markdown raw inspector panel
node bin/mdvim-ctl.js exec editor.toggleInspector
```

### 2.4 Send Neovim Key Sequences (`send-keys`)
Sends Vim keys directly to Neovim. Useful for Vim motions, visual selections, and buffer edits.

```bash
# Line yank and paste
node bin/mdvim-ctl.js send-keys yy
node bin/mdvim-ctl.js send-keys p

# Jump to top of document
node bin/mdvim-ctl.js send-keys gg

# Insert text via Vim Insert mode
node bin/mdvim-ctl.js send-keys "i## New Heading<Esc>"
```

### 2.5 Insert / Stream Text (`insert-text`)
Injects text or generated Markdown directly at the current cursor position.

```bash
node bin/mdvim-ctl.js insert-text "안녕하세요, AI가 작성한 마크다운 문단입니다.\n"
```

---

## 3. Registered Core Commands Catalog

| Command ID | Category | Description | Default Key |
|---|---|---|---|
| `view.toggleMode` | `View` | Toggle between Rendered WYSIWYG & Raw Code | `Ctrl+E` |
| `view.setRendered`| `View` | Switch to Rendered WYSIWYG View | - |
| `view.setCode`    | `View` | Switch to Raw Markdown Code View | - |
| `format.toggleBold`| `Format` | Toggle Bold (`**text**`) on word at cursor | `Ctrl+B` |
| `format.toggleItalic`| `Format` | Toggle Italic (`*text*`) on word at cursor | `Ctrl+I` |
| `format.toggleStrikethrough`| `Format` | Toggle Strikethrough (`~~text~~`) on word at cursor | `Ctrl+D` |
| `format.toggleCode`| `Format` | Toggle Inline Code (`` `text` ``) at cursor | - |
| `format.toggleCheckbox`| `Format` | Toggle Task Checkbox (`- [ ]` ⟷ `- [x]`) on current line | `Ctrl+L` |
| `tab.new`         | `Tab` | Open new Untitled Document Tab | `Ctrl+T` |
| `tab.next`        | `Tab` | Switch to next Tab | `Ctrl+Tab` |
| `tab.prev`        | `Tab` | Switch to previous Tab | `Ctrl+Shift+Tab` |
| `tab.close`       | `Tab` | Close active Tab | `Ctrl+W` |
| `app.quit`        | `Editor` | Auto-save dirty tabs and quit mdvim | `Ctrl+Q / Alt+F4` |
| `editor.toggleInspector` | `Editor` | Toggle Markdown Inspector Sidebar | - |
| `file.open`       | `Editor` | Open Markdown / Text file via native dialog | `Ctrl+O` |
| `file.save`       | `Editor` | Save active file | `Ctrl+S` |
| `file.saveAs`     | `Editor` | Save active file as new file | `Ctrl+Shift+S` |
| `cli.installPath` | `Editor` | Install `mdvim` CLI symlink in user PATH | - |
| `recent.open.<N>` | `Editor` | Reopen N-th recently opened/saved file | - |

---

## 4. Best Practices for AI Agents

1. **Check State Before Editing**: Always run `node bin/mdvim-ctl.js get-state` to verify active document content and cursor location.
2. **Preserve Mode Consistency**: If performing editing operations via `send-keys`, ensure the sequence returns to Normal mode with `<Esc>`.
3. **Use `insert-text` for Content Generation**: For large Markdown blocks or AI streaming, prefer `insert-text` over simulated key sequences to avoid character dropping or unintended Vim command execution.
