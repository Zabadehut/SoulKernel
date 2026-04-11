const state = {
  latest: null,
  timeline: [],
  status: null,
};

const headlineCards    = document.getElementById('headlineCards');
const charts           = document.getElementById('charts');
const snapshotGrid     = document.getElementById('snapshotGrid');
const processList      = document.getElementById('processList');
const freshnessPill    = document.getElementById('freshnessPill');
const statusLine       = document.getElementById('statusLine');
const activePath       = document.getElementById('activePath');
const archiveCount     = document.getElementById('archiveCount');
const sampleCount      = document.getElementById('sampleCount');
const domeImpactPanel  = document.getElementById('domeImpactPanel');
const kpiLearningPanel = document.getElementById('kpiLearningPanel');
const auditLogPanel    = document.getElementById('auditLogPanel');
const missionPanel     = document.getElementById('missionPanel');

document.getElementById('refreshBtn').addEventListener('click', refreshAll);

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

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
function fmtDuration(ms) {
  if (ms == null || ms < 0) return '—';
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60), rs = s % 60;
  if (m < 60) return `${m}m${rs}s`;
  return `${Math.floor(m / 60)}h${m % 60}m`;
}

function kpiColor(label) {
  if (!label) return 'var(--muted)';
  const l = label.toLowerCase();
  if (l === 'efficace' || l === 'excellent') return 'var(--green)';
  if (l === 'modéré' || l === 'moderate') return 'var(--yellow)';
  if (l === 'inefficace' || l === 'inefficient') return 'var(--red)';
  return 'var(--cyan)';
}
function activityColor(a) {
  if (!a) return 'var(--muted)';
  return a === 'active' ? 'var(--green)' : a === 'idle' ? 'var(--cyan)' : 'var(--muted)';
}

// ---------------------------------------------------------------------------
// Trend detection
// ---------------------------------------------------------------------------

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
  const firstHalfAvg  = ys.slice(0, half).reduce((a, b) => a + b, 0) / half;
  const secondHalfAvg = ys.slice(half).reduce((a, b) => a + b, 0) / (n - half);
  return { slope, slopePerHour, r2, firstHalfAvg, secondHalfAvg, n };
}

function trendArrow(trend, lowerIsBetter) {
  if (!trend || trend.r2 < 0.05) return { arrow: '→', color: '#5a6480', label: 'stable' };
  const relChange = trend.secondHalfAvg !== 0
    ? Math.abs((trend.secondHalfAvg - trend.firstHalfAvg) / trend.firstHalfAvg) : 0;
  if (relChange < 0.03) return { arrow: '→', color: '#5a6480', label: 'stable' };
  const going_up = trend.slope > 0;
  const is_good = lowerIsBetter ? !going_up : going_up;
  return going_up
    ? { arrow: '↑', color: is_good ? '#22c55e' : '#ff6b6b', label: is_good ? 'hausse favorable' : 'hausse défavorable' }
    : { arrow: '↓', color: is_good ? '#22c55e' : '#ff6b6b', label: is_good ? 'baisse favorable' : 'baisse défavorable' };
}

// ---------------------------------------------------------------------------
// Sparkline SVG with optional trend line overlay
// ---------------------------------------------------------------------------

