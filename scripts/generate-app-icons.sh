#!/usr/bin/env bash
# Génère les icônes bundle Tauri (toutes plateformes) + copie les PNG UI (favicon / workload).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SRC="${ROOT}/icons/source/soulkernel-app.svg"
if [[ ! -f "$SRC" ]]; then
  echo "missing $SRC" >&2
  exit 1
fi

if ! command -v cargo >/dev/null; then
  echo "cargo not found — install Rust" >&2
  exit 1
fi

echo "==> cargo tauri icon (PNG + ICO + ICNS + Linux hicolor)"
cargo tauri icon "$SRC" -o "$ROOT/icons"

UI_BRAND="${ROOT}/ui/assets/brand"
mkdir -p "$UI_BRAND"
for sz in 16 24 32 48 64 128 256; do
  if [[ -f "${ROOT}/icons/png/icon_${sz}x${sz}.png" ]]; then
    cp "${ROOT}/icons/png/icon_${sz}x${sz}.png" "${UI_BRAND}/icon_${sz}x${sz}.png"
  fi
done
echo "==> synced PNG → ui/assets/brand/ (tailles présentes)"
echo "Done."
