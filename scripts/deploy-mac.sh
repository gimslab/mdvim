#!/usr/bin/env bash
set -euo pipefail

# Move to project root directory
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

APP_NAME="mdvim.app"
DEST_DIR="${HOME}/Applications"
DEST_APP="${DEST_DIR}/${APP_NAME}"

DO_PULL=true
OPEN_AFTER=false

for arg in "$@"; do
  case "$arg" in
    --no-pull)
      DO_PULL=false
      ;;
    --open)
      OPEN_AFTER=true
      ;;
    -h|--help)
      echo "Usage: $0 [--no-pull] [--open]"
      echo "  --no-pull  Skip 'git pull' before building"
      echo "  --open     Launch ${APP_NAME} after deploying"
      exit 0
      ;;
  esac
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Error: This script is intended to run on macOS (Darwin)." >&2
  exit 1
fi

if [[ "$DO_PULL" == true ]]; then
  echo "==> Pulling latest changes..."
  git pull --ff-only
fi

echo "==> Building macOS app bundle..."
npm run tauri build -- --bundles app

# Locate the built .app bundle
BUNDLE_PATH="src-tauri/target/release/bundle/macos/${APP_NAME}"
if [[ ! -d "$BUNDLE_PATH" ]]; then
  # Fallback in case a specific target triple directory was used
  BUNDLE_PATH="$(find src-tauri/target -path "*/release/bundle/macos/${APP_NAME}" -type d | head -n 1 || true)"
fi

if [[ -z "$BUNDLE_PATH" || ! -d "$BUNDLE_PATH" ]]; then
  echo "Error: Could not find built ${APP_NAME} under src-tauri/target." >&2
  exit 1
fi

echo "==> Deploying to ${DEST_APP}..."
mkdir -p "$DEST_DIR"
rm -rf "$DEST_APP"
ditto "$BUNDLE_PATH" "$DEST_APP"

echo "==> Successfully deployed to ${DEST_APP}"

echo "==> Registering 'mdvim' CLI symlink in PATH..."
"${DEST_APP}/Contents/MacOS/mdvim" --install-path || true

if [[ "$OPEN_AFTER" == true ]]; then
  echo "==> Launching ${APP_NAME}..."
  open "$DEST_APP"
fi