function sparkline(points, color, trend) {
  if (!points.length) return '<div class="empty">Aucune donnée</div>';
  const width = 320, height = 120;
  const values = points.map((p) => p.value).filter((v) => v != null && Number.isFinite(v));
  if (!values.length) return '<div class="empty">Aucune donnée</div>';
  const min = Math.min(...values), max = Math.max(...values), span = max - min || 1;
  const toX = (i) => (i / Math.max(1, points.length - 1)) * width;
  const toY = (v) => height - ((v - min) / span) * (height - 10) - 5;
  const coords = points.map((p, i) => `${toX(i)},${toY(p.value)}`).join(' ');
  let trendLine = '';
  if (trend && trend.r2 >= 0.05) {
    const n = points.length, mx = (n - 1) / 2;
    const my = values.reduce((a, b) => a + b, 0) / n;
    const y1v = my + trend.slope * (0 - mx);
    const y2v = my + trend.slope * (n - 1 - mx);
    trendLine = `<line x1="${toX(0)}" y1="${toY(y1v)}" x2="${toX(n - 1)}" y2="${toY(y2v)}"
      stroke="rgba(255,255,255,.45)" stroke-width="1.5" stroke-dasharray="5,3"/>`;
  }
  return `<svg class="chart-svg" viewBox="0 0 ${width} ${height}" preserveAspectRatio="none">
    <polyline points="${coords}" fill="none" stroke="${color}" stroke-width="3"
      stroke-linecap="round" stroke-linejoin="round"></polyline>
    ${trendLine}
  </svg>`;
}

// ---------------------------------------------------------------------------
// Card / chart builders
// ---------------------------------------------------------------------------

function card(label, value, sub, accent) {
  return `<div class="card" style="--accent:${accent}">
    <div class="card-label">${label}</div>
    <div class="card-value">${value}</div>
    <div class="card-sub">${sub}</div>
  </div>`;
}

function chartCard(title, unit, color, points, lowerIsBetter = false) {
  const values = points.map((p) => p.value).filter((v) => v != null && Number.isFinite(v));
  const latest = values.at(-1);
  const avg = values.length ? values.reduce((a, b) => a + b, 0) / values.length : null;
  const min = values.length ? Math.min(...values) : null;
  const max = values.length ? Math.max(...values) : null;
  const timestamps = points.map((p) => p.ts).filter(Boolean);
  const durationHours = timestamps.length >= 2
    ? (timestamps.at(-1) - timestamps[0]) / 3_600_000 : 0;
  const trend = linearTrend(points, durationHours);
  const ta = trendArrow(trend, lowerIsBetter);
  const slopeHtml = trend && trend.slopePerHour != null && Math.abs(trend.slopePerHour) > 0.001
    ? `<span class="trend-slope" style="color:${ta.color}">${trend.slopePerHour > 0 ? '+' : ''}${fmt(trend.slopePerHour, 3)} ${unit}/h R²=${fmt(trend.r2, 2)}</span>`
    : '';
  const halfDelta = trend ? trend.secondHalfAvg - trend.firstHalfAvg : null;
  const halfHtml = halfDelta != null && Math.abs(halfDelta) > 0.001
    ? `<div><span>1ère½→2ème½</span><strong style="color:${ta.color}">${fmt(trend.firstHalfAvg, 1)}→${fmt(trend.secondHalfAvg, 1)} ${unit}</strong></div>`
    : '';
  return `<div class="chart-card">
    <div class="chart-title">
      <h4>${title} <span style="font-size:16px;color:${ta.color}">${ta.arrow}</span></h4>
      <div class="chart-meta">${values.length.toLocaleString('fr-FR')} pts<br>dernier ${fmt(latest, 2)} ${unit}</div>
    </div>
    ${slopeHtml ? `<div class="trend-label" style="color:${ta.color}">${ta.label} — ${slopeHtml}</div>` : ''}
    ${sparkline(points, color, trend)}
    <div class="chart-stats">
      <div><span>Moy.</span><strong>${fmt(avg, 2)} ${unit}</strong></div>
      <div><span>Min</span><strong>${fmt(min, 2)} ${unit}</strong></div>
      <div><span>Max</span><strong>${fmt(max, 2)} ${unit}</strong></div>
    </div>
    ${halfHtml ? `<div class="chart-stats">${halfHtml}</div>` : ''}
  </div>`;
}

function toPoints(series, key) {
  return series
    .filter((s) => s[key] != null && Number.isFinite(s[key]))
    .map((s) => ({ ts: s.ts_ms, value: s[key] }));
}

