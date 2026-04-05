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
const domeImpactPanel = document.getElementById('domeImpactPanel');
const kpiLearningPanel = document.getElementById('kpiLearningPanel');

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

// --- Trend detection (same logic as soulkernel_energy_dashboard.html) ---

function linearTrend(points, durationHours) {
  const n = points.length;
  if (n < 4) return null;
  const ys = points.map((p) => p.value);
  const xs = ys.map((_, i) => i);
  const mx = xs.reduce((a, b) => a + b, 0) / n;
  const my = ys.reduce((a, b) => a + b, 0) / n;
  let num = 0, den = 0;
  for (let i = 0; i < n; i++) { num += (xs[i] - mx) * (ys[i] - my); den += (xs[i] - mx) ** 2; }
  if (den === 0) return null;
  const slope = num / den;
  const slopePerHour = durationHours > 0 ? (slope * n / durationHours) : null;
  const ssRes = ys.reduce((acc, y, i) => acc + (y - (my + slope * (xs[i] - mx))) ** 2, 0);
  const ssTot = ys.reduce((acc, y) => acc + (y - my) ** 2, 0);
  const r2 = ssTot > 0 ? Math.max(0, 1 - ssRes / ssTot) : 0;
  const half = Math.floor(n / 2);
  const firstHalfAvg = ys.slice(0, half).reduce((a, b) => a + b, 0) / half;
  const secondHalfAvg = ys.slice(half).reduce((a, b) => a + b, 0) / (n - half);
  return { slope, slopePerHour, r2, firstHalfAvg, secondHalfAvg, n };
}

function trendArrow(trend, lowerIsBetter) {
  if (!trend || trend.r2 < 0.05) return { arrow: '→', color: '#5a6480', label: 'stable' };
  const relChange = trend.secondHalfAvg !== 0 ? Math.abs((trend.secondHalfAvg - trend.firstHalfAvg) / trend.firstHalfAvg) : 0;
  if (relChange < 0.03) return { arrow: '→', color: '#5a6480', label: 'stable' };
  const going_up = trend.slope > 0;
  const is_good = lowerIsBetter ? !going_up : going_up;
  return going_up
    ? { arrow: '↑', color: is_good ? '#22c55e' : '#ff6b6b', label: is_good ? 'hausse favorable' : 'hausse défavorable' }
    : { arrow: '↓', color: is_good ? '#22c55e' : '#ff6b6b', label: is_good ? 'baisse favorable' : 'baisse défavorable' };
}

function sparkline(points, color, trend) {
  if (!points.length) return '<div class="empty">Aucune donnée</div>';
  const width = 320;
  const height = 120;
  const values = points.map((p) => p.value).filter((v) => v != null && Number.isFinite(v));
  if (!values.length) return '<div class="empty">Aucune donnée</div>';
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;
  const toX = (i) => (i / Math.max(1, points.length - 1)) * width;
  const toY = (v) => height - ((v - min) / span) * (height - 10) - 5;
  const coords = points.map((point, idx) => `${toX(idx)},${toY(point.value)}`).join(' ');

  let trendLine = '';
  if (trend && trend.r2 >= 0.05) {
    const n = points.length;
    const mx = (n - 1) / 2;
    const my = values.reduce((a, b) => a + b, 0) / n;
    const y1v = my + trend.slope * (0 - mx);
    const y2v = my + trend.slope * (n - 1 - mx);
    trendLine = `<line x1="${toX(0)}" y1="${toY(y1v)}" x2="${toX(n - 1)}" y2="${toY(y2v)}" stroke="rgba(255,255,255,.45)" stroke-width="1.5" stroke-dasharray="5,3"/>`;
  }

  return `<svg class="chart-svg" viewBox="0 0 ${width} ${height}" preserveAspectRatio="none">
    <polyline points="${coords}" fill="none" stroke="${color}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"></polyline>
    ${trendLine}
  </svg>`;
}

function card(label, value, sub, accent) {
  return `<div class="card" style="--accent:${accent}">
    <div class="card-label">${label}</div>
    <div class="card-value">${value}</div>
    <div class="card-sub">${sub}</div>
  </div>`;
}

