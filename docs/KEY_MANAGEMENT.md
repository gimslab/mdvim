# 저장소 보안 및 키 관리 가이드 (Key Management)

이 문서는 저장소 내 암호화 파일(`docs/ideas.md` 등)을 관리하는 `git-crypt` 키의 보관 및 복구 구조를 설명합니다.

---

## 1. 키 관리 아키텍처

```
[사용자 마스터 비밀번호]
         ↓ (GPG AES-256 대칭 암호화)
[git-crypt-key.gpg] (저장소에 커밋되어 공개 보관)
         ↓ (복호화 파이프라인: gpg -d | git-crypt unlock -)
[git-crypt 키 (메모리)]
         ↓
[docs/ideas.md 복호화 (로컬 평문 작업)]
```

- **키 파일 분실 방지**: 물리적인 키 파일(`~/.mdvim-git-crypt.key`)을 잃어버리거나 다른 PC로 이동하더라도, **마스터 비밀번호 하나만 기억하면 언제든 복원**할 수 있습니다.
- **보안성**: `git-crypt-key.gpg`는 군사 등급의 AES-256 대칭 암호화로 보호되므로 저장소(GitHub)에 공개되어도 비밀번호 없이는 복호화가 불가능합니다.

---

## 2. 새 환경 / 키 분실 시 잠금 해제 (Unlock)

저장소를 새로 클론했거나 로컬 키 파일이 없을 때, 다음 명령어 한 줄로 즉시 잠금을 해제할 수 있습니다:

```bash
gpg -d git-crypt-key.gpg | git-crypt unlock -
```

1. 위 명령어를 실행하면 GPG 마스터 비밀번호 입력창이 나타납니다.
2. 마스터 비밀번호를 입력하면 `git-crypt`가 메모리 상에서 바로 키를 전달받아 저장소의 암호화 파일들을 복호화합니다.
3. 복호화가 완료되면 평소처럼 일반 텍스트/마크다운으로 작업할 수 있습니다.

---

## 3. 키 파일 생성 및 GPG 암호화 방법 (참고용)

저장소 루트의 `git-crypt-key.gpg`는 아래 명령어로 생성되었습니다:

```bash
# git-crypt 대칭키를 파이프로 GPG에 전달하여 AES-256 암호화 파일 생성
git-crypt export-key - | gpg --symmetric --cipher-algo AES256 --output git-crypt-key.gpg
```

*(실행 시 마스터 비밀번호를 2회 입력하게 됩니다)*

---

## 4. 로컬 키 백업 파일 추출 (선택 사항)

필요에 따라 로컬 파일시스템에 대칭키 파일 자체를 보관하려면:

```bash
git-crypt export-key ~/.mdvim-git-crypt.key
chmod 600 ~/.mdvim-git-crypt.key
```

이 키 파일로 잠금 해제할 때는:
```bash
git-crypt unlock ~/.mdvim-git-crypt.key
```

---

## 5. 보안 수칙

- 평문 키 파일(`*.key`)은 절대로 Git 저장소에 커밋하지 않습니다. (`.gitignore`에 `*.key` 등록됨)
- 마스터 비밀번호는 충분히 길고 안전한 비밀번호(패스프레이즈)를 사용합니다.