// ---------------------------------------------------------------------------
// Audit log — dome & algorithm state transitions
// ---------------------------------------------------------------------------

function buildAuditEvents(timeline) {
  const events = [];
  let prevDome = null;
  let prevKpiLabel = null;
  let prevActivity = null;
  let domeOnSince = null;

  for (const s of timeline) {
    const ts = s.ts_ms;

    // Dome transitions
    if (prevDome !== null && s.dome_active !== prevDome) {
      const direction = s.dome_active ? 'on' : 'off';
      const duration = s.dome_active ? null : (domeOnSince != null ? ts - domeOnSince : null);
      events.push({
        type: 'dome',
        direction,
        ts,
        watts: s.watts,
        kpi_label: s.kpi_label,
        kpi_value: s.kpi_penalized,
        duration_ms: duration,
      });
      if (s.dome_active) domeOnSince = ts;
      else domeOnSince = null;
    } else if (prevDome === null && s.dome_active) {
      domeOnSince = ts;
    }

    // KPI label changes
    if (prevKpiLabel !== null && s.kpi_label && s.kpi_label !== prevKpiLabel) {
      events.push({
        type: 'kpi_change',
        ts,
        from: prevKpiLabel,
        to: s.kpi_label,
        kpi_value: s.kpi_penalized,
        watts: s.watts,
        dome_active: s.dome_active,
      });
    }

    // Machine activity changes
    if (prevActivity !== null && s.machine_activity && s.machine_activity !== prevActivity) {
      events.push({
        type: 'activity_change',
        ts,
        from: prevActivity,
        to: s.machine_activity,
        watts: s.watts,
      });
    }

    prevDome = s.dome_active;
    if (s.kpi_label) prevKpiLabel = s.kpi_label;
    if (s.machine_activity) prevActivity = s.machine_activity;
  }

  return events.sort((a, b) => b.ts - a.ts); // most recent first
}

function eventRow(ev) {
  if (ev.type === 'dome') {
    const icon = ev.direction === 'on' ? '🔵' : '⚫';
    const label = ev.direction === 'on'
      ? `<span class="ev-dome-on">Dôme ACTIVÉ</span>`
      : `<span class="ev-dome-off">Dôme DÉSACTIVÉ</span>${ev.duration_ms != null ? ` <span class="ev-duration">(était ON ${fmtDuration(ev.duration_ms)})</span>` : ''}`;
    const detail = [
      ev.watts != null ? fmtWatts(ev.watts) : null,
      ev.kpi_label ? `KPI ${ev.kpi_label}` : null,
      ev.kpi_value != null ? `${fmt(ev.kpi_value, 2)} W/%` : null,
    ].filter(Boolean).join(' · ');
    return `<div class="ev-row ev-dome">
      <div class="ev-ts">${fmtDate(ev.ts)}</div>
      <div class="ev-body">${icon} ${label}</div>
      <div class="ev-detail">${detail}</div>
    </div>`;
  }

  if (ev.type === 'kpi_change') {
    const fromColor = kpiColor(ev.from), toColor = kpiColor(ev.to);
    const better = (
      (ev.to.toLowerCase() === 'efficace' || ev.to.toLowerCase() === 'excellent') ||
      (ev.from.toLowerCase() === 'inefficace' && ev.to.toLowerCase() === 'modéré')
    );
    const icon = better ? '📈' : '📉';
    const detail = [
      ev.watts != null ? fmtWatts(ev.watts) : null,
      ev.kpi_value != null ? `${fmt(ev.kpi_value, 2)} W/%` : null,
      ev.dome_active ? 'dôme ON' : 'dôme OFF',
    ].filter(Boolean).join(' · ');
    return `<div class="ev-row ev-kpi">
      <div class="ev-ts">${fmtDate(ev.ts)}</div>
      <div class="ev-body">${icon} KPI <span style="color:${fromColor}">${ev.from}</span> → <span style="color:${toColor}">${ev.to}</span></div>
      <div class="ev-detail">${detail}</div>
    </div>`;
  }

  if (ev.type === 'activity_change') {
    const icon = ev.to === 'active' ? '⚡' : '💤';
    const detail = ev.watts != null ? fmtWatts(ev.watts) : '';
    return `<div class="ev-row ev-activity">
      <div class="ev-ts">${fmtDate(ev.ts)}</div>
      <div class="ev-body">${icon} Activité <span style="color:${activityColor(ev.from)}">${ev.from}</span> → <span style="color:${activityColor(ev.to)}">${ev.to}</span></div>
      <div class="ev-detail">${detail}</div>
    </div>`;
  }

  return '';
}