function kpiColor(label) {
  if (!label) return 'var(--muted)';
  if (label === 'Efficace' || label === 'Excellent') return 'var(--green)';
  if (label === 'Modéré' || label === 'Moderate') return 'var(--yellow)';
  if (label === 'Inefficace' || label === 'Inefficient') return 'var(--red)';
  return 'var(--cyan)';
}

function activityColor(activity) {
  if (!activity) return 'var(--muted)';
  if (activity === 'active') return 'var(--green)';
  if (activity === 'idle') return 'var(--cyan)';
  return 'var(--muted)';
}

function chartCard(title, unit, color, points, lowerIsBetter = false) {
  const values = points.map((point) => point.value).filter((value) => value != null && Number.isFinite(value));
  const latest = values.at(-1);
  const avg = values.length ? values.reduce((acc, value) => acc + value, 0) / values.length : null;
  const min = values.length ? Math.min(...values) : null;
  const max = values.length ? Math.max(...values) : null;

  // Compute duration in hours from timestamps
  const timestamps = points.map((p) => p.ts).filter(Boolean);
  const durationHours = timestamps.length >= 2
    ? (timestamps.at(-1) - timestamps[0]) / 3_600_000
    : 0;

  const trend = linearTrend(points, durationHours);
  const ta = trendArrow(trend, lowerIsBetter);

  const slopeHtml = trend && trend.slopePerHour != null && Math.abs(trend.slopePerHour) > 0.001
    ? `<span class="trend-slope" style="color:${ta.color}">${trend.slopePerHour > 0 ? '+' : ''}${fmt(trend.slopePerHour, 3)} ${unit}/h (R²=${fmt(trend.r2, 2)})</span>`
    : '';

  const halfDelta = trend ? trend.secondHalfAvg - trend.firstHalfAvg : null;
  const halfHtml = halfDelta != null && Math.abs(halfDelta) > 0.001
    ? `<div><span>1ère½→2ème½</span><strong style="color:${ta.color}">${fmt(trend.firstHalfAvg, 1)} → ${fmt(trend.secondHalfAvg, 1)} ${unit}</strong></div>`
    : '';

  return `<div class="chart-card">
    <div class="chart-title">
      <h4>${title} <span style="font-size:16px;color:${ta.color}">${ta.arrow}</span></h4>
      <div class="chart-meta">${values.length.toLocaleString('fr-FR')} pts<br>dernier ${fmt(latest, 2)} ${unit}</div>
    </div>
    ${slopeHtml ? `<div class="trend-label" style="color:${ta.color}">${ta.label} ${slopeHtml}</div>` : ''}
    ${sparkline(points, color, trend)}
    <div class="chart-stats">
      <div><span>Moyenne</span><strong>${fmt(avg, 2)} ${unit}</strong></div>
      <div><span>Min</span><strong>${fmt(min, 2)} ${unit}</strong></div>
      <div><span>Max</span><strong>${fmt(max, 2)} ${unit}</strong></div>
    </div>
    ${halfHtml ? `<div class="chart-stats">${halfHtml}</div>` : ''}
  </div>`;
}

function toPoints(series, key) {
  return series
    .filter((item) => item[key] != null && Number.isFinite(item[key]))
    .map((item) => ({ ts: item.ts_ms, value: item[key] }));
}

