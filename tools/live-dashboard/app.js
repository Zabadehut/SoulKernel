const state = {
  latest: null,
  timeline: [],
  status: null,
};

const headlineCards = document.getElementById('headlineCards');
const charts = document.getElementById('charts');
const snapshotGrid = document.getElementById('snapshotGrid');
const processList = document.getElementById('processList');
const freshnessPill = document.getElementById('freshnessPill');
const statusLine = document.getElementById('statusLine');
const activePath = document.getElementById('activePath');
const archiveCount = document.getElementById('archiveCount');
const sampleCount = document.getElementById('sampleCount');

document.getElementById('refreshBtn').addEventListener('click', refreshAll);

function fmt(value, digits = 1) {
  return value == null || Number.isNaN(Number(value)) ? '—' : Number(value).toFixed(digits);
}

function fmtPct(value, digits = 1) {
  return value == null ? '—' : `${fmt(value, digits)}%`;
}

function fmtWatts(value) {
  return value == null ? '—' : `${fmt(value, 1)} W`;
}

function fmtDate(ts) {
  return ts ? new Date(ts).toLocaleString('fr-FR') : '—';
}

function sparkline(points, color) {
  if (!points.length) return '<div class="empty">Aucune donnée</div>';
  const width = 320;
  const height = 120;
  const values = points.map((p) => p.value).filter((v) => v != null && Number.isFinite(v));
  if (!values.length) return '<div class="empty">Aucune donnée</div>';
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;
  const coords = points
    .map((point, idx) => {
      const x = (idx / Math.max(1, points.length - 1)) * width;
      const y = height - ((point.value - min) / span) * (height - 10) - 5;
      return `${x},${y}`;
    })
    .join(' ');
  return `<svg class="chart-svg" viewBox="0 0 ${width} ${height}" preserveAspectRatio="none">
    <polyline points="${coords}" fill="none" stroke="${color}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"></polyline>
  </svg>`;
}

function card(label, value, sub, accent) {
  return `<div class="card" style="--accent:${accent}">
    <div class="card-label">${label}</div>
    <div class="card-value">${value}</div>
    <div class="card-sub">${sub}</div>
  </div>`;
}

function chartCard(title, unit, color, points) {
  const values = points.map((point) => point.value).filter((value) => value != null && Number.isFinite(value));
  const latest = values.at(-1);
  const avg = values.length ? values.reduce((acc, value) => acc + value, 0) / values.length : null;
  const min = values.length ? Math.min(...values) : null;
  const max = values.length ? Math.max(...values) : null;
  return `<div class="chart-card">
    <div class="chart-title">
      <h4>${title}</h4>
      <div class="chart-meta">${values.length.toLocaleString('fr-FR')} pts<br>dernier ${fmt(latest, 2)} ${unit}</div>
    </div>
    ${sparkline(points, color)}
    <div class="chart-stats">
      <div><span>Moyenne</span><strong>${fmt(avg, 2)} ${unit}</strong></div>
      <div><span>Min</span><strong>${fmt(min, 2)} ${unit}</strong></div>
      <div><span>Max</span><strong>${fmt(max, 2)} ${unit}</strong></div>
    </div>
  </div>`;
}

function toPoints(series, key) {
  return series
    .filter((item) => item[key] != null && Number.isFinite(item[key]))
    .map((item) => ({ ts: item.ts_ms, value: item[key] }));
}