function renderAuditLog(timeline) {
  const events = buildAuditEvents(timeline);
  const domeEvents = events.filter((e) => e.type === 'dome');
  const kpiEvents  = events.filter((e) => e.type === 'kpi_change');

  const summary = `<div class="audit-summary">
    <span class="audit-stat"><strong>${domeEvents.length}</strong> transitions dôme</span>
    <span class="audit-stat"><strong>${kpiEvents.length}</strong> changements KPI</span>
    <span class="audit-stat"><strong>${events.length}</strong> événements total</span>
  </div>`;

  const rows = events.length
    ? events.slice(0, 40).map(eventRow).join('')
    : '<div class="empty">Aucun événement — timeline trop courte ou données manquantes.</div>';

  return `<div class="panel-head"><h3>Historique des événements</h3></div>
    ${summary}
    <div class="audit-log">${rows}</div>`;
}

// ---------------------------------------------------------------------------
// Mission check — est-ce que SoulKernel atteint son but ?
// ---------------------------------------------------------------------------

function renderMissionCheck(timeline, projection) {
  const criteria = [];

  // 1. Dome activates at all
  const domeOnSamples  = timeline.filter((s) => s.dome_active);
  const domeOffSamples = timeline.filter((s) => !s.dome_active);
  const domeOnPct = timeline.length ? (domeOnSamples.length / timeline.length) * 100 : 0;
  criteria.push({
    label: 'Dôme s\'active',
    pass: domeOnPct >= 5,
    value: `${fmt(domeOnPct, 1)}% du temps`,
    note: domeOnPct < 5 ? 'Le dôme ne s\'active pas — vérifier KPI, guard, auto_dome' : null,
  });

  // 2. Watts lower when dome ON vs OFF
  const avgOnW  = domeOnSamples.filter((s) => s.watts != null).reduce((a, s) => a + s.watts, 0) / (domeOnSamples.filter((s) => s.watts != null).length || 1);
  const avgOffW = domeOffSamples.filter((s) => s.watts != null).reduce((a, s) => a + s.watts, 0) / (domeOffSamples.filter((s) => s.watts != null).length || 1);
  const hasEnoughBoth = domeOnSamples.length >= 4 && domeOffSamples.length >= 4;
  const wattsEcart = hasEnoughBoth ? avgOffW - avgOnW : null;
  criteria.push({
    label: 'Watts ↓ dôme ON vs OFF',
    pass: wattsEcart != null && wattsEcart > 1,
    value: wattsEcart != null ? `${fmt(wattsEcart, 1)} W d'écart (corrélation)` : 'données insuffisantes',
    note: wattsEcart != null && wattsEcart <= 0 ? 'Pas d\'écart watts — le dôme n\'influence pas la consommation observée' : null,
  });

  // 3. KPI trending better (lower)
  const kpiPoints = toPoints(timeline, 'kpi_penalized');
  const timestamps = kpiPoints.map((p) => p.ts);
  const durationHours = timestamps.length >= 2 ? (timestamps.at(-1) - timestamps[0]) / 3_600_000 : 0;
  const kpiTrend = linearTrend(kpiPoints, durationHours);
  const kpiBetter = kpiTrend && kpiTrend.r2 >= 0.05 && kpiTrend.slope < 0;
  criteria.push({
    label: 'KPI en amélioration',
    pass: kpiBetter,
    value: kpiTrend
      ? `slope ${fmt(kpiTrend.slope, 4)} W/% / tick, R²=${fmt(kpiTrend.r2, 2)}`
      : 'données insuffisantes',
    note: kpiTrend && kpiTrend.r2 < 0.05 ? 'Tendance trop faible (R² < 0.05)' : null,
  });

  // 4. KPI currently Efficace or Modéré (not Inefficace)
  const curLabel = projection?.kpi_label?.toLowerCase();
  const kpiOk = curLabel && curLabel !== 'inefficace' && curLabel !== 'inefficient';
  criteria.push({
    label: 'KPI courant acceptable',
    pass: kpiOk,
    value: projection?.kpi_label || '—',
    note: (!kpiOk && curLabel) ? 'Système en mode Inefficace — SoulKernel devrait agir' : null,
  });

  // 5. SoulRAM active when needed
  const soulramActive = projection?.soulram_active ?? false;
  const highFaults = (projection?.faults_per_sec ?? 0) > 100;
  criteria.push({
    label: 'SoulRAM actif si nécessaire',
    pass: !highFaults || soulramActive,
    value: soulramActive ? 'Actif' : `Inactif — ${fmt(projection?.faults_per_sec, 0)} faults/s`,
    note: highFaults && !soulramActive ? 'Pression mémoire élevée mais SoulRAM inactif' : null,
  });

  // 6. Dome respects pi signal (advanced_guard > 0 when dome ON)
  const domeOnGuardSamples = domeOnSamples.filter((s) => s.advanced_guard != null);
  const avgGuard = domeOnGuardSamples.length
    ? domeOnGuardSamples.reduce((a, s) => a + s.advanced_guard, 0) / domeOnGuardSamples.length
    : null;
  criteria.push({
    label: 'Formule pi cohérente (guard > 0)',
    pass: avgGuard != null ? avgGuard > 0 : null,
    value: avgGuard != null ? `guard moy. ${fmt(avgGuard, 3)} (dôme ON)` : '—',
    note: avgGuard != null && avgGuard <= 0 ? 'Guard négatif en moyenne — le signal pi est peut-être mal calibré' : null,
  });

  const passed = criteria.filter((c) => c.pass === true).length;
  const total  = criteria.filter((c) => c.pass !== null).length;
  const score  = total > 0 ? passed / total : null;

  let verdict, verdictColor;
  if (score === null) { verdict = 'Données insuffisantes'; verdictColor = 'var(--muted)'; }
  else if (score >= 0.8) { verdict = 'Objectif atteint'; verdictColor = 'var(--green)'; }
  else if (score >= 0.5) { verdict = 'Résultats partiels'; verdictColor = 'var(--yellow)'; }
  else { verdict = 'Objectif non atteint'; verdictColor = 'var(--red)'; }

  const scoreBar = total > 0
    ? `<div class="mission-bar"><div class="mission-bar-fill" style="width:${Math.round((passed / total) * 100)}%;background:${verdictColor}"></div></div>`
    : '';

  const rows = criteria.map((c) => {
    const icon = c.pass === true ? '✅' : c.pass === false ? '❌' : '⏳';
    return `<div class="mission-row">
      <span class="mission-icon">${icon}</span>
      <div class="mission-body">
        <div class="mission-label">${c.label}</div>
        <div class="mission-value">${c.value}</div>
        ${c.note ? `<div class="mission-note">${c.note}</div>` : ''}
      </div>
    </div>`;
  }).join('');

  return `<div class="panel-head">
    <h3>Mission SoulKernel</h3>
    <div class="mission-verdict" style="color:${verdictColor}">${verdict} ${score != null ? `${passed}/${total}` : ''}</div>
  </div>
  ${scoreBar}
  <div class="mission-list">${rows}</div>`;
}

