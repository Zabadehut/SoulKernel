#!/usr/bin/env bash
# run.sh — Lance le pipeline MemLab complet et exporte un résumé JSON
# Usage : ./run.sh [dashboard|app|all]  (défaut : all)

set -euo pipefail
cd "$(dirname "$0")"

TARGET="${1:-all}"
REPORTS_DIR="$(pwd)/memlab-reports"
SUMMARY="$REPORTS_DIR/summary.json"

mkdir -p "$REPORTS_DIR"

echo "{ \"started_at\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\", \"target\": \"$TARGET\", \"scenarios\": {} }" > "$SUMMARY"

check_server() {
  local url="$1"
  curl -s --max-time 3 "$url" > /dev/null 2>&1
}

run_scenario() {
  local name="$1"
  local url_env="${2:-}"   # optional env var holding server URL to pre-check
  local script="scenarios/${name}.js"
  local work_dir="$REPORTS_DIR/$name"
  mkdir -p "$work_dir"

  # Vérification de disponibilité avant de lancer MemLab
  if [ -n "$url_env" ]; then
    local server_url="${!url_env:-}"
    # Détermine l'URL par défaut selon le scénario
    if [ -z "$server_url" ]; then
      case "$name" in
        app-shell)       server_url="http://localhost:1420" ;;
        live-dashboard)  server_url="http://localhost:8787" ;;
      esac
    fi
    if ! check_server "$server_url"; then
      echo "⚠  $name : serveur inaccessible sur $server_url — scénario ignoré"
      node -e "
        const fs = require('fs');
        const s = JSON.parse(fs.readFileSync('$SUMMARY', 'utf8'));
        s.scenarios['$name'] = { status: 'skipped', leak_count: 0, reason: 'server_unavailable', url: '$server_url' };
        s.completed_at = new Date().toISOString();
        fs.writeFileSync('$SUMMARY', JSON.stringify(s, null, 2));
      "
      echo "  ↳ scenario=$name  status=skipped"
      return 0
    fi
  fi

  local MEMLAB_BIN
  MEMLAB_BIN="$(pwd)/node_modules/.bin/memlab"
  if [ ! -x "$MEMLAB_BIN" ]; then
    MEMLAB_BIN="$(npm bin)/memlab"
  fi

  echo "▶ memlab run --scenario $script"
  if "$MEMLAB_BIN" run --scenario "$script" --work-dir "$work_dir"; then
    STATUS="ok"
  else
    STATUS="error"
  fi

  # Extraire le résumé du rapport JSON généré par memlab
  REPORT_JSON="$work_dir/leaks.json"
  LEAK_COUNT=0
  if [ -f "$REPORT_JSON" ]; then
    LEAK_COUNT=$(node -e "
      const r = require('$REPORT_JSON');
      const leaks = Array.isArray(r) ? r : (r.leaks || []);
      console.log(leaks.length);
    " 2>/dev/null || echo 0)
  fi

  # Mise à jour du résumé global
  node -e "
    const fs = require('fs');
    const s = JSON.parse(fs.readFileSync('$SUMMARY', 'utf8'));
    s.scenarios['$name'] = { status: '$STATUS', leak_count: $LEAK_COUNT, work_dir: '$work_dir' };
    s.completed_at = new Date().toISOString();
    fs.writeFileSync('$SUMMARY', JSON.stringify(s, null, 2));
  "

  echo "  ↳ scenario=$name  status=$STATUS  leaks=$LEAK_COUNT"
}

case "$TARGET" in
  dashboard) run_scenario "live-dashboard" "SOULKERNEL_DASHBOARD_URL" ;;
  app)       run_scenario "app-shell"      "SOULKERNEL_APP_URL"       ;;
  all)
    run_scenario "live-dashboard" "SOULKERNEL_DASHBOARD_URL"
    run_scenario "app-shell"      "SOULKERNEL_APP_URL"
    ;;
  *)
    echo "Usage: $0 [dashboard|app|all]"
    exit 1
    ;;
esac

echo ""
echo "Résumé : $SUMMARY"
cat "$SUMMARY"
