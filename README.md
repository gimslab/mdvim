# mdvim

> **Cross-platform Markdown & Text Editor with True Embedded Neovim and WYSIWYG Projection**
>
> 실제 임베디드 Neovim의 강력한 편집 기능과 직관적인 마크다운 실시간 WYSIWYG 렌더링 뷰를 단일 패인에서 매끄럽게 오가는 크로스플랫폼 데스크톱 에디터입니다.

---

## 목차
- [주요 특징](#주요-특징)
- [시스템 사전 요구사항](#시스템-사전-요구사항)
  - [공통 필수 구성 요소](#공통-필수-구성-요소)
  - [OS별 의존성 패키지 설치](#os별-의존성-패키지-설치)
    - [Linux (Ubuntu / Debian / Fedora / Arch)](#1-linux)
    - [macOS](#2-macos)
    - [Windows](#3-windows)
- [설치 및 실행 가이드](#설치-및-실행-가이드)
  - [소스코드 다운로드 및 준비](#1-저장소-클론-및-패키지-설치)
  - [개발 모드 실행](#2-개발-모드로-실행)
  - [프로덕션 빌드 (설치 프로그램 생성)](#3-프로덕션-앱-패키징-빌드)
- [기본 사용법 및 단축키](#기본-사용법-및-단축키)
- [외부 AI 및 CLI 연동 (`mdvim-ctl`)](#외부-ai-및-cli-연동-mdvim-ctl)
- [문제 해결 (Troubleshooting)](#문제-해결-troubleshooting)

---

## 주요 특징

- **True Neovim Embedding (`nvim --embed`)**: 키바인딩 단순 에뮬레이션이 아닌 실제 Neovim 바이너리를 백엔드에 내장하여 모든 Vim 모션, 텍스트 객체, 매크로, 레지스터, Ex 커맨드, 기존 플러그인 생태계를 지원합니다.
- **단일 패인 코드 뷰 ↔ WYSIWYG 렌더링 뷰 즉각 전환 (`Ctrl+E`)**: 원본 마크다운 문법 기호가 숨겨진 깔끔한 위지윅 상태에서도 Vim의 편집 모션을 보이는 그대로 수행할 수 있습니다.
- **즉시 마크다운 투영 & 목록 자동 완성**: 렌더링 뷰에서 `# 제목`, `**굵게**`, `*기울임*`, `~~취소선~~`, `` `코드` ``, `- [ ] 할 일` 등을 입력한 뒤 줄을 이동하거나 `Esc`를 누르면 즉시 위지윅으로 변환되며, 목록(`-`, `1.`, `- [ ]`) 및 인용문(`>`)에서 `Enter` 입력 시 다음 항목 접두어가 자동 완성됩니다.
- **Rich Yank & Paste**: 렌더링 뷰에서 복사(`yy`, `yw`) 및 잘라내기(`dd`, `dw`), 붙여넣기(`p`) 시 인라인 서식(`**굵게**`, `*기울임*`, `~~취소선~~`, 코드 등)과 블록 서식이 유지됩니다.
- **완벽한 한글 IME 지원**: WebKitGTK / macOS / Windows 환경에서 한글 조합 도중 인라인 Preedit 표시 및 `Esc` 입력 시 깔끔한 조합 확정/Normal 모드 복귀를 보장합니다.
- **멀티 탭, 자동 저장 & 외부 파일 변경 감지**:
  - 탭 기반 문서 편집(`Ctrl+T`, `Ctrl+W`, `Ctrl+Tab`) 및 디바운스(1.5초) / 포커스 이탈(`blur`) / 주기적(30초) 무중단 자동 저장 기능을 제공합니다.
  - 미저장 새 문서(`untitled.md`)는 앱을 실행한 작업 디렉토리(CWD)에 기본 저장되며, 동일 파일명이 존재하면 `untitled-1.md`, `untitled-2.md` 순으로 자동 생성됩니다.
  - 외부 에디터나 AI 도구가 파일을 수정하면 실시간으로 감지하여 자동 리로드하거나 충돌 해결 배너를 표시합니다.
  - 최근 열거나 저장한 파일 목록(`~/.config/mdvim/recent.json`)을 기록하여 커맨드 팔레트(`Ctrl+Shift+P`)에서 즉시 다시 열 수 있습니다.
- **로컬 IPC 기반 AI 제어 (`mdvim-ctl`)**: 에이전트나 CLI 스크립트가 실행 중인 에디터 상태를 조회하고 명령 및 텍스트를 실시간으로 주입할 수 있습니다.

---

## 시스템 사전 요구사항

### 공통 필수 구성 요소

`mdvim`은 [Tauri v2](https://tauri.app/), [Svelte 5](https://svelte.dev/), 그리고 시스템에 설치된 **[Neovim](https://neovim.io/)**을 바탕으로 구동됩니다.

1. **Neovim** (v0.9.0 이상 권장): 터미널에서 `nvim --version` 명령어가 실행 가능해야 합니다.
2. **Node.js** (v18+ 또는 v20/v22 LTS) & **npm**
3. **Rust & Cargo** (최신 안정화 버전): [rustup.rs](https://rustup.rs/)를 통해 설치

---

### OS별 의존성 패키지 설치

#### 1. Linux

Tauri GUI 렌더링을 위한 WebKitGTK 및 시스템 라이브러리가 필요합니다.

##### Ubuntu / Debian 계열 (22.04, 24.04 등)
```bash
sudo apt update
sudo apt install -y \
  pkg-config \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev \
  librsvg2-dev \
  libayatana-appindicator3-dev \
  neovim \
  curl \
  build-essential
```

##### Fedora / RHEL 계열
```bash
sudo dnf install -y \
  pkgconf-pkg-config \
  webkit2gtk4.1-devel \
  gtk3-devel \
  libsoup3-devel \
  javascriptcoregtk4.1-devel \
  librsvg2-devel \
  libayatana-appindicator3-devel \
  neovim
```

##### Arch Linux 계열
```bash
sudo pacman -S --needed \
  base-devel \
  webkit2gtk-4.1 \
  gtk3 \
  libsoup3 \
  neovim \
  rust \
  nodejs \
  npm
```

---

#### 2. macOS

macOS에서는 Xcode 명령줄 도구와 [Homebrew](https://brew.sh/)를 사용하는 것을 권장합니다.

```bash
# 1. Xcode Command Line Tools 설치
xcode-select --install

# 2. Homebrew를 통한 의존성 설치
brew install neovim node rust
```

---

#### 3. Windows

Windows에서는 C++ 빌드 환경과 Neovim 설치가 필요합니다.

1. **C++ 빌드 도구 (Visual Studio C++ Build Tools)**:
   - [Visual Studio Installer](https://visualstudio.microsoft.com/visual-cpp-build-tools/)에서 **"C++를 사용한 데스크톱 개발" (Desktop development with C++)** 워크로드를 설치합니다.
2. **WebView2**:
   - Windows 10(최신 빌드) 및 Windows 11에는 기본 탑재되어 있습니다. 미설치된 경우 [Evergreen Bootstrapper](https://developer.microsoft.com/microsoft-edge/webview2/)를 설치합니다.
3. **패키지 관리자(`winget` 또는 `choco`)를 통한 설치**:
   PowerShell(관리자 권한)에서 다음을 실행합니다.

```powershell
# Neovim 및 Node.js 설치
winget install Neovim.Neovim
winget install OpenJS.NodeJS
winget install Rustlang.Rustup

# 설치 후 터미널을 다시 열어 환경변수(PATH)를 갱신합니다.
nvim --version
cargo --version
node --version
```

---

## 설치 및 실행 가이드

### 1. 저장소 클론 및 패키지 설치

```bash
git clone https://github.com/gimslab/mdvim.git
cd mdvim

# Node.js 의존성 패키지 설치
npm install
```

### 2. 개발 모드로 실행

소스코드 변경 사항이 실시간으로 핫 리로드되는 개발 모드입니다.

```bash
npm run tauri dev
```

> **참고**: 최초 실행 시 Rust 크레이트 컴파일로 인해 몇 분 정도 소요될 수 있습니다.

### 3. 프로덕션 앱 패키징 (빌드)

현재 사용 중인 운영체제에 맞는 단독 실행 파일 및 인스톨러 패키지를 생성합니다.

```bash
npm run tauri build
```

빌드가 완료되면 운영체제별로 아래 디렉토리에 설치 패키지가 생성됩니다.

| OS | 생성되는 패키지 형식 | 빌드 산출물 위치 |
|---|---|---|
| **Linux** | `.deb`, `.AppImage` | `src-tauri/target/release/bundle/deb/`<br>`src-tauri/target/release/bundle/appimage/` |
| **macOS** | `.dmg`, `.app` | `src-tauri/target/release/bundle/dmg/`<br>`src-tauri/target/release/bundle/macos/` |
| **Windows**| `.exe` (NSIS), `.msi` | `src-tauri/target/release/bundle/nsis/`<br>`src-tauri/target/release/bundle/msi/` |

---

## 기본 사용법 및 단축키

### 에디터 모드 및 Vim 조작

- **Normal 모드 (`Esc`)**: Vim 표준 모션 (`h`, `j`, `k`, `l`, `w`, `b`, `gg`, `G`), 서식 단위 편집 (`ciw`, `yap` 등), `ZZ` (저장 후 닫기), `ZQ` (저장하지 않고 강제 닫기)
- **Insert 모드 (`i`, `a`, `o`, `A`, `I`)**: 일반 텍스트 및 마크다운 입력
- **Visual 모드 (`v`, `V`, `Ctrl+v`)**: 범위 선택 및 조작
- **Command-line 모드 (`:`)**:
  - `:w`, `:w <경로>` (저장)
  - `:q` (탭 닫기), `:q!` (저장하지 않고 강제 탭 닫기), `:wq` / `:x` (저장 후 탭 닫기)
  - `:e <경로>` (파일 열기), `:tabnew` (새 탭), `:bn` / `:bp` (다음/이전 탭 전환)

### 주요 기능 단축키

| 단축키 | 동작 모드 | 기능 설명 |
|---|---|---|
| `Ctrl + E` | 항상 (`always`) | **단일 패인 뷰 전환**: 위지윅 렌더링 뷰 ⟷ 원본 마크다운 코드 뷰 토글 |
| `Ctrl + Shift + P` / `F1` | 항상 (`always`) | **커맨드 팔레트**: 전체 명령어 및 최근 파일 검색·실행 (한국어/영어 지원) |
| `Ctrl + T` | 항상 (`always`) | **새 탭**: 새 문서 열기 (`untitled.md`, 중복 시 `untitled-N.md`) |
| `Ctrl + W` | 항상 (`always`) | **탭 닫기**: 현재 활성 탭 닫기 |
| `Ctrl + Tab` | 항상 (`always`) | 다음 탭으로 이동 |
| `Ctrl + Shift + Tab` | 항상 (`always`) | 이전 탭으로 이동 |
| `Ctrl + O` | 항상 (`always`) | 파일 열기 (시스템 네이티브 다이얼로그) |
| `Ctrl + S` | 항상 (`always`) | 현재 파일 저장 |
| `Ctrl + Shift + S` | 항상 (`always`) | 다른 이름으로 저장 |
| `Ctrl + L` | 항상 (`always`) | 현재 줄 **체크박스 (`- [ ]` ⟷ `- [x]`)** 토글 (일반/목록 줄은 체크박스로 변환) |
| `Ctrl + B` | 입력 (`insert`) | 커서 위치 단어 **굵게 (`**bold**`)** 토글 |
| `Ctrl + I` | 입력 (`insert`) | 커서 위치 단어 *기울임 (`*italic*`)* 토글 |
| `Ctrl + D` | 입력 (`insert`) | 커서 위치 단어 ~~취소선 (`~~strikethrough~~`)~~ 토글 |

### 사용자 설정 커스터마이징 (`config.toml`)

키 매핑 및 에디터 설정을 TOML 파일로 커스터마이징할 수 있습니다. 사용자 설정에 지정한 단축키는 기본 단축키 위에 병합(override)됩니다.
- **Linux / macOS**: `~/.config/mdvim/config.toml`
- **Windows**: `%APPDATA%\mdvim\config.toml` (또는 `%USERPROFILE%\.config\mdvim\config.toml`)

---

## 외부 AI 및 CLI 연동 (`mdvim-ctl`)

`mdvim`은 실행 중 로컬 IPC 소켓을 오픈하여 외부 에이전트나 터미널 스크립트와 통신할 수 있습니다.
- **Linux / macOS 소켓**: `/tmp/mdvim-<UID>.sock`
- **Windows 소켓**: `%TEMP%\mdvim.sock`

```bash
# 에디터 상태 확인 (활성 탭, 커서 위치, 문서 내용 등)
node bin/mdvim-ctl.js get-state

# 명령어 목록 조회
node bin/mdvim-ctl.js list-commands

# 명령어 실행 (예: 뷰 토글)
node bin/mdvim-ctl.js exec view.toggleMode

# Neovim 키 시퀀스 전송
node bin/mdvim-ctl.js send-keys "yy p"

# 커서 위치에 마크다운 텍스트 직접 삽입
node bin/mdvim-ctl.js insert-text "## 자동 생성된 헤딩\n"
```

> AI 에이전트(Claude Code, Antigravity, Gemini CLI 등)와의 상세 연동 사양은 [SKILL.md](file:///home/gim/projects/mdvim/SKILL.md)를 참고하세요.

---

## 문제 해결 (Troubleshooting)

### 1. `Failed to spawn nvim: No such file or directory` 에러
- **원인**: 시스템 `PATH`에 `nvim` 실행 바이너리가 없거나 인식되지 않는 경우입니다.
- **해결**:
  - 터미널이나 명령 프롬프트에서 `nvim --version`을 실행하여 Neovim이 정상 설치되어 있는지 확인합니다.
  - Windows의 경우 Neovim 설치 경로(예: `C:\Program Files\Neovim\bin`)가 시스템 환경 변수 `Path`에 등록되어 있는지 확인하고 터미널을 다시 시작하세요.

### 2. Linux에서 빌드 시 `webkit2gtk-4.1` 패키지를 찾을 수 없는 오류
- **원인**: 최신 배포판(Ubuntu 22.04+, Fedora 37+)에서는 `webkit2gtk-4.1`이 사용되며, 이전 버전 패키지(`webkit2gtk-4.0`)와 다릅니다.
- **해결**: [OS별 의존성 패키지 설치](#1-linux) 안내에 따라 `libwebkit2gtk-4.1-dev` 및 관련 라이브러리를 설치하세요.

### 3. Windows에서 빌드 중 C++ 링커(link.exe) 에러 발생
- **해결**: Visual Studio 설치 관리자를 실행하여 **C++를 사용한 데스크톱 개발** 도구가 온전히 설치되어 있는지 확인하고 컴퓨터를 재부팅 후 다시 시도하세요.