// ---------------------------------------------------------------------------
// Dome impact panel
// ---------------------------------------------------------------------------

function renderDomeImpact(projection, timeline) {
  const domeOnSamples  = timeline.filter((s) => s.dome_active  && s.watts != null);
  const domeOffSamples = timeline.filter((s) => !s.dome_active && s.watts != null);
  const avgOn  = domeOnSamples.length  ? domeOnSamples.reduce( (a, s) => a + s.watts, 0) / domeOnSamples.length  : null;
  const avgOff = domeOffSamples.length ? domeOffSamples.reduce((a, s) => a + s.watts, 0) / domeOffSamples.length : null;
  const ecart   = avgOn != null && avgOff != null ? avgOff - avgOn : null;
  const domeOnPct = timeline.length ? (timeline.filter((s) => s.dome_active).length / timeline.length) * 100 : null;
  const savedKwh = projection?.energy_saved_kwh;

  const rows = [
    ['Dôme courant',    projection?.dome_active ? '<span style="color:var(--green)">Actif</span>' : 'Inactif'],
    ['Moy. watts ON',   fmtWatts(projection?.dome_on_avg_w ?? avgOn)],
    ['Moy. watts OFF',  fmtWatts(projection?.dome_off_avg_w ?? avgOff)],
    ['Écart timeline',  ecart != null ? `<span style="color:${ecart > 0 ? 'var(--green)' : 'var(--muted)'}">${fmt(ecart, 1)} W</span>` : '—'],
    ['kWh écart session', savedKwh != null ? `${fmt(savedKwh, 4)} kWh` : '—'],
    ['% temps dôme ON', domeOnPct != null ? fmtPct(domeOnPct) : '—'],
    ['Confidence power', projection?.power_confidence != null ? fmtPct(projection.power_confidence * 100) : '—'],
  ];

  return `<div class="panel-head"><h3>Impact dôme</h3></div>
    <p class="dome-note">Corrélation observée — les périodes ON/OFF ne sont pas contrôlées.</p>
    <div class="snapshot-grid">${rows.map(([l, v]) => `
      <div class="snapshot-item"><div class="snapshot-label">${l}</div><div class="snapshot-value">${v}</div></div>`
    ).join('')}</div>`;
}