function render() {
  const latest = state.latest?.latest;
  const projection = state.latest?.latest_projection;
  const status = state.status;
  const timeline = state.timeline?.samples || [];

  freshnessPill.textContent = status?.is_fresh ? 'live' : 'stale';
  freshnessPill.style.color = status?.is_fresh ? 'var(--green)' : 'var(--orange)';
  statusLine.textContent = latest
    ? `Dernier tick ${fmtDate(status.latest_sample_ts_ms)} · ${status.power_source || 'source inconnue'} · ${fmtWatts(status.latest_watts)}`
    : 'Aucun tick observability détecté pour le moment.';
  activePath.textContent = status?.observability_path || '—';
  archiveCount.textContent = status ? String(status.archive_count) : '—';
  sampleCount.textContent = status ? String(status.sample_count) : '—';

  headlineCards.innerHTML = [
    card('Watts mur', fmtWatts(projection?.watts), 'Mesure live à la prise', 'var(--green)'),
    card('CPU total', fmtPct(projection?.cpu_pct), 'Charge machine', 'var(--cyan)'),
    card('RAM', fmtPct(projection?.ram_pct), `${fmt(projection?.ram_used_mb, 0)} / ${fmt(projection?.ram_total_mb, 0)} MiB`, '#60a5fa'),
    card('GPU', fmtPct(projection?.gpu_pct), `${fmt(projection?.gpu_power_watts, 1)} W GPU`, '#a78bfa'),
    card('KPI pénalisé', projection?.kpi_penalized == null ? '—' : `${fmt(projection.kpi_penalized, 2)} W/%`, 'Signal runtime', 'var(--orange)'),
    card('Faults', projection?.faults_per_sec == null ? '—' : `${fmt(projection.faults_per_sec, 0)}/s`, 'Pression mémoire', 'var(--red)'),
  ].join('');

  charts.innerHTML = [
    chartCard('Watts', 'W', '#4ade80', toPoints(timeline, 'watts')),
    chartCard('CPU total', '%', '#22d3ee', toPoints(timeline, 'cpu_pct')),
    chartCard('RAM utilisée', '%', '#60a5fa', toPoints(timeline, 'ram_pct')),
    chartCard('GPU', '%', '#a78bfa', toPoints(timeline, 'gpu_pct')),
    chartCard('KPI pénalisé', 'W/%', '#fb923c', toPoints(timeline, 'kpi_penalized')),
    chartCard('Page faults', '/s', '#fb7185', toPoints(timeline, 'faults_per_sec')),
  ].join('');

  snapshotGrid.innerHTML = latest ? [
    ['Workload', latest.report?.workload || '—'],
    ['Dôme', latest.report?.dome_active ? 'Actif' : 'Inactif'],
    ['SoulRAM', latest.report?.soulram_active ? 'Actif' : 'Inactif'],
    ['Cible PID', latest.report?.target_pid ?? '—'],
    ['CPU utile', latest.kpi?.cpu_useful_pct == null ? '—' : `${fmt(latest.kpi.cpu_useful_pct, 1)}%`],
    ['CPU overhead', latest.kpi?.cpu_overhead_pct == null ? '—' : `${fmt(latest.kpi.cpu_overhead_pct, 1)}%`],
    ['KPI label', latest.kpi?.label || '—'],
    ['Trend KPI', latest.kpi?.trend == null ? '—' : fmt(latest.kpi.trend, 2)],
    ['Bridge', latest.external_power?.bridge_state || '—'],
    ['Fraîcheur', latest.external_power?.freshness || '—'],
    ['Export', latest.report?.exported_at || '—'],
    ['Archives', status?.archive_count ?? '—'],
  ].map(([label, value]) => `
    <div class="snapshot-item">
      <div class="snapshot-label">${label}</div>
      <div class="snapshot-value">${value}</div>
    </div>
  `).join('') : '<div class="empty">Aucun snapshot live.</div>';

  const processes = latest?.process_impact_report?.top_process_rows || [];
  processList.innerHTML = processes.length ? processes.slice(0, 8).map((process) => `
    <div class="process-item">
      <div class="process-head">
        <div>
          <div class="process-name">${process.name}</div>
          <div class="process-role">${process.is_self_process ? 'SoulKernel' : process.is_embedded_webview ? 'WebView' : process.role || 'processus'}</div>
        </div>
        <strong>${process.power_label || '—'}</strong>
      </div>
      <div class="process-metrics">
        <span>CPU ${process.cpu_label || '—'}</span>
        <span>RAM ${process.ram_label || '—'}</span>
        <span>Impact ${process.impact_label || '—'}</span>
        <span>Durée ${process.duration_label || '—'}</span>
      </div>
    </div>
  `).join('') : '<div class="empty">Pas de contribution processus disponible.</div>';
}

async function refreshAll() {
  const [statusRes, latestRes, timelineRes] = await Promise.all([
    fetch('/api/status'),
    fetch('/api/latest'),
    fetch('/api/timeline?limit=720'),
  ]);

  state.status = await statusRes.json();
  state.latest = latestRes.ok ? await latestRes.json() : null;
  state.timeline = timelineRes.ok ? await timelineRes.json() : { samples: [] };
  render();
}

refreshAll();
setInterval(refreshAll, 2500);
