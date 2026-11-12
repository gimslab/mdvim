# mdvim CLI 지원 요구사항 및 아키텍처 설계서

> **문서 목적**: `mdvim`을 macOS 및 Ubuntu(Linux) 터미널(CLI) 환경에서 `mdvim ~/docs/test.md` 형태로 자연스럽게 실행하고 제어하기 위한 요구사항, 업계 표준 배포/설치 방식, 그리고 Vim 호환 CLI 옵션을 정리한 검토용 문서입니다.

---

## 1. 핵심 요구사항 요약

1. **지원 운영체제**: macOS (OS X) 및 Ubuntu (Linux) 우선 지원
2. **기본 실행 형태**:
   ```bash
   mdvim ~/docs/test.md
   mdvim ./notes.md
   mdvim file1.md file2.md
   ```
   - 터미널 어느 위치에서든 `mdvim <파일경로>` 명령으로 특정 파일을 즉시 열 수 있어야 함.
   - 홈 디렉토리(`~`), 상대 경로(`./`, `../`), 절대 경로(`/...`), 존재하지 않는 새 파일 경로(저장 시 해당 경로로 생성)를 모두 올바르게 해석해야 함.
3. **배포 및 아키텍처 방향**:
   - **별도의 외부 래퍼 스크립트(Shell/Node 스크립트)를 함께 배포하는 방식은 지양**함.
   - CLI를 함께 지원하는 대중적인 모던 GUI 에디터(VS Code, Zed, Sublime Text, Neovide, MacVim 등)의 **일반적인 표준 방식**을 따름.
   - 어디까지가 **애플리케이션 개발 코드(Rust)**에 들어가고, 어디까지가 **설치/배포 과정(패키지/설치 스크립트)**에 들어가는지 명확히 분리함.

---

## 2. 아키텍처 결정: 개발 코드 vs 설치 스크립트 (업계 표준 방식)

> **질문**: *"이것이 설치 스크립트로 들어가야 하는지 아니면 개발 코드에 들어가야 하는지, 별도의 스크립트를 만들어서 함께 배포해야 하는지?"*

### 2.1 모던 GUI + CLI 겸용 에디터들의 작동 원리

GUI 에디터가 CLI 명령어를 제공하는 방식은 크게 두 가지로 나뉩니다.

| 구분 | 대표 앱 | 구조 및 특징 |
|---|---|---|
| **A. Electron 기반 앱** | VS Code (`code`), Atom | 실행 파일 자체가 무거운 Chromium 런타임이라 단순 `--help` 출력이나 파일 전달에도 수백 MB 메모리가 필요함. 어쩔 수 없이 별도의 셸 스크립트(`bin/code`)를 동봉하여 배포함. |
| **B. 네이티브 / Rust 기반 앱 (권장 표준)** | **Zed (`zed`), Neovide (`neovide`), Sublime Text (`subl`), Alacritty** | **단일 네이티브 바이너리(Single Binary)** 자체가 CLI 파서와 GUI 런처 역할을 겸함. 별도의 스크립트 파일 없이 **실행 바이너리 하나**로 동작하며, OS의 `PATH` 경로에 **심볼릭 링크(Symlink)**만 연결함. |

### 2.2 `mdvim`에 가장 적합한 표준 구조: **"단일 바이너리 내장 + 심볼릭 링크"**

`mdvim`은 **Tauri v2 (Rust)** 기반의 네이티브 바이너리 앱이므로 **B안(단일 바이너리 방식)**이 가장 깔끔하고 표준적인 정석입니다. **별도의 래퍼 스크립트를 만들고 배포할 필요가 전혀 없습니다.**

역할 분담은 다음과 같습니다:

```mermaid
flowchart TB
  subgraphINSTALL["1. 설치 단계 (PATH 등록)"]
    DEB["Ubuntu .deb 패키지 설치\n(/usr/bin/mdvim 자동 생성)"]
    MAC["macOS 배포 스크립트 또는 앱 내 메뉴\n(/usr/local/bin/mdvim → mdvim.app/.../MacOS/mdvim 심볼릭 링크)"]
  end

  subgraphCODE["2. 앱 개발 코드 (src-tauri/src/main.rs)"]
    CLI["main() 진입 직후 CLI 인자 파싱 (GUI 초기화 전, < 5ms)"]
    FAST["--help / --version / ctl 명령\n→ 즉시 콘솔 출력 후 프로세스 종료"]
    CHECK{"이미 실행 중인\nmdvim 창이 있는가?\n(/tmp/mdvim-UID.sock)"}
    IPC["기존 창에 IPC로 파일 경로 전달 & 창 포커스\n→ 터미널 프로세스 즉시 종료"]
    SPAWN["새 GUI 창 백그라운드(Detach) 실행\n→ 터미널 프롬프트 즉시 반환\n(--wait 옵션 시에는 창/탭 닫힐 때까지 대기)"]
  end

  DEB --> CLI
  MAC --> CLI
  CLI --> FAST
  CLI --> CHECK
  CHECK -->|"Yes (실행 중)"| IPC
  CHECK -->|"No (미실행)"| SPAWN
```

#### ① 개발 코드(`src-tauri/src/main.rs`)에 들어갈 내용 (핵심 로직 95%)
1. **초고속 CLI 선행 처리**:
   - GUI 윈도우나 임베디드 Neovim을 띄우기 **전**에 `main()` 최상단에서 CLI 인자(`--help`, `--version`, 옵션 플래그, 파일 경로)를 먼저 파싱합니다.
2. **호출 시점의 작업 디렉토리(`PWD`) 기준 절대 경로 변환**:
   - 터미널에서 `mdvim ./test.md` 또는 `mdvim ~/docs/test.md`를 실행한 바로 그 프로세스에서 절대 경로(`/Users/gim/docs/test.md`)로 변환합니다. (이미 떠 있는 GUI 앱은 작업 디렉토리가 다를 수 있기 때문에 진입 시점에 변환하는 것이 핵심입니다.)
3. **단일 인스턴스 감지 및 IPC 전달 (Existing Window Handoff)**:
   - 이미 `mdvim` GUI 창이 열려 있다면(소켓 `/tmp/mdvim-<uid>.sock` 연결 성공 시), 새 앱을 또 띄우지 않고 **실행 중인 창에 IPC(`open-files`)로 파일을 열어 새 탭으로 추가하고 창을 앞으로 가져온 뒤(Focus), 터미널 명령은 즉시 종료**합니다.
4. **터미널 프롬프트 즉시 반환 (Background Detach)**:
   - 실행 중인 `mdvim`이 없을 때 터미널에서 `mdvim ~/docs/test.md`를 치면, GUI 창을 백그라운드(분리된 프로세스)로 띄우고 터미널 프롬프트는 즉시 사용자에게 돌려주며, 불필요한 WebKit/GTK 로그가 터미널을 어지럽히지 않도록 표준 입출력을 분리합니다. (단, `--wait` 옵션 사용 시에는 닫힐 때까지 대기)

#### ② 설치 과정(패키저 / 배포 스크립트 / 앱 커맨드)에 들어갈 내용 (PATH 연결 5%)
별도의 실행 스크립트를 만드는 것이 아니라, **빌드된 `mdvim` 본체 바이너리를 OS의 `PATH` 경로에서 찾을 수 있게 연결**만 해줍니다.

- **Ubuntu (Linux)**:
  - **`.deb` 패키지 설치 시 (`dpkg -i` / `apt`)**: Tauri 번들러가 기본적으로 실행 파일을 **`/usr/bin/mdvim`**에 설치합니다. `/usr/bin`은 모든 Ubuntu의 기본 `PATH`에 포함되어 있으므로 **별도 작업 없이 설치 즉시 `mdvim ~/docs/test.md`가 동작**합니다.
  - **로컬 소스 빌드 시**: 배포/설치 스크립트(`scripts/deploy-linux.sh` 등)에서 `~/.local/bin/mdvim` 또는 `/usr/local/bin/mdvim`에 빌드된 바이너리 심볼릭 링크를 한 줄(`ln -sf`)로 걸어줍니다.