// ---------------------------------------------------------------------------
// KPI learning panel
// ---------------------------------------------------------------------------

function renderKpiLearning(projection) {
  const label = projection?.kpi_label;
  const color = kpiColor(label);
  const rows = [
    ['KPI label',      label ? `<span style="color:${color}">${label}</span>` : '—'],
    ['KPI pénalisé',   projection?.kpi_penalized != null ? `${fmt(projection.kpi_penalized, 3)} W/%` : '—'],
    ['KPI de base',    projection?.kpi_basic      != null ? `${fmt(projection.kpi_basic, 3)} W/%`      : '—'],
    ['Reward ratio',   projection?.kpi_reward_ratio != null ? fmt(projection.kpi_reward_ratio, 3) : '—'],
    ['Trend KPI',      projection?.kpi_trend        != null ? fmt(projection.kpi_trend, 3)         : '—'],
    ['CPU utile',      fmtPct(projection?.cpu_useful_pct)],
    ['CPU overhead',   fmtPct(projection?.cpu_overhead_pct)],
    ['Pi (formule)',   projection?.pi              != null ? fmt(projection.pi, 3)              : '—'],
    ['Advanced guard', projection?.advanced_guard  != null ? fmt(projection.advanced_guard, 3)  : '—'],
    ['Compression',    projection?.compression     != null ? fmt(projection.compression, 3)     : '—'],
    ['Sigma',          projection?.sigma           != null ? fmt(projection.sigma, 3)           : '—'],
    ['Activité',       projection?.machine_activity
      ? `<span style="color:${activityColor(projection.machine_activity)}">${projection.machine_activity}</span>` : '—'],
  ];

  return `<div class="panel-head"><h3>Apprentissage KPI</h3></div>
    <div class="snapshot-grid">${rows.map(([l, v]) => `
      <div class="snapshot-item"><div class="snapshot-label">${l}</div><div class="snapshot-value">${v}</div></div>`
    ).join('')}</div>`;
}