function renderDomeImpact(projection, timeline) {
  const domeOnSamples = timeline.filter((s) => s.dome_active && s.watts != null);
  const domeOffSamples = timeline.filter((s) => !s.dome_active && s.watts != null);
  const avgOn = domeOnSamples.length
    ? domeOnSamples.reduce((acc, s) => acc + s.watts, 0) / domeOnSamples.length
    : null;
  const avgOff = domeOffSamples.length
    ? domeOffSamples.reduce((acc, s) => acc + s.watts, 0) / domeOffSamples.length
    : null;
  const ecart = avgOn != null && avgOff != null ? avgOff - avgOn : null;
  const domeOnPct = timeline.length
    ? (domeOnSamples.length / timeline.length) * 100
    : null;

  // Session-level from projection
  const sessionOnW = projection?.dome_on_avg_w;
  const sessionOffW = projection?.dome_off_avg_w;
  const savedKwh = projection?.energy_saved_kwh;

  const rows = [
    ['Dôme actif', projection?.dome_active ? '<span style="color:var(--green)">Oui</span>' : '<span style="color:var(--muted)">Non</span>'],
    ['Moy. watts dôme ON (session)', fmtWatts(sessionOnW)],
    ['Moy. watts dôme OFF (session)', fmtWatts(sessionOffW)],
    ['Écart dôme (session)', sessionOnW != null && sessionOffW != null
      ? `<span style="color:${sessionOffW > sessionOnW ? 'var(--green)' : 'var(--muted)'}">${fmt(sessionOffW - sessionOnW, 1)} W</span>`
      : '—'],
    ['Écart watts timeline', ecart != null
      ? `<span style="color:${ecart > 0 ? 'var(--green)' : 'var(--muted)'}">${fmt(ecart, 1)} W</span>`
      : '—'],
    ['kWh écart session', savedKwh != null ? `${fmt(savedKwh, 4)} kWh` : '—'],
    ['% temps dôme ON', domeOnPct != null ? fmtPct(domeOnPct) : '—'],
    ['Confidence power', projection?.power_confidence != null ? fmtPct(projection.power_confidence * 100) : '—'],
  ];

  const note = '<p class="dome-note">Corrélation observée, pas causalité prouvée — les périodes dôme ON/OFF ne sont pas contrôlées.</p>';

  return `<div class="panel-head"><h3>Impact dôme</h3></div>
    ${note}
    <div class="snapshot-grid">${rows.map(([label, value]) => `
      <div class="snapshot-item">
        <div class="snapshot-label">${label}</div>
        <div class="snapshot-value">${value}</div>
      </div>`).join('')}
    </div>`;
}

