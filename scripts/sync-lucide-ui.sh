#!/usr/bin/env bash
# Met à jour ui/vendor/lucide.min.js (bundle UMD, 100 % offline).
# Usage : ./scripts/sync-lucide-ui.sh [version]   défaut : 0.462.0
set -euo pipefail
VER="${1:-0.462.0}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${ROOT}/ui/vendor/lucide.min.js"
mkdir -p "$(dirname "$OUT")"
URL="https://cdn.jsdelivr.net/npm/lucide@${VER}/dist/umd/lucide.min.js"
echo "Fetching ${URL}"
curl -fsSL "$URL" -o "$OUT"
wc -c "$OUT"
echo "Wrote $OUT"