// ---------------------------------------------------------------------------
// Main render
// ---------------------------------------------------------------------------

function render() {
  const latest     = state.latest?.latest;
  const projection = state.latest?.latest_projection;
  const status     = state.status;
  const timeline   = state.timeline?.samples || [];

  freshnessPill.textContent  = status?.is_fresh ? 'live' : 'stale';
  freshnessPill.style.color  = status?.is_fresh ? 'var(--green)' : 'var(--orange)';
  statusLine.textContent     = latest
    ? `Dernier tick ${fmtDate(status.latest_sample_ts_ms)} · ${status.power_source || 'source inconnue'} · ${fmtWatts(status.latest_watts)}`
    : 'Aucun tick observability détecté pour le moment.';
  activePath.textContent  = status?.observability_path || '—';
  archiveCount.textContent = status ? String(status.archive_count) : '—';
  sampleCount.textContent  = status ? String(status.sample_count) : '—';

  const kpiLabel  = projection?.kpi_label;
  const kpiAccent = kpiColor(kpiLabel);
  const activity  = projection?.machine_activity;

  headlineCards.innerHTML = [
    card('Watts mur',   fmtWatts(projection?.watts),  `host ${fmtWatts(projection?.host_power_w)}`, 'var(--green)'),
    card('CPU total',   fmtPct(projection?.cpu_pct),  `utile ${fmtPct(projection?.cpu_useful_pct, 0)} / ovhd ${fmtPct(projection?.cpu_overhead_pct, 0)}`, 'var(--cyan)'),
    card('RAM',         fmtPct(projection?.ram_pct),  `${fmt(projection?.ram_used_mb, 0)} / ${fmt(projection?.ram_total_mb, 0)} MiB`, '#60a5fa'),
    card('GPU',         fmtPct(projection?.gpu_pct),  `${fmt(projection?.gpu_power_watts, 1)} W GPU`, '#a78bfa'),
    card('KPI',
      kpiLabel
        ? `<span style="color:${kpiAccent}">${kpiLabel}</span>`
        : (projection?.kpi_penalized == null ? '—' : `${fmt(projection.kpi_penalized, 2)} W/%`),
      projection?.kpi_penalized != null ? `${fmt(projection.kpi_penalized, 3)} W/%` : 'Signal runtime',
      kpiAccent),
    card('Activité',
      activity ? `<span style="color:${activityColor(activity)}">${activity}</span>` : '—',
      `faults ${fmt(projection?.faults_per_sec, 0)}/s`, 'var(--orange)'),
    card('Pi / Guard',  projection?.pi != null ? fmt(projection.pi, 3) : '—',
      `guard ${fmt(projection?.advanced_guard, 3)}`, '#e879f9'),
    card('Workload',    projection?.workload || '—',
      `SoulRAM ${latest?.report?.soulram_active ? 'actif' : 'inactif'}`, 'var(--yellow)'),
  ].join('');

  charts.innerHTML = [
    chartCard('Watts mur',      'W',   '#4ade80', toPoints(timeline, 'watts'),           true),
    chartCard('CPU total',      '%',   '#22d3ee', toPoints(timeline, 'cpu_pct'),          true),
    chartCard('RAM utilisée',   '%',   '#60a5fa', toPoints(timeline, 'ram_pct'),          true),
    chartCard('GPU',            '%',   '#a78bfa', toPoints(timeline, 'gpu_pct'),          true),
    chartCard('KPI pénalisé',   'W/%', '#fb923c', toPoints(timeline, 'kpi_penalized'),    true),
    chartCard('Page faults',    '/s',  '#fb7185', toPoints(timeline, 'faults_per_sec'),   true),
    chartCard('CPU utile',      '%',   '#86efac', toPoints(timeline, 'cpu_useful_pct'),   false),
    chartCard('CPU overhead',   '%',   '#fca5a5', toPoints(timeline, 'cpu_overhead_pct'), true),
    chartCard('Pi (formule)',   '',    '#e879f9', toPoints(timeline, 'pi'),               false),
    chartCard('Advanced guard', '',    '#c084fc', toPoints(timeline, 'advanced_guard'),   false),
    chartCard('Compression',    '',    '#67e8f9', toPoints(timeline, 'compression'),      false),
    chartCard('Watts host',     'W',   '#6ee7b7', toPoints(timeline, 'host_power_w'),     true),
  ].join('');

  snapshotGrid.innerHTML = latest ? [
    ['Workload',    latest.report?.workload || '—'],
    ['Dôme',        latest.report?.dome_active   ? '<span style="color:var(--green)">Actif</span>' : 'Inactif'],
    ['SoulRAM',     latest.report?.soulram_active ? '<span style="color:var(--cyan)">Actif</span>' : 'Inactif'],
    ['Cible PID',   latest.report?.target_pid ?? '—'],
    ['KPI label',   kpiLabel ? `<span style="color:${kpiAccent}">${kpiLabel}</span>` : '—'],
    ['Reward ratio', projection?.kpi_reward_ratio != null ? fmt(projection.kpi_reward_ratio, 3) : '—'],
    ['Activité',    activity ? `<span style="color:${activityColor(activity)}">${activity}</span>` : '—'],
    ['Bridge',      latest.external_power?.bridge_state || '—'],
    ['Fraîcheur',   latest.external_power?.freshness || '—'],
    ['Export',      latest.report?.exported_at || '—'],
    ['Archives',    status?.archive_count ?? '—'],
    ['Échantillons', status?.sample_count ?? '—'],
  ].map(([label, value]) => `
    <div class="snapshot-item">
      <div class="snapshot-label">${label}</div>
      <div class="snapshot-value">${value}</div>
    </div>`).join('')
  : '<div class="empty">Aucun snapshot live.</div>';

  const processes = latest?.process_impact_report?.top_process_rows || [];
  processList.innerHTML = processes.length
    ? processes.slice(0, 8).map((p) => `
    <div class="process-item">
      <div class="process-head">
        <div>
          <div class="process-name">${p.name}</div>
          <div class="process-role">${p.is_self_process ? 'SoulKernel' : p.is_embedded_webview ? 'WebView' : p.role || 'processus'}</div>
        </div>
        <strong>${p.power_label || '—'}</strong>
      </div>
      <div class="process-metrics">
        <span>CPU ${p.cpu_label || '—'}</span>
        <span>RAM ${p.ram_label || '—'}</span>
        <span>Impact ${p.impact_label || '—'}</span>
        <span>Durée ${p.duration_label || '—'}</span>
      </div>
    </div>`).join('')
    : '<div class="empty">Pas de contribution processus disponible.</div>';

  domeImpactPanel.innerHTML  = renderDomeImpact(projection, timeline);
  kpiLearningPanel.innerHTML = renderKpiLearning(projection);
  auditLogPanel.innerHTML    = renderAuditLog(timeline);
  missionPanel.innerHTML     = renderMissionCheck(timeline, projection);
}

// ---------------------------------------------------------------------------
// Data fetch
// ---------------------------------------------------------------------------

async function refreshAll() {
  const [statusRes, latestRes, timelineRes] = await Promise.all([
    fetch('/api/status'),
    fetch('/api/latest'),
    fetch('/api/timeline?limit=720'),
  ]);
  state.status   = await statusRes.json();
  state.latest   = latestRes.ok  ? await latestRes.json()   : null;
  state.timeline = timelineRes.ok ? await timelineRes.json() : { samples: [] };
  render();
}

refreshAll();
setInterval(refreshAll, 2500);