function renderKpiLearning(projection) {
  const label = projection?.kpi_label;
  const color = kpiColor(label);
  const rows = [
    ['KPI label', label ? `<span style="color:${color}">${label}</span>` : '—'],
    ['KPI pénalisé', projection?.kpi_penalized != null ? `${fmt(projection.kpi_penalized, 3)} W/%` : '—'],
    ['KPI de base', projection?.kpi_basic != null ? `${fmt(projection.kpi_basic, 3)} W/%` : '—'],
    ['Reward ratio', projection?.kpi_reward_ratio != null ? fmt(projection.kpi_reward_ratio, 3) : '—'],
    ['Trend KPI', projection?.kpi_trend != null ? fmt(projection.kpi_trend, 3) : '—'],
    ['CPU utile', fmtPct(projection?.cpu_useful_pct)],
    ['CPU overhead', fmtPct(projection?.cpu_overhead_pct)],
    ['Pi (formule)', projection?.pi != null ? fmt(projection.pi, 3) : '—'],
    ['Advanced guard', projection?.advanced_guard != null ? fmt(projection.advanced_guard, 3) : '—'],
    ['Compression', projection?.compression != null ? fmt(projection.compression, 3) : '—'],
    ['Sigma', projection?.sigma != null ? fmt(projection.sigma, 3) : '—'],
    ['Activité machine', projection?.machine_activity
      ? `<span style="color:${activityColor(projection.machine_activity)}">${projection.machine_activity}</span>`
      : '—'],
  ];

  return `<div class="panel-head"><h3>Apprentissage KPI</h3></div>
    <div class="snapshot-grid">${rows.map(([lbl, val]) => `
      <div class="snapshot-item">
        <div class="snapshot-label">${lbl}</div>
        <div class="snapshot-value">${val}</div>
      </div>`).join('')}
    </div>`;
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

  const kpiLabel = projection?.kpi_label;
  const kpiAccent = kpiColor(kpiLabel);
  const activity = projection?.machine_activity;

  headlineCards.innerHTML = [
    card('Watts mur', fmtWatts(projection?.watts), `host ${fmtWatts(projection?.host_power_w)}`, 'var(--green)'),
    card('CPU total', fmtPct(projection?.cpu_pct), `utile ${fmtPct(projection?.cpu_useful_pct, 0)} / ovhd ${fmtPct(projection?.cpu_overhead_pct, 0)}`, 'var(--cyan)'),
    card('RAM', fmtPct(projection?.ram_pct), `${fmt(projection?.ram_used_mb, 0)} / ${fmt(projection?.ram_total_mb, 0)} MiB`, '#60a5fa'),
    card('GPU', fmtPct(projection?.gpu_pct), `${fmt(projection?.gpu_power_watts, 1)} W GPU`, '#a78bfa'),
    card('KPI', kpiLabel
      ? `<span style="color:${kpiAccent}">${kpiLabel}</span>`
      : (projection?.kpi_penalized == null ? '—' : `${fmt(projection.kpi_penalized, 2)} W/%`),
      projection?.kpi_penalized != null ? `${fmt(projection.kpi_penalized, 3)} W/% pénalisé` : 'Signal runtime', kpiAccent),
    card('Activité', activity
      ? `<span style="color:${activityColor(activity)}">${activity}</span>`
      : '—',
      `faults ${fmt(projection?.faults_per_sec, 0)}/s`, 'var(--orange)'),
    card('Pi / Guard', projection?.pi != null ? fmt(projection.pi, 3) : '—',
      `guard ${fmt(projection?.advanced_guard, 3)}`, '#e879f9'),
    card('Workload', projection?.workload || '—',
      `SoulRAM ${latest?.report?.soulram_active ? 'actif' : 'inactif'}`, 'var(--yellow)'),
  ].join('');

  charts.innerHTML = [
    chartCard('Watts mur', 'W', '#4ade80', toPoints(timeline, 'watts'), true),
    chartCard('CPU total', '%', '#22d3ee', toPoints(timeline, 'cpu_pct'), true),
    chartCard('RAM utilisée', '%', '#60a5fa', toPoints(timeline, 'ram_pct'), true),
    chartCard('GPU', '%', '#a78bfa', toPoints(timeline, 'gpu_pct'), true),
    chartCard('KPI pénalisé', 'W/%', '#fb923c', toPoints(timeline, 'kpi_penalized'), true),
    chartCard('Page faults', '/s', '#fb7185', toPoints(timeline, 'faults_per_sec'), true),
    chartCard('CPU utile', '%', '#86efac', toPoints(timeline, 'cpu_useful_pct'), false),
    chartCard('CPU overhead', '%', '#fca5a5', toPoints(timeline, 'cpu_overhead_pct'), true),
    chartCard('Pi (formule)', '', '#e879f9', toPoints(timeline, 'pi'), false),
    chartCard('Advanced guard', '', '#c084fc', toPoints(timeline, 'advanced_guard'), false),
    chartCard('Compression', '', '#67e8f9', toPoints(timeline, 'compression'), false),
    chartCard('Watts host', 'W', '#6ee7b7', toPoints(timeline, 'host_power_w'), true),
  ].join('');

  snapshotGrid.innerHTML = latest ? [
    ['Workload', latest.report?.workload || '—'],
    ['Dôme', latest.report?.dome_active ? '<span style="color:var(--green)">Actif</span>' : 'Inactif'],
    ['SoulRAM', latest.report?.soulram_active ? '<span style="color:var(--cyan)">Actif</span>' : 'Inactif'],
    ['Cible PID', latest.report?.target_pid ?? '—'],
    ['KPI label', kpiLabel ? `<span style="color:${kpiAccent}">${kpiLabel}</span>` : '—'],
    ['Reward ratio', projection?.kpi_reward_ratio != null ? fmt(projection.kpi_reward_ratio, 3) : '—'],
    ['Activité', activity ? `<span style="color:${activityColor(activity)}">${activity}</span>` : '—'],
    ['Bridge', latest.external_power?.bridge_state || '—'],
    ['Fraîcheur', latest.external_power?.freshness || '—'],
    ['Export', latest.report?.exported_at || '—'],
    ['Archives', status?.archive_count ?? '—'],
    ['Échantillons', status?.sample_count ?? '—'],
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

  domeImpactPanel.innerHTML = renderDomeImpact(projection, timeline);
  kpiLearningPanel.innerHTML = renderKpiLearning(projection);
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
