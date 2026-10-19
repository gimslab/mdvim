# mdvim 프로젝트 진행 현황 (Progress Tracker)

> 본 문서는 [SPEC.md](file:///home/gim/projects/mdvim/docs/SPEC.md) 요구사항을 기준으로 **완료된 항목**, **진행 중(부분 구현)인 항목**, **진행 예정인 항목**을 추적 관리하는 문서입니다.
>
> **상태 범례**: `[x]` 완료 · `[-]` 진행 중 / 부분 구현 · `[ ]` 예정

---

## 1. 마일스톤 요약 대시보드

| 영역 | 상태 | 요약 |
|---|---|---|
| **0. 요구사항 정의 & 기술 스파이크** | ✅ 완료 | 37개 문답(`ideas.md`), `SPEC.md` v0.1, Spike 01(투영 엔진), Spike 02(Tauri + Neovim + 한글 IME) |
| **1. 코어 아키텍처 & 기반 구조** | ✅ 완료 | Tauri v2 + Svelte 5 + TS 스캐폴딩, `nvim --embed` 비동기 RPC, 한글 IME 처리, 다국어(ko/en) |
| **2. 커맨드 시스템 & 외부 AI 제어** | ✅ 완료 | 단일 커맨드 레지스트리, 커맨드 팔레트, `config.toml` 단축키, Local IPC + `mdvim ctl` + `SKILL.md` |
| **3. 투영 버퍼 & WYSIWYG 편집 엔진** | ✅ 완료 (기본) | 블록/인라인(굵게·기울임·취소선·코드) 투영, 줄 이탈 즉시 렌더링, 목록 자동 완성, Rich Yank/Paste 완료 |
| **4. 파일 관리 & 에디터 워크플로우** | ✅ 완료 | 열기/저장/다른 이름으로 저장, 자동 저장(3종), `:w`/`:q`/`:wq`/`:q!`, 외부 변경 감지, 최근 파일 완료 |
| **5. Phase 1 (MVP) 마크다운 문법 완성** | 🔄 진행 중 | 취소선·체크박스 단축키 완료, 링크/이미지 팝업 및 클립보드 이미지 붙여넣기, 표/코드블록 예정 |
| **6. Phase 2 (확장 기능 & 배포 자동화)** | ⏳ 예정 | 수식(KaTeX), Mermaid, 위키링크, 콜아웃, 다크 테마/커스텀 CSS, 3대 OS 릴리스 패키징 |

---

## 2. 완료된 항목 (Completed)

### 2.1 기획 및 기술 검증 (Spikes)
- [x] 요구사항 인터뷰(37문항) 및 [ideas.md](file:///home/gim/projects/mdvim/docs/ideas.md) 정리
- [x] 공식 요구사항 및 아키텍처 스펙 [SPEC.md](file:///home/gim/projects/mdvim/docs/SPEC.md) 작성
- [x] **Spike 01 (`spikes/01-projection`)**: 투영 버퍼, 무손실 역변환, 서식 경계 규칙, Rich Yank/Paste 프로토타입 검증
- [x] **Spike 02 (`spikes/02-tauri-ime`)**: Tauri v2 + `nvim --embed` 비동기 RPC 데드락 방지(`nvim_get_mode` 선행 확인) 및 WebKitGTK/IBus 한글 IME 조합 검증

### 2.2 코어 엔진 & Neovim 연동
- [x] **Tauri v2 + Svelte 5 + TypeScript** 프로젝트 초기 구성 및 CI 워크플로우(`.github/workflows`) 기본 설정
- [x] **임베디드 Neovim (`nvim --embed`)** 프로세스 스폰 및 msgpack-RPC 비동기 브리지 구현 ([main.rs](file:///home/gim/projects/mdvim/src-tauri/src/main.rs))
- [x] **`nvim_get_mode()` 논블로킹 가드**: `blocking: true` 상태(`r`, `f`, `t` 등 대기)에서 RPC 데드락 방지
- [x] **한글 IME 조합 처리**:
  - [x] 조합 중(`compositionstart` ~ `compositionupdate`) 인라인 Preedit 오버레이 표시
  - [x] IBus/WebKitGTK 이중 커밋 및 스페이스/엔터 꼬임 방지 처리
  - [x] 조합 도중 `Esc` 입력 시 조합 글자 확정 후 Normal 모드 전환

### 2.3 WYSIWYG 투영 엔진 ( 기본 문법 & Rich Yank/Paste )
- [x] **기본 블록/인라인 마크다운 투영 및 역변환** ([projection/index.ts](file:///home/gim/projects/mdvim/src/lib/projection/index.ts)):
  - [x] 제목(`#` ~ `######`), 인용문(`>`), 목록(`-`, `*`, `+`, `1.`), 체크박스(`- [ ]`, `- [x]`), 수평선(`---`, `***`, `___`)
  - [x] 인라인 서식: 굵게(`**`), 기울임(`*`), 취소선(`~~`), 인라인 코드(`` ` ``)
  - [x] 무변경 줄 바이트 보존 (`dirty === false`일 때 원본 `raw` 그대로 유지)
  - [x] 서식 경계 입력 규칙 (교체 시 첫 글자 서식 상속, 삽입 시 왼쪽 인접 서식 상속, 공백 마커 외부 정규화)
  - [x] 렌더링 뷰에서 마크다운 문법 직접 입력 시 줄 이탈/Normal 모드 전환 즉시 렌더링 (`reparseCompletedLines`)
  - [x] 렌더링 뷰 목록(`-`, `1.`, `- [ ]`) 및 인용(`>`) 줄바꿈 자동 완성 및 빈 항목 해제
  - [x] 체크박스 마우스 클릭 및 단축키(`Ctrl+L`, `format.toggleCheckbox`) 토글 지원
- [x] **서식 보존 복사/붙여넣기 (Rich Yank & Paste)**:
  - [x] `TextYankPost` 이벤트 연동 및 사이드카 레지스터 맵 저장
  - [x] 렌더링 뷰에서 줄 단위(`yy`, `dd`) 및 글자 단위(`yw`, `dw`) 복사/붙여넣기 시 접두어(`# `, `- ` 등)와 인라인 서식 보존
- [x] **뷰 모드 전환**: 단일 패인 렌더링 뷰 ↔ 코드 뷰 실시간 전환 (`Ctrl+E`) 및 마크다운 실시간 원본 인스펙터 패널

### 2.4 커맨드 아키텍처, 설정 & 외부 AI 제어
- [x] **단일 커맨드 레지스트리 (Unified Command Registry)** ([commands/index.ts](file:///home/gim/projects/mdvim/src/lib/commands/index.ts), [defaults.ts](file:///home/gim/projects/mdvim/src/lib/commands/defaults.ts))
- [x] **커맨드 팔레트 (Command Palette)** ([CommandPalette.svelte](file:///home/gim/projects/mdvim/src/components/CommandPalette.svelte)):
  - [x] `Ctrl+Shift+P` / `F1` 호출, 한국어/영어 실시간 퍼지·부분문자열 검색, 단축키 배지 표시
- [x] **사용자 설정 및 단축키 커스터마이징**:
  - [x] `~/.config/mdvim/config.toml` 파서 및 모드별(`normal`, `insert`, `visual`, `always`) 단축키 라우팅 ([config/index.ts](file:///home/gim/projects/mdvim/src/lib/config/index.ts))
- [x] **외부 AI 개발 툴 제어 인터페이스**:
  - [x] Local Unix Domain Socket IPC 서버 (`/tmp/mdvim-{pid}.sock` & `/tmp/mdvim.sock`)
  - [x] CLI 브리지 `bin/mdvim` (`mdvim ctl ping | list-commands | get-state | exec`) 및 멀티라인 텍스트 삽입(`nvim_paste`) 처리
  - [x] 외부 AI 에이전트용 [SKILL.md](file:///home/gim/projects/mdvim/SKILL.md) 작성 및 동기화

### 2.5 파일 입출력, 탭 관리 & Vim Ex 명령줄
- [x] **멀티 탭(Multi-tab) 관리**: 새 탭(`Ctrl+T`), 탭 전환(`Ctrl+Tab`/`Ctrl+Shift+Tab`), 탭 닫기(`Ctrl+W`), 탭별 독립 상태 유지
- [x] **네이티브 파일 열기 / 저장 / 다른 이름으로 저장**:
  - [x] `Ctrl+O` (`file.open`), `Ctrl+S` (`file.save`), `Ctrl+Shift+S` (`file.saveAs`)
  - [x] 수정 여부(`dirty`) 상태바 및 탭 표시기(`•`)
  - [x] 새 문서(`untitled.md`) 첫 저장 시 앱 실행 디렉토리(CWD)를 기본 경로로 사용하며, 동일 이름의 파일이 이미 존재할 경우 대시 숫자 증가(`untitled-1.md`, `untitled-2.md`...)로 자동 생성
- [x] **자동 저장 (Auto-save)**:
  - [x] 타이핑 멈춤 디바운스(1.5초), 창 포커스 이탈(`blur`), 주기적 타이머(30초) 자동 저장
  - [x] 다이얼로그 블로킹 없는 Rust 네이티브 파일 I/O(`app_write_file` / `app_read_file` / `app_save_draft`) 기반 저장 및 `untitled-N.md` 자동 초안 저장
- [x] **Vim 명령줄(`:`) 인터셉트 및 처리**:
  - [x] 하단 상태바 `:` 명령줄 UI 및 `:w`, `:q`, `:wq`, `:q!`, `:qa`, `:qa!`, `:e`, `:tabnew`, `:bn`, `:bp` 지원
  - [x] 백엔드(`nvim_input`)에서 `:q` 계열 명령이 임베디드 Neovim 프로세스를 종료시키지 않도록 안전하게 인터셉트하여 프론트엔드 탭 닫기/파일 저장으로 라우팅
- [x] **외부 파일 변경 감지 (File Watcher)**:
  - [x] 1.5초 주기 및 창 포커스(`focus`) 시 열린 파일의 `mtime` 변경 확인
  - [x] 외부 에디터/AI가 디스크 파일을 수정했을 때 `dirty === false`면 자동 리로드, `dirty === true`면 상단 충돌 알림 배너(디스크 내용으로 다시 불러오기 / 내 변경사항 유지) 표시
- [x] **CLI 실행 인자로 파일 열기**: `mdvim <파일경로>` 실행 시 해당 파일들을 초기 탭으로 자동 로드
- [x] **최근 파일 목록 (Recent Files)**: `~/.config/mdvim/recent.json`에 최근 열거나 저장한 파일(최대 15개)을 자동 기록하고 커맨드 팔레트(`recent.open.*`)에서 즉시 검색·열기 지원

---

## 3. 진행 중 / 보완 필요 항목 (In Progress)

- [-] **통합 Undo / Redo 히스토리 고도화 (`u` / `Ctrl+R`)**
  - 현재 단일 뷰 내 Undo/Redo는 동작하나, 렌더링 뷰 ↔ 코드 뷰를 반복 전환할 때도 Undo 트리가 끊기지 않도록 트랜잭션 동기화 정교화 필요

---

## 4. 진행 예정 항목 (Planned / Backlog)

### 4.1 Phase 1 (MVP) 잔여 과제 — 파일 & 설정
- [ ] **사용자 Neovim 설정(`init.lua` / `.vimrc`) 로드 옵션 및 안전 래퍼**:
  - [ ] 설정에 따라 사용자 `init.lua`를 로드하되 UI/좌표 충돌 설정만 방어하도록 개선

### 4.2 Phase 1 (MVP) 잔여 과제 — 마크다운 문법 및 편집
- [ ] **링크 (`[text](url)`)**:
  - [ ] 렌더링 뷰 인라인 링크 표시 및 `Ctrl+클릭`으로 브라우저 열기
  - [ ] 단축키(`Ctrl+K`, `format.editLink`) 팝업으로 링크 텍스트/URL 편집
- [ ] **이미지 (`![alt](src)`)**:
  - [ ] 로컬/원격 이미지 렌더링 뷰 인라인 표시 및 경로/대체 텍스트 편집 팝업
  - [ ] **클립보드 이미지 붙여넣기**: 이미지를 붙여넣으면 문서 폴더(또는 `./assets/`)에 저장 후 상대 경로 마크다운 자동 삽입
- [ ] **코드 블록 (` ```lang ... ``` `)**:
  - [ ] 멀티라인 펜스드 코드 블록 파싱 및 렌더링 뷰 내 코드 편집 + 문법 하이라이트(Syntax Highlighting)
- [ ] **표 (GFM Table)**:
  - [ ] 멀티라인 GFM 표 파싱 및 렌더링 뷰 셀 단위 편집, 행/열 추가·삭제
- [ ] **Vim 검색 줄 (`/`, `?`) UI 연동**:
  - [ ] 하단 바에서 `/`, `?` 검색어 입력 및 `n`/`N` 매치 하이라이트 연동

### 4.3 Phase 2 (확장 기능)
- [ ] **확장 마크다운 문법**:
  - [ ] 수식 (LaTeX / KaTeX 인라인 `$...$` 및 블록 `$$...$$`)
  - [ ] Mermaid 다이어그램 렌더링
  - [ ] 위키링크 (`[[문서명]]`), 콜아웃 (`> [!NOTE]` 등), 각주 (`[^1]`), YAML 프런트매터 (`---`)
- [ ] **테마 및 스타일 커스터마이징**:
  - [ ] 다크 모드(Dark Theme) 추가 및 라이트/다크 테마 전환 커맨드
  - [ ] 사용자 커스텀 CSS 및 폰트 설정 지원
- [ ] **배포 및 패키징 완성**:
  - [ ] 시스템에 `nvim` 미설치 시 번들링된 Neovim 바이너리 폴백 지원
  - [ ] GitHub Actions 3대 OS(Linux AppImage/deb, macOS dmg, Windows msi/exe) 릴리스 자동화 검증

---

## 5. 최근 작업 이력 (Recent Activity Log)

| 날짜 | 주요 변경 내용 | 관련 커밋 |
|---|---|---|
| 2026-10-19 | 인라인 취소선(`~~text~~`) 투영/렌더링/단축키(`Ctrl+D`), 체크박스 토글 단축키(`Ctrl+L`), 최근 파일 목록(`recent.json`) 구현 | `HEAD` |
| 2026-10-19 | 외부 파일 변경 감지(File Watcher) 및 충돌 배너, CLI 파일 인자 로드, 렌더링 뷰 즉시 변환 및 목록 자동 완성 구현 | `c82aaf1` |
| 2026-10-19 | 실행 디렉토리에 동일 파일명 존재 시 `untitled-N.md` 대시 숫자 자동 증가 생성 | `153c1e8` |
| 2026-10-19 | 새 문서(`untitled.md`) 기본 저장 위치를 앱 실행 디렉토리(CWD)로 변경 | `bb3ad31` |
| 2026-10-19 | Rust 네이티브 파일 I/O 기반 자동 저장 및 `untitled.md` 초안 자동 저장으로 dirty 표시기(`•`) 해제 수정 | `d697eb0` |
| 2026-10-19 | 백엔드에서 `:q!` / `:q` / `:wq` / `:w` 인터셉트 (임베디드 Neovim 종료 방지 및 강제 탭 닫기 지원) | `2708e90` |
| 2026-10-19 | 디바운스(1.5초), 포커스 이탈(blur), 주기적(30초) 자동 저장 구현 | `dd640ee` |
| 2026-10-19 | `Ctrl+Shift+P` / `F1` 커맨드 팔레트 복구 및 하단 `:` 명령줄(`:w`, `:q`, `:wq`) 구현 | `7e4aad3` |
| 2026-10-19 | 네이티브 파일 열기/저장/다른 이름으로 저장 및 dirty 상태 표시기 구현 | `ab99b05` |
| 2026-10-18 | `~/.config/mdvim/config.toml` 사용자 단축키 설정 및 모드별 라우팅 구현 | `8fb2bb1` |
| 2026-10-18 | Local IPC 소켓 서버, `mdvim ctl` CLI 브리지(멀티라인 `nvim_paste` 포함), `SKILL.md` 구현 | `98d309a`, `9353fa1` |
| 2026-10-18 | 렌더링 뷰 `yy` 복사 시 마크다운 접두어 및 인라인 서식 보존 | `aaa1b94` |
| 2026-10-18 | 단일 커맨드 레지스트리 및 커맨드 팔레트 UI 구현 | `8a10321` |
| 2026-10-18 | Tauri v2 + Svelte 5 초기 스캐폴딩, 투영 엔진, 한글 IME 및 CommonMark 규칙 적용 | `ddc1c47` ~ `2a20ee8` |