- **macOS (OS X)**:
  - macOS의 앱 번들(`mdvim.app`) 내부에는 이미 실제 네이티브 실행 파일인 **`mdvim.app/Contents/MacOS/mdvim`**이 들어 있습니다.
  - 따라서 두 가지 표준 경로 중 하나로 심볼릭 링크만 걸어주면 됩니다:
    1. **배포 스크립트([scripts/deploy-mac.sh](file:///home/gim/projects/mdvim/scripts/deploy-mac.sh)) 자동 링크**: 앱을 `~/Applications/mdvim.app` 또는 `/Applications/mdvim.app`에 배포할 때 `/usr/local/bin/mdvim`(또는 `~/.local/bin/mdvim`)으로 심볼릭 링크(`ln -sf`)를 자동 생성합니다.
    2. **커맨드 팔레트 내장 명령 (VS Code / Zed 스타일)**: 앱 내 커맨드 팔레트(`Ctrl+Shift+P` / `Cmd+Shift+P`)에 **`CLI: PATH에 'mdvim' 명령어 설치` (`cli.installPath`)** 메뉴를 제공하여, 클릭 한 번으로 심볼릭 링크를 생성할 수 있게 합니다.

---

## 3. CLI 사용 시나리오 및 상세 동작 명세

### 3.1 파일 및 경로 인자 처리

| 실행 명령 예시 | 기대 동작 |
|---|---|
| `mdvim` | 인자 없이 실행. 실행 중인 창이 있으면 포커스하고, 없으면 빈 문서(`untitled.md`)로 앱 실행 |
| `mdvim ~/docs/test.md` | 홈 디렉토리(`~`)가 확장된 절대 경로 파일을 탭으로 열기 |
| `mdvim ./notes.md` / `mdvim ../a.md` | 현재 터미널의 작업 디렉토리(`PWD`) 기준으로 상대 경로를 해석하여 열기 |
| `mdvim a.md b.md c.md` | 여러 파일을 순서대로 각각의 탭으로 열기 (첫 번째 파일 탭 활성화) |
| `mdvim ~/docs/new-file.md` | 아직 디스크에 없는 파일 경로일 경우, 해당 경로가 지정된 빈 탭을 열고 `:w` 또는 저장 시 그 경로에 파일 생성 (상위 디렉토리 자동 생성 포함) |
| `mdvim ~/docs/test.md:45` | (편의 기능) 파일명 뒤에 `:줄번호`(또는 `:줄번호:열번호`)가 붙은 경우 해당 파일을 열고 즉시 45번째 줄로 이동 |

---

## 4. 제안하는 CLI 옵션 (Vim 기본 옵션 + 모던 GUI 에디터 표준 옵션)

Vim 사용자가 습관적으로 입력하는 옵션과 GUI+CLI 에디터에서 필수적으로 쓰이는 옵션들을 중요도 순으로 정리했습니다.

### 4.1 [필수 / 핵심] Vim 커서 이동 및 초기 명령 옵션

Vim에서 파일을 열 때 가장 많이 사용하는 옵션들입니다.

| 옵션 | 사용 예시 | 설명 |
|---|---|---|
| **`+{줄번호}`** | `mdvim +42 ~/docs/test.md` | 파일을 열고 즉시 **42번째 줄**로 커서 이동 및 스크롤 (에러 로그나 검색 결과 이동 시 가장 많이 사용) |
| **`+`** | `mdvim + ~/docs/test.md` | 파일을 열고 문서의 **맨 마지막 줄(`G`)**로 커서 이동 (일지, 회의록, 로그 파일 끝에 이어 쓸 때 사용) |
| **`+/{패턴}`** | `mdvim +/TODO ~/docs/test.md` | 파일을 열고 **`{패턴}`이 처음 등장하는 줄**로 커서 이동 |
| **`-c <명령>`** / **`+{명령}`** | `mdvim -c "normal! zz" test.md` | 파일을 연 직후 지정한 Vim Ex 명령을 실행 |
| **`-R`** / **`--readonly`** | `mdvim -R ~/docs/spec.md` | **읽기 전용(Read-only)** 모드로 열기. 실수로 내용을 수정하거나 자동 저장(Auto-save)이 덮어쓰는 것을 방지 |

### 4.2 [필수 / 핵심] 프로세스 및 창 제어 옵션 (GUI + CLI 표준)

VS Code(`code`), Sublime Text(`subl`), Zed(`zed`), MacVim(`mvim`)에서 공통으로 제공하는 옵션입니다.

| 옵션 | 사용 예시 | 설명 |
|---|---|---|
| **(기본 동작)** | `mdvim test.md` | 이미 열려 있는 `mdvim` 창이 있으면 **기존 창의 새 탭**으로 파일을 열고 터미널은 즉시 종료. 창이 없으면 백그라운드로 GUI를 띄우고 터미널 즉시 반환 |
| **`-w`** / **`--wait`** *(Vim `-f` / `--nofork` 호환)* | `mdvim --wait COMMIT_EDITMSG`<br>`export EDITOR="mdvim -w"` | **해당 파일 탭(또는 창)이 닫힐 때까지 터미널이 종료되지 않고 대기**. `git commit`, `git rebase -i`, `crontab -e` 등의 기본 에디터(`$EDITOR`)로 `mdvim`을 쓸 때 필수 옵션 |
| **`--new-window`** | `mdvim --new-window test.md` | 기존 창에 탭으로 합치지 않고 별도의 새 프로세스/창으로 열기 |

> **참고 (`-n` 플래그 호환성)**: 표준 Vim에서 `-n`은 스왑 파일을 만들지 않는(`noswapfile`) 옵션입니다. `mdvim`은 기본적으로 `noswapfile`로 동작하므로, 사용자가 습관적으로 `mdvim -n test.md`를 입력하더라도 에러를 내지 않고 자연스럽게 수용(무시)하도록 처리합니다.

### 4.3 [권장] `mdvim` 전용 뷰 모드 및 제어 옵션

| 옵션 | 사용 예시 | 설명 |
|---|---|---|
| **`--code`** | `mdvim --code ~/docs/test.md` | 파일을 열 때 초기 뷰 모드를 **코드 뷰(원본 마크다운)**로 시작 |
| **`--rendered`** | `mdvim --rendered ~/docs/test.md` | 파일을 열 때 초기 뷰 모드를 **렌더링 뷰(WYSIWYG)**로 시작 (기본값) |
| **`ctl <하위명령>`** | `mdvim ctl get-state`<br>`mdvim ctl exec view.toggleMode` | 기존 Node.js 스크립트([bin/mdvim-ctl.js](file:///home/gim/projects/mdvim/bin/mdvim-ctl.js))로 분리되어 있던 외부 제어 기능을 `mdvim` 단일 바이너리 안으로 통합 (Node.js 없이도 동작) |
| **`-h`** / **`--help`** | `mdvim --help` | GUI를 띄우지 않고 터미널에 즉시 사용법 출력 후 종료 |
| **`-v`** / **`--version`** | `mdvim --version` | GUI를 띄우지 않고 터미널에 즉시 버전 출력 후 종료 |

### 4.4 [선택 / 추후 확장] 파이프 입력 (`stdin`) 지원

| 옵션 | 사용 예시 | 설명 |
|---|---|---|
| **`-` (하이픈)** | `cat note.md \| mdvim -`<br>`git diff \| mdvim -` | 표준 입력(`stdin`)으로 파이프된 텍스트를 읽어들여 새 임시 탭으로 열기 (Vim의 `\| vim -` 패턴 호환) |

---

## 5. 구현 단계별 계획 (안)

검토 후 진행이 확정되면 다음 순서로 구현을 진행할 예정입니다.

1. **Step 1 — Rust CLI 파서 및 백그라운드 런처 구현 ([src-tauri/src/main.rs](file:///home/gim/projects/mdvim/src-tauri/src/main.rs))**:
   - `main()` 최상단에서 GUI 초기화 전에 CLI 인자 파싱 (`--help`, `--version`, `+line`, `+/pattern`, `-R`, `--code`, `--wait`, `file:line` 등).
   - 터미널에서 일반 실행 시 자식 프로세스를 Detach하여 터미널을 블로킹하지 않고 프롬프트를 즉시 반환하도록 처리 (`--wait` / `-f` 지정 시에는 대기).
2. **Step 2 — 단일 인스턴스 IPC 파일 열기 연동 ([src-tauri/src/main.rs](file:///home/gim/projects/mdvim/src-tauri/src/main.rs) & [src/App.svelte](file:///home/gim/projects/mdvim/src/App.svelte))**:
   - 이미 실행 중인 `mdvim` 소켓(`/tmp/mdvim-<uid>.sock`)이 감지되면 `open-files` IPC 요청(파일 경로 목록, 초기 줄/패턴, 읽기 전용 여부, 뷰 모드 등)을 전송하고 CLI 프로세스 즉시 종료.
   - 프론트엔드(`App.svelte`)에서 `open-files` 수신 시 이미 열린 파일이면 해당 탭으로 전환, 새 파일이면 새 탭으로 추가 후 지정된 줄/패턴으로 커서 이동 및 창 포커스 활성화 (`window.set_focus()`).
   - `--wait` 옵션이 켜진 경우 해당 탭이 닫힐 때(`:q`, `Ctrl+W`, 창 종료) IPC 응답을 돌려주어 터미널 대기를 해제.
3. **Step 3 — `mdvim ctl` 네이티브 통합**:
   - `mdvim ctl ...` 서브커맨드를 Rust 바이너리에서 직접 처리하여 별도의 Node.js 스크립트 없이도 `mdvim` 명령어 하나로 모든 CLI 제어 수행.
4. **Step 4 — macOS & Ubuntu PATH 등록 자동화**:
   - [scripts/deploy-mac.sh](file:///home/gim/projects/mdvim/scripts/deploy-mac.sh)에 `/usr/local/bin/mdvim` (또는 `~/.local/bin/mdvim`) 심볼릭 링크 자동 설정 추가.
   - Ubuntu용 로컬 배포/설치 스크립트(`scripts/deploy-linux.sh`) 추가 및 `.deb` 빌드 확인.
   - 앱 내 커맨드 팔레트에 `cli.installPath` ("PATH에 'mdvim' 명령어 설치") 커맨드 추가.

---

## 6. 검토 및 확인 요청 사항

위 설계 내용 중 아래 사항을 확인해 주시면 감사하겠습니다:

1. **아키텍처 방식**: 별도 래퍼 스크립트 배포 없이 **Rust 단일 바이너리(`mdvim`) + 심볼릭 링크(PATH 등록)** 방식으로 가는 것에 동의하시는지?
2. **기존 창 재사용 동작**: 이미 `mdvim` 창이 떠 있을 때 터미널에서 `mdvim ~/docs/test.md`를 실행하면 **기존 창에 새 탭으로 열리는 방식**(기본값)이 좋은지, 아니면 매번 **별도의 새 창**이 뜨는 것이 좋은지?
3. **CLI 옵션 범위**: 4절에서 제안드린 옵션(`+{줄번호}`, `+`, `+/{패턴}`, `-R`, `-w/--wait`, `--code/--rendered`, `-` 파이프 입력 등) 중 꼭 포함할 것과 제외할 것이 있는지?
