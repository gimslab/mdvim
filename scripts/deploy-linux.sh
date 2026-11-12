#!/usr/bin/env bash
set -euo pipefail

# Move to project root directory
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

DO_PULL=true
BUILD_DEB=false

for arg in "$@"; do
  case "$arg" in
    --no-pull)
      DO_PULL=false
      ;;
    --deb)
      BUILD_DEB=true
      ;;
    -h|--help)
      echo "Usage: $0 [--no-pull] [--deb]"
      echo "  --no-pull  Skip 'git pull' before building"
      echo "  --deb      Also build Ubuntu/Debian .deb package"
      exit 0
      ;;
  esac
done

if [[ "$DO_PULL" == true ]]; then
  echo "==> Pulling latest changes..."
  git pull --ff-only
fi

if [[ "$BUILD_DEB" == true ]]; then
  echo "==> Building Ubuntu/Linux release & .deb package..."
  npm run tauri build -- --bundles deb
else
  echo "==> Building frontend & release binary..."
  npm run build
  cargo build --release --features tauri/custom-protocol --manifest-path src-tauri/Cargo.toml
fi

BIN_PATH="src-tauri/target/release/mdvim"
if [[ ! -x "$BIN_PATH" ]]; then
  echo "Error: Built binary not found at ${BIN_PATH}" >&2
  exit 1
fi

echo "==> Registering 'mdvim' CLI symlink in PATH..."
"$BIN_PATH" --install-path
