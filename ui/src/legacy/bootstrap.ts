// @ts-nocheck — logique UI historique (migration progressive vers Svelte)

function refreshSoulKernelLucide() {
  try {
    if (typeof lucide !== 'undefined' && lucide.createIcons) {
      lucide.createIcons({ attrs: { 'stroke-width': 1.5 } });
    }
  } catch (_) {}
}
// ─── Tauri bridge (v2: __TAURI__.core.invoke + tauriReady pour éviter race) ─────
let hasTauri = false;
let rawInvoke = (cmd, args) => fallbackInvoke(cmd, args);
let invoke  = (cmd, args) => invokeWithAudit(cmd, args);
const AUDIT_INTERNAL_CMDS = new Set(['audit_log_event', 'get_audit_log_path']);
const HUD_ONLY = new URLSearchParams(window.location.search).get('hud') === '1';
const AUDIT_HIGH_FREQ_CMDS = new Set([
  'get_metrics',
  'ingest_telemetry_sample',
  'get_telemetry_summary',
  'set_taskbar_gauge',
  'list_processes',
]);

function truncateAuditString(s, max = 1200) {
  if (typeof s !== 'string') return s;
  if (s.length <= max) return s;
  return s.slice(0, max) + '...';
}

function sanitizeAudit(value, depth = 0) {
  if (value == null) return value;
  if (depth > 3) return '[depth-limit]';
  if (typeof value === 'string') return truncateAuditString(value);
  if (typeof value === 'number' || typeof value === 'boolean') return value;
  if (Array.isArray(value)) return value.slice(0, 30).map(v => sanitizeAudit(v, depth + 1));
  if (typeof value === 'object') {
    const out = {};
    const keys = Object.keys(value).slice(0, 40);
    for (const k of keys) out[k] = sanitizeAudit(value[k], depth + 1);
    return out;
  }
  return String(value);
}

function escapeHtml(s) {
  return String(s ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

async function auditEmit(category, action, level = 'info', data = null) {
  if (!hasTauri) return;
  try {
    await rawInvoke('audit_log_event', {
      category,
      action,
      level,
      data: sanitizeAudit(data),
    });
  } catch (_) {}
}

/** Journal lisible + audit structuré : métriques de gain au moment de l’activation (données réelles du backend). */
function logDomeActivationGains(result, detail) {
  if (!result || !result.activated) return;
  const wl = detail && detail.workload != null ? detail.workload : state.wl;
  const pid = detail && detail.targetPid !== undefined ? detail.targetPid : state.targetPid;
  const ctx = (detail && detail.context) ? String(detail.context) : 'activation';
  const pi = Number(result.pi);
  const dGain = Number(result.dome_gain);
  const bIdle = Number(result.b_idle);
  const ok = result.actions_ok ?? 0;
  const tot = result.actions_total ?? 0;
  const cible = pid != null ? 'PID ' + pid : 'priorité SoulKernel (sans PID externe)';
  const piS = Number.isFinite(pi) ? pi.toFixed(4) : 'N/A';
  const dS = Number.isFinite(dGain) ? dGain.toFixed(4) : 'N/A';
  const bS = Number.isFinite(bIdle) ? bIdle.toFixed(4) : 'N/A';
  const msg =
    'GAIN DÔME — ' + ctx +
    ' | π=' + piS + ' (performance instantanée)' +
    ' | 𝒟=' + dS + ' (gain marginal modèle à l\'activation)' +
    ' | b_idle=' + bS + ' (baseline charge inactive)' +
    ' | noyau ' + ok + '/' + tot + ' actions OK' +
    ' | profil ' + wl +
    ' | cible ' + cible;
  log(msg, 'ok');
  auditEmit('dome', 'activation_gains', 'ok', {
    context: ctx,
    pi: Number.isFinite(pi) ? pi : null,
    dome_gain: Number.isFinite(dGain) ? dGain : null,
    b_idle: Number.isFinite(bIdle) ? bIdle : null,
    actions_ok: ok,
    actions_total: tot,
    workload: wl,
    target_pid: pid != null ? pid : null,
  });
}

/** Fin de session dôme : intégrale temps réel (Σ π·Δt) avec le même correctif de borne que l’historique local. */
function logDomeSessionIntegralEnd(adjustedIntegral, extra) {
  const v = Number(adjustedIntegral);
  if (!Number.isFinite(v)) return;
  const msg =
    'GAIN DÔME — session terminée | intégrale ≈ ' + v.toFixed(4) +
    ' (somme π·Δt sur ticks « machine active », correction −0.06 incluse)';
  log(msg, 'ok');
  auditEmit('dome', 'session_integral', 'ok', Object.assign({
    dome_integral_session_approx: v,
  }, extra && typeof extra === 'object' ? extra : {}));
}

/** Résumé A/B : pourcentages mesurés sur la sonde (pas de valeurs inventées). */
function logBenchGainsHuman(summary) {
  if (!summary || typeof summary !== 'object') return;
  const pct = (x) => {
    if (x == null || !Number.isFinite(Number(x))) return 'N/A';
    return Number(x).toFixed(1) + '%';
  };
  const parts = [
    'latence médiane ' + pct(summary.gain_median_pct) + ' vs réf. dôme OFF (plus bas = mieux)',
    'p95 ' + pct(summary.gain_p95_pct),
  ];
  if (summary.gain_mem_median_pct != null && Number.isFinite(Number(summary.gain_mem_median_pct))) {
    parts.push('RAM utilisée Δ ' + Number(summary.gain_mem_median_pct).toFixed(1) + '%');
  }
  if (summary.gain_cpu_median_pct != null && Number.isFinite(Number(summary.gain_cpu_median_pct))) {
    parts.push('charge CPU Δ ' + Number(summary.gain_cpu_median_pct).toFixed(1) + '%');
  }
  if (summary.gain_gpu_median_pct != null && Number.isFinite(Number(summary.gain_gpu_median_pct))) {
    parts.push('GPU Δ ' + Number(summary.gain_gpu_median_pct).toFixed(1) + '%');
  }
  if (summary.gain_power_median_pct != null && Number.isFinite(Number(summary.gain_power_median_pct))) {
    parts.push('puissance Δ ' + Number(summary.gain_power_median_pct).toFixed(1) + '%');
  }
  if (summary.gain_sigma_median_pct != null && Number.isFinite(Number(summary.gain_sigma_median_pct))) {
    parts.push('stress Δ ' + Number(summary.gain_sigma_median_pct).toFixed(1) + '%');
  }
  if (summary.gain_cpu_temp_median_pct != null && Number.isFinite(Number(summary.gain_cpu_temp_median_pct))) {
    parts.push('temp. CPU Δ ' + Number(summary.gain_cpu_temp_median_pct).toFixed(1) + '%');
  }
  if (summary.gain_gpu_temp_median_pct != null && Number.isFinite(Number(summary.gain_gpu_temp_median_pct))) {
    parts.push('temp. GPU Δ ' + Number(summary.gain_gpu_temp_median_pct).toFixed(1) + '%');
  }
  log('GAIN A/B (sonde KPI) — ' + parts.join(' · '), 'ok');
  auditEmit('benchmark', 'gains_summary', 'ok', {
    gain_median_pct: summary.gain_median_pct != null ? Number(summary.gain_median_pct) : null,
    gain_p95_pct: summary.gain_p95_pct != null ? Number(summary.gain_p95_pct) : null,
    gain_mem_median_pct: summary.gain_mem_median_pct != null ? Number(summary.gain_mem_median_pct) : null,
    gain_cpu_median_pct: summary.gain_cpu_median_pct != null ? Number(summary.gain_cpu_median_pct) : null,
    gain_gpu_median_pct: summary.gain_gpu_median_pct != null ? Number(summary.gain_gpu_median_pct) : null,
    gain_power_median_pct: summary.gain_power_median_pct != null ? Number(summary.gain_power_median_pct) : null,
    gain_sigma_median_pct: summary.gain_sigma_median_pct != null ? Number(summary.gain_sigma_median_pct) : null,
    efficiency_score: summary.efficiency_score != null ? Number(summary.efficiency_score) : null,
  });
}

/** Une ligne lisible dans le panneau ; détail JSON uniquement dans l’audit fichier. */
function logStateDiagTarget(payload) {
  const label = payload.next_target_label || 'SoulKernel';
  const src = String(payload.source || '').indexOf('auto') >= 0 ? 'rafraîchissement auto' : 'sélection manuelle';
  const wl = payload.workload != null ? payload.workload : state.wl;
  log('Cible processus (' + src + ') : ' + label + ' · workload ' + wl, 'info');
  auditEmit('state_diag', 'target', 'info', sanitizeAudit(payload));
}

function logStateDiagWorkload(prevWl, wl, sourceLabel) {
  const src = sourceLabel === 'manuel' ? 'manuel' : 'auto / ' + sourceLabel;
  log(
    'Profil workload : ' + wl + ' (avant ' + prevWl + ') · source ' + src +
    ' · cible PID ' + (state.targetPid != null ? state.targetPid : '—'),
    'info'
  );
  auditEmit('state_diag', 'workload', 'info', sanitizeAudit({
    previous_workload: prevWl,
    next_workload: wl,
    source: sourceLabel,
    target_pid: state.targetPid,
    dome_history_len: state.domeHistory.length,
  }));
}

async function invokeWithAudit(cmd, args) {
  const start = Date.now();
  try {
    const result = await rawInvoke(cmd, args);
    if (!AUDIT_INTERNAL_CMDS.has(cmd) && !AUDIT_HIGH_FREQ_CMDS.has(cmd)) {
      auditEmit('invoke', cmd, 'ok', {
        duration_ms: Date.now() - start,
        args,
      });
    }
    return result;
  } catch (e) {
    if (!AUDIT_INTERNAL_CMDS.has(cmd) && !AUDIT_HIGH_FREQ_CMDS.has(cmd)) {
      auditEmit('invoke', cmd, 'err', {
        duration_ms: Date.now() - start,
        args,
        error: String(e),
      });
    }
    throw e;
  }
}

function initTauri() {
  const t = window.__TAURI__ || {};
  const invokeFn = t?.core?.invoke || window.__TAURI_INTERNALS__?.invoke;
  if (!invokeFn) return;
  hasTauri = true;
  rawInvoke = invokeFn;
  const banner = document.getElementById('noTauriBanner');
  if (banner) banner.style.display = 'none';
  log('Tauri connecté — métriques réelles (backend natif)', 'ok');
  rawInvoke('get_audit_log_path').then(p => {
    state.auditLogPath = p;
    log('Audit JSONL: ' + p, 'ok');
    auditEmit('audit', 'path_ready', 'ok', { path: p });
  }).catch(e => log('Audit path error: ' + e, 'warn'));
  loadPlatformInfo();
  updateSystemStatus(null);
}

/** À appeler une fois le shell DOM monté (Svelte {@html}), pas au chargement du module. */
function registerTauriBridge() {
  if (window.__TAURI__?.core?.invoke || window.__TAURI_INTERNALS__?.invoke) {
    initTauri();
  } else {
    window.addEventListener('tauriReady', initTauri, { once: true });
    setTimeout(() => {
      if (!hasTauri && (window.__TAURI__?.core?.invoke || window.__TAURI_INTERNALS__?.invoke)) {
        initTauri();
      }
      if (!hasTauri) {
        const b = document.getElementById('noTauriBanner');
        if (b) b.style.display = 'block';
      }
    }, 150);
  }
}

// ─── Workload alpha table (mirrors Rust side + catalogue 50 scénarios) ──────────
const WORKLOADS_FALLBACK = {
  es:      [0.20, 0.35, 0.20, 0.25, 0.00],
  compile: [0.55, 0.25, 0.10, 0.10, 0.00],
  gamer:   [0.45, 0.20, 0.05, 0.05, 0.25],
  ai:      [0.15, 0.20, 0.05, 0.05, 0.55],
  backup:  [0.15, 0.15, 0.30, 0.40, 0.00],
  sqlite:  [0.20, 0.10, 0.05, 0.65, 0.00],
  oracle:  [0.25, 0.30, 0.10, 0.35, 0.00],
};
let WORKLOADS = { ...WORKLOADS_FALLBACK };
/** Métadonnées scène (hints SoulRAM multi-OS), remplies par loadWorkloadCatalog. */
let WORKLOAD_SCENES = [];

function updateWlSceneMeta(wlId) {
  const box = document.getElementById('wlSceneMeta');
  if (!box) return;
  box.replaceChildren();
  const sc = (WORKLOAD_SCENES || []).find(s => s.id === wlId);
  if (!sc) {
    const p = document.createElement('p');
    p.className = 'wl-meta-empty';
    p.textContent = 'Sélectionnez un scénario pour afficher les actions SoulRAM par OS.';
    box.appendChild(p);
    return;
  }
  const head = document.createElement('div');
  head.className = 'wl-meta-title';
  head.textContent = `${sc.label} · ${sc.hardware_focus || ''} · T≈${Number(sc.duration_estimate_s || 0).toFixed(0)}s · α=${(sc.alpha || []).map(x => Number(x).toFixed(2)).join('/')}`;
  box.appendChild(head);
  const grid = document.createElement('div');
  grid.className = 'wl-meta-os-grid';
  [
    ['Linux (zRAM / swap / PSI)', sc.soulram_linux],
    ['Windows (compression + trim)', sc.soulram_windows],
    ['macOS (ratio + caches)', sc.soulram_macos],
  ].forEach(([t, txt]) => {
    const row = document.createElement('div');
    row.className = 'wl-meta-os';
    const th = document.createElement('div');
    th.className = 'wl-meta-os-label';
    th.textContent = t;
    row.appendChild(th);
    const p = document.createElement('p');
    p.className = 'wl-meta-os-txt';
    p.textContent = txt;
    row.appendChild(p);
    grid.appendChild(row);
  });
  box.appendChild(grid);
}

function fillWlSelect() {
  const sel = document.getElementById('wlSelect');
  if (!sel) return;
  sel.innerHTML = '';
  const list = (WORKLOAD_SCENES && WORKLOAD_SCENES.length)
    ? WORKLOAD_SCENES
    : Object.keys(WORKLOADS_FALLBACK).map(id => ({ id, label: id, category: 'base' }));
  list.forEach(s => {
    const opt = document.createElement('option');
    opt.value = s.id;
    opt.textContent = s.label ? `${s.label} · ${s.category || ''}` : s.id;
    sel.appendChild(opt);
  });
  if (WORKLOADS[state.wl]) sel.value = state.wl;
  else sel.value = 'es';
}

function syncWorkloadUiHighlight() {
  const wl = state.wl;
  document.querySelectorAll('.wl-btn').forEach(b => b.classList.remove('active'));
  const btn = document.querySelector(`.wl-btn[data-wl="${wl}"]`);
  if (btn) btn.classList.add('active');
  const sel = document.getElementById('wlSelect');
  if (sel && WORKLOADS[wl]) sel.value = wl;
  updateWlSceneMeta(wl);
}

async function loadWorkloadCatalog() {
  let scenes = [];
  if (hasTauri) {
    try {
      scenes = await invoke('list_workload_scenes');
    } catch (e) {
      log('list_workload_scenes: ' + e, 'warn');
    }
  }
  if (!scenes.length) {
    try {
      const r = await fetch('/workload_scenes.json');
      if (r.ok) scenes = await r.json();
    } catch (_) {}
  }
  if (!scenes.length) {
    WORKLOAD_SCENES = [];
    WORKLOADS = { ...WORKLOADS_FALLBACK };
    fillWlSelect();
    syncWorkloadUiHighlight();
    return;
  }
  WORKLOAD_SCENES = scenes;
  WORKLOADS = {};
  scenes.forEach(s => { WORKLOADS[s.id] = s.alpha; });
  fillWlSelect();
  if (!WORKLOADS[state.wl]) state.wl = 'es';
  syncWorkloadUiHighlight();
}

let state = {
  wl: 'es',
  kappa: 2.0,
  sigmaMax: 0.75,
  eta: 0.15,
  domeActive: false,
  lastMetrics: null,
  snapshotBefore: null,
  targetPid: null,
  domeHistory: [],
  autoProcessTarget: true,
  lastProcessRefreshTs: null,
  lastProcessCount: 0,
  processImpactReport: { processes: [], top_processes: [], top_process_rows: [], grouped_processes: [], overhead_audit: null, summary: null },
  processList: [],
  windowFocused: true,
  lastUserInteractionTs: Date.now(),
  pendingAdvice: null,
  soulRamActive: false,
  soulRamPercent: 20,
  soulRamBackend: '',
  lastTaskbarPushTs: 0,
  policyMode: 'privileged',
  autoReapplyIntent: true,
  rebootPending: false,
  memoryCompressionEnabled: null,
  soulRamNeedsReboot: false,
  adaptiveEnabled: false,
  adaptiveAutoDome: true,
  adaptiveLastTickTs: 0,
  adaptiveLastSystemApplyTs: 0,
  adaptiveLastDomeSwitchTs: 0,
  kpiBench: {
    running: false,
    sessions: [],
    lastSummary: null,
    tuningAdvice: null,
    topSessions: [],
  },
  lastPi: null,
  lastDomeModel: null,
  machineActivity: 'active',
  domeActionsOk: 0,
  domeActionsTotal: 0,
  domeRealIntegral: 0,
  domeRealLastTs: null,
  workloadLastSwitchTs: 0,
  adaptiveWorkloadCandidate: null,
  adaptiveWorkloadCandidateCount: 0,
  adviceCandidateKey: null,
  adviceCandidateCount: 0,
  adviceCurrentKey: null,
  adviceLastAcceptedTs: 0,
  auditMetricSeq: 0,
  auditLogPath: null,
  naLabel: 'N/A',
  telemetrySummary: null,
  lastTelemetryIngestTs: 0,
  lastTelemetryRefreshTs: 0,
  viewMode: 'detailed',
  hudVisible: false,
  hudInteractive: false,
  hudPreset: 'compact',
  hudDisplayIndex: null,
  hudOpacity: 0.82,
  hudSizeMode: 'screen',
  hudScreenWidthPct: 22,
  hudScreenHeightPct: 28,
  hudManualWidth: 420,
  hudManualHeight: 260,
  hudVisibleMetrics: ['dome', 'sigma', 'pi', 'cpu', 'ram', 'target', 'power', 'energy'],
};
const HUD_METRIC_DEFAULTS = ['dome', 'sigma', 'pi', 'cpu', 'ram', 'target', 'power', 'energy'];
const MAX_DOME_HISTORY = 30;
/** Limite options `<select>` processus (évite milliers de nœuds DOM + gros tableaux JS). */
const MAX_PROCESS_SELECT_OPTIONS = 200;
const MAX_KPI_SESSIONS_IN_MEMORY = 20;
const MAX_BENCH_TOP_UI = 24;
/** Sessions A/B chargées depuis le disque : garde les derniers échantillons si export massif. */
const MAX_SAMPLES_PER_SESSION_UI = 500;
const PROCESS_REFRESH_ACTIVE_MS = 8000;
const PROCESS_REFRESH_IDLE_MS = 15000;
const PROCESS_REFRESH_HIDDEN_MS = 30000;
const ADAPTIVE_WORKLOAD_CONFIRM_CYCLES = 3;
const ADAPTIVE_WORKLOAD_COOLDOWN_MS = 30000;
const ADVICE_CONFIRM_CYCLES = 4;
const ADVICE_COOLDOWN_MS = 12000;
const POLL_FAST_MS = 700;
const POLL_MEDIUM_MS = 1000;
const POLL_SLOW_MS = 1400;
const POLL_HIDDEN_MS = 15000;
const POLL_UI_IDLE_MS = 5000;
const METRICS_AUDIT_MS = 10000;
const UI_IDLE_SLEEP_AFTER_MS = 45000;
let pollInFlight = false;
let pollTimer = null;
let clockIntervalId = null;
let processRefreshIntervalId = null;
let processRefreshInFlight = false;
let lastProcessReportRevision = null;
let lastProcessUiRevision = null;
let pendingUiMetric = null;
let uiFrameScheduled = false;
let lastMetricsAuditTs = 0;
let lastRenderedMetricKey = null;

// Polling loop (adaptive + low-overhead)

function nextPollDelayMs() {
  if (document.hidden) return POLL_HIDDEN_MS;
  if (shouldSleepWebview()) return POLL_UI_IDLE_MS;
  if (state.kpiBench.running) return POLL_MEDIUM_MS;
  if (state.domeActive || state.adaptiveEnabled) return POLL_FAST_MS;
  return POLL_SLOW_MS;
}

function nextProcessRefreshDelayMs() {
  if (document.hidden) return PROCESS_REFRESH_HIDDEN_MS;
  if (shouldSleepWebview()) return 60000;
  if (state.domeActive || state.adaptiveEnabled || state.kpiBench.running) return PROCESS_REFRESH_ACTIVE_MS;
  return PROCESS_REFRESH_IDLE_MS;
}

function markUiInteraction() {
  const now = Date.now();
  if (now - state.lastUserInteractionTs < 1500) return;
  state.lastUserInteractionTs = now;
}

function shouldSleepWebview() {
  if (document.hidden) return true;
  if (!state.windowFocused) return true;
  if (state.hudVisible || HUD_ONLY) return false;
  if (state.domeActive || state.adaptiveEnabled || state.kpiBench.running) return false;
  return (Date.now() - state.lastUserInteractionTs) >= UI_IDLE_SLEEP_AFTER_MS;
}

function scheduleNextPoll() {
  if (pollTimer) clearTimeout(pollTimer);
  pollTimer = setTimeout(poll, nextPollDelayMs());
}

function scheduleMetricRender(m) {
  if (document.hidden) return;
  pendingUiMetric = m;
  if (uiFrameScheduled) return;
  uiFrameScheduled = true;
  requestAnimationFrame(() => {
    uiFrameScheduled = false;
    const mm = pendingUiMetric;
    pendingUiMetric = null;
    if (!mm) return;
    const renderKey = JSON.stringify([
      Number(mm.raw?.cpu_pct || 0).toFixed(1),
      Number(mm.raw?.cpu_clock_mhz || 0).toFixed(0),
      Number(mm.raw?.cpu_max_clock_mhz || 0).toFixed(0),
      Number(mm.raw?.cpu_temp_c || 0).toFixed(1),
      Number(mm.raw?.mem_used_mb || 0),
      Number(mm.raw?.ram_clock_mhz || 0).toFixed(0),
      Number(mm.sigma || 0).toFixed(3),
      Number(mm.raw?.load_avg_1m_norm || 0).toFixed(2),
      Number(mm.raw?.gpu_pct || 0).toFixed(1),
      Number(mm.raw?.gpu_core_clock_mhz || 0).toFixed(0),
      Number(mm.raw?.gpu_mem_clock_mhz || 0).toFixed(0),
      Number(mm.raw?.gpu_temp_c || 0).toFixed(1),
      Number(mm.raw?.gpu_power_watts || 0).toFixed(1),
      Number(mm.raw?.io_read_mb_s || 0).toFixed(2),
      Number(mm.raw?.io_write_mb_s || 0).toFixed(2),
      state.domeActive,
      state.machineActivity || 'active',
      state.viewMode,
    ]);
    if (renderKey === lastRenderedMetricKey) return;
    lastRenderedMetricKey = renderKey;
    renderMetrics(mm);
    renderFormula(mm);
    renderTuningAdvice(mm);
    if (state.domeActive || state.snapshotBefore) renderProofPanel();
    if (state.hudVisible || HUD_ONLY) renderCompactHud(mm);
  });
}
let _metricsLoggedOnce = false;
async function poll() {
  if (pollInFlight) {
    scheduleNextPoll();
    return;
  }
  pollInFlight = true;
  try {
    const m = await invoke('get_metrics');
    state.lastMetrics = m;
    const now = Date.now();
    if (now - lastMetricsAuditTs >= METRICS_AUDIT_MS) {
      lastMetricsAuditTs = now;
      state.auditMetricSeq += 1;
      auditEmit('metrics', 'poll', 'info', {
        seq: state.auditMetricSeq,
        sigma: m.sigma,
        epsilon: m.epsilon,
        cpu: m.cpu,
        mem: m.mem,
        compression: m.compression,
        io_bandwidth: m.io_bandwidth,
        gpu: m.gpu,
        raw: m.raw,
      });
    }
    if (!_metricsLoggedOnce) {
      _metricsLoggedOnce = true;
      const r = m.raw;
      const na = v => (v != null ? String(v) : 'N/A');
      const memUsed = (r.mem_used_mb / 1024).toFixed(2);
      const memTotal = (r.mem_total_mb / 1024).toFixed(2);
      const sigma = (m.sigma != null ? Number(m.sigma).toFixed(3) : '—');
      const psi = (r.psi_cpu != null && r.psi_mem != null)
        ? `${(r.psi_cpu * 100).toFixed(0)} % / ${(r.psi_mem * 100).toFixed(0)} %`
        : 'n/a';
      log(
        `Métriques OK · ${r.platform} · CPU ${r.cpu_pct.toFixed(1)} % · RAM ${memUsed}/${memTotal} Go · σ=${sigma} · PSI ${psi}`,
        'ok',
      );
      log(
        `Détail (1 ligne) — swap ${r.swap_used_mb}/${r.swap_total_mb} Mo · zRAM ${na(r.zram_used_mb)} Mo · I/O R/W ${na(r.io_read_mb_s)}/${na(r.io_write_mb_s)} Mo/s · GPU ${na(r.gpu_pct)} %`,
        'info',
      );
    }
    // Detect machine activity state
    state.machineActivity = detectMachineActivity(m);
    // Accumulate real dome integral (only during active periods)
    if (state.domeActive && state.machineActivity === 'active' && state.lastPi != null) {
      const now = Date.now();
      if (state.domeRealLastTs != null) {
        const dt_s = Math.min((now - state.domeRealLastTs) / 1000, 30);
        state.domeRealIntegral += state.lastPi * dt_s;
      }
      state.domeRealLastTs = now;
    } else if (!state.domeActive) {
      state.domeRealLastTs = null;
    }
    if (!shouldSleepWebview()) scheduleMetricRender(m);
    await runAdaptiveController(m);
    pushTaskbarGauge(m.sigma);
    await ingestTelemetry(m);
    await refreshTelemetrySummary(false);
  } catch(e) {
    log(`Metrics error: ${e}`, 'err');
  } finally {
    pollInFlight = false;
    scheduleNextPoll();
  }
}

async function pushTaskbarGauge(sigma) {
  if (!hasTauri) return;
  const now = Date.now();
  if (now - state.lastTaskbarPushTs < 900) return;
  state.lastTaskbarPushTs = now;
  try {
    await invoke('set_taskbar_gauge', { value: sigma });
  } catch (_) {}
}
function clamp(v, min, max) {
  return Math.max(min, Math.min(max, v));
}

function getDominantResource(m) {
  const io = (m.io_bandwidth != null ? m.io_bandwidth : 0);
  const gpu = (m.gpu != null ? m.gpu : 0);
  const pairs = [
    ['cpu', m.cpu || 0],
    ['mem', m.mem || 0],
    ['io', io],
    ['gpu', gpu],
  ];
  pairs.sort((a, b) => b[1] - a[1]);
  return pairs[0];
}

function setSlidersFromState() {
  document.getElementById('kappaSlider').value = state.kappa.toFixed(1);
  document.getElementById('kappaNum').textContent = state.kappa.toFixed(1);
  document.getElementById('sigmaMaxSlider').value = state.sigmaMax.toFixed(2);
  document.getElementById('sigmaMaxNum').textContent = state.sigmaMax.toFixed(2);
  document.getElementById('smaxBot').textContent = state.sigmaMax.toFixed(2);
  document.getElementById('etaSlider').value = state.eta.toFixed(2);
  document.getElementById('etaNum').textContent = state.eta.toFixed(2);
  updateAdaptiveStatusText();
}

function updateAdaptiveStatusText(extra = '') {
  const statusEl = document.getElementById('adaptiveStatus');
  if (!statusEl) return;
  if (!state.adaptiveEnabled) {
    statusEl.textContent = extra || 'OFF';
    return;
  }
  statusEl.textContent =
    `ON | wl=${state.wl} k=${state.kappa.toFixed(2)} sMax=${state.sigmaMax.toFixed(2)} ` +
    `eta=${state.eta.toFixed(2)} policy=${state.policyMode}` +
    (extra ? ` | ${extra}` : '');
}


function setWorkload(nextWl, sourceLabel = "auto") {
  const wl = WORKLOADS[nextWl] ? nextWl : 'es';
  if (state.wl === wl) return;
  const prevWl = state.wl;
  state.wl = wl;
  state.workloadLastSwitchTs = Date.now();
  state.adaptiveWorkloadCandidate = null;
  state.adaptiveWorkloadCandidateCount = 0;
  syncWorkloadUiHighlight();
  logStateDiagWorkload(prevWl, wl, sourceLabel);
  if (state.lastMetrics) renderFormula(state.lastMetrics);
  loadBenchmarkHistory(true).catch(() => {});
}
function renderTuningAdvice(m) {
  const el = document.getElementById('tuningAdvice');
  const applyBtn = document.getElementById('btnApplyAdvice');
  if (!m || !m.raw || !el || !applyBtn) return;
  applyBtn.textContent = 'Appliquer';
  applyBtn.title = 'Appliquer la recommandation courante';

  const memRatio = m.raw.mem_total_mb > 0 ? m.raw.mem_used_mb / m.raw.mem_total_mb : 0;
  const [dominantKey, dominantVal] = getDominantResource(m);
  const learned = state.kpiBench?.tuningAdvice || null;
  const base = learned ? {
    kappa: Number(learned.recommended_kappa),
    sigmaMax: Number(learned.recommended_sigma_max),
    eta: Number(learned.recommended_eta),
    policyMode: String(learned.recommended_policy_mode || state.policyMode || 'privileged'),
    soulRamPercent: clamp(Number(learned.recommended_soulram_percent || state.soulRamPercent || 20), 10, 60),
  } : {
    kappa: state.kappa,
    sigmaMax: state.sigmaMax,
    eta: state.eta,
    policyMode: state.policyMode,
    soulRamPercent: state.soulRamPercent,
  };
  const next = { ...base };
  let reason = learned
    ? 'Base benchmark active: ajustement autour du meilleur profil appris'
    : 'Reglage stable';

  if (m.sigma >= 0.8 || memRatio >= 0.9) {
    next.kappa = clamp(Math.max(state.kappa, base.kappa) + 0.3, 0.5, 5.0);
    next.sigmaMax = clamp(Math.min(state.sigmaMax, base.sigmaMax) - 0.05, 0.3, 0.95);
    reason = learned
      ? 'Stress eleve: durcir temporairement le profil benchmark'
      : 'Stress eleve: renforcer la stabilite';
  } else if (learned && Number(learned.expected_efficiency_score || 0) >= 4.0 && m.sigma <= 0.55) {
    next.eta = clamp(Math.max(state.eta, base.eta) + 0.02, 0.01, 0.5);
    next.sigmaMax = clamp(Math.max(state.sigmaMax, base.sigmaMax) + 0.02, 0.3, 0.95);
    reason = 'Benchmark efficient: elargir legerement la fenetre autour du meilleur profil';
  } else if (learned && Number(learned.expected_efficiency_score || 0) <= -1.5) {
    next.kappa = clamp(Math.max(state.kappa, base.kappa) + 0.2, 0.5, 5.0);
    next.eta = clamp(Math.min(state.eta, base.eta) - 0.02, 0.01, 0.5);
    reason = 'Benchmark prudent: reduire la poussée automatique';
  } else if (dominantKey === 'cpu' && dominantVal >= 0.65) {
    next.eta = clamp(Math.max(state.eta, base.eta) + 0.03, 0.01, 0.5);
    reason = learned
      ? 'CPU dominant: pousser eta au-dessus de la base benchmark'
      : 'CPU dominant: pousser eta pour plus de perf';
  } else if (dominantKey === 'io' && dominantVal >= 0.55) {
    next.eta = clamp(Math.max(state.eta, base.eta) + 0.02, 0.01, 0.5);
    reason = learned
      ? 'I/O dominant: depasser moderement la base benchmark'
      : 'I/O dominant: augmenter eta moderement';
  } else if (learned && (
      Math.abs(state.kappa - base.kappa) >= 0.11 ||
      Math.abs(state.sigmaMax - base.sigmaMax) >= 0.031 ||
      Math.abs(state.eta - base.eta) >= 0.011
    )) {
    reason = 'Revenir au meilleur benchmark connu pour ce workload';
  } else if (!learned && m.sigma <= 0.35 && state.sigmaMax < 0.9) {
    next.sigmaMax = clamp(state.sigmaMax + 0.05, 0.3, 0.95);
    reason = 'Stress faible: augmenter la marge sigma max';
  } else {
    reason = learned
      ? 'Profil benchmark deja optimal pour la charge actuelle'
      : 'Ajustement non necessaire maintenant';
  }

  const changes = [];
  if (next.kappa !== state.kappa) changes.push(`kappa ${state.kappa.toFixed(1)} -> ${next.kappa.toFixed(1)}`);
  if (next.sigmaMax !== state.sigmaMax) changes.push(`sigmaMax ${state.sigmaMax.toFixed(2)} -> ${next.sigmaMax.toFixed(2)}`);
  if (next.eta !== state.eta) changes.push(`eta ${state.eta.toFixed(2)} -> ${next.eta.toFixed(2)}`);
  const learnedTxt = learned
    ? ` Base benchmark ${base.kappa.toFixed(1)}/${base.sigmaMax.toFixed(2)}/${base.eta.toFixed(2)}${
        learned.expected_gain_median_pct == null ? '' : ` | gain median ${Number(learned.expected_gain_median_pct).toFixed(1)}%`
      }${
        learned.expected_gain_p95_pct == null ? '' : ` | p95 ${Number(learned.expected_gain_p95_pct).toFixed(1)}%`
      }${
        learned.expected_efficiency_score == null ? '' : ` | eff ${Number(learned.expected_efficiency_score).toFixed(2)}`
      }${
        learned.recommended_policy_mode ? ` | policy ${String(learned.recommended_policy_mode).toUpperCase()}` : ''
      }${
        learned.recommended_soulram_percent != null ? ` | SoulRAM ${Number(learned.recommended_soulram_percent)}%` : ''
      }.`
    : '';

  if (!changes.length) {
    state.pendingAdvice = null;
    state.adviceCandidateKey = null;
    state.adviceCandidateCount = 0;
    state.adviceCurrentKey = null;
    applyBtn.disabled = true;
    applyBtn.style.opacity = '.55';
    el.textContent = `${reason}.${learnedTxt}`;
    return;
  }

  const now = Date.now();
  const candidatePayload = {
    kappa: Number(next.kappa.toFixed(2)),
    sigmaMax: Number(next.sigmaMax.toFixed(3)),
    eta: Number(next.eta.toFixed(3)),
  };
  const candidateKey = JSON.stringify(candidatePayload);

  if (state.adviceCandidateKey !== candidateKey) {
    state.adviceCandidateKey = candidateKey;
    state.adviceCandidateCount = 1;
  } else {
    state.adviceCandidateCount += 1;
  }

  const stableEnough = state.adviceCandidateCount >= ADVICE_CONFIRM_CYCLES;
  const cooldownOk = (now - state.adviceLastAcceptedTs) >= ADVICE_COOLDOWN_MS;

  if (stableEnough && cooldownOk) {
    state.pendingAdvice = next;
    state.adviceCurrentKey = candidateKey;
    applyBtn.disabled = false;
    applyBtn.style.opacity = '1';
    el.textContent = `${reason}. Recommande: ${changes.join(', ')}.${learnedTxt}`;
  } else if (state.pendingAdvice && state.adviceCurrentKey === candidateKey) {
    applyBtn.disabled = false;
    applyBtn.style.opacity = '1';
    el.textContent = `${reason}. Recommande: ${changes.join(', ')}.${learnedTxt}`;
  } else {
    state.pendingAdvice = null;
    applyBtn.disabled = true;
    applyBtn.style.opacity = '.55';
    el.textContent = `${reason}. Stabilisation en cours (${state.adviceCandidateCount}/${ADVICE_CONFIRM_CYCLES}).${learnedTxt}`;
  }
}

function tokenizeArgs(raw) {
  if (!raw || !raw.trim()) return [];
  const re = /"([^"]*)"|'([^']*)'|(\S+)/g;
  const out = [];
  let m;
  while ((m = re.exec(raw)) !== null) {
    out.push(m[1] ?? m[2] ?? m[3]);
  }
  return out;
}

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

function medianSorted(sorted) {
  const n = sorted.length;
  if (n === 0) return null;
  if (n % 2 === 1) return sorted[(n - 1) / 2];
  return (sorted[n / 2 - 1] + sorted[n / 2]) / 2;
}

function medianOf(values) {
  if (!values || values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  return medianSorted(sorted);
}

/** Percentile nearest-rank (aligné Rust benchmark) : rang = ceil(p/100 × n). */
function percentileNearestRank(values, p) {
  if (!values || values.length === 0 || p === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const n = sorted.length;
  const rank = Math.ceil((p / 100) * n);
  const idx = Math.max(0, Math.min(n - 1, rank - 1));
  return sorted[idx];
}

function gainLowerIsBetterMedians(offVals, onVals, minPositive) {
  const mo = medianOf(offVals);
  const mn = medianOf(onVals);
  if (mo == null || mn == null || mo < minPositive) return null;
  return ((mo - mn) / mo) * 100;
}

function computeAbSummary(samples) {
  const off = samples.filter(s => s.phase === 'off' && s.success).map(s => s.duration_ms);
  const on = samples.filter(s => s.phase === 'on' && s.success).map(s => s.duration_ms);
  const medianOff = medianOf(off);
  const medianOn = medianOf(on);
  const p95Off = percentileNearestRank(off, 95);
  const p95On = percentileNearestRank(on, 95);
  let gainMedianPct = null;
  let gainP95Pct = null;
  if (medianOff != null && medianOn != null && medianOff > 0) {
    gainMedianPct = ((medianOff - medianOn) / medianOff) * 100;
  }
  if (p95Off != null && p95On != null && p95Off > 0) {
    gainP95Pct = ((p95Off - p95On) / p95Off) * 100;
  }

  const numOk = v => v != null && Number.isFinite(Number(v));
  const memOff = samples.filter(s => s.phase === 'off' && s.success && numOk(s.mem_after_gb)).map(s => Number(s.mem_after_gb));
  const memOn = samples.filter(s => s.phase === 'on' && s.success && numOk(s.mem_after_gb)).map(s => Number(s.mem_after_gb));
  const gpuOff = samples.filter(s => s.phase === 'off' && s.success && numOk(s.gpu_after_pct)).map(s => Number(s.gpu_after_pct));
  const gpuOn = samples.filter(s => s.phase === 'on' && s.success && numOk(s.gpu_after_pct)).map(s => Number(s.gpu_after_pct));
  const cpuOff = samples.filter(s => s.phase === 'off' && s.success && numOk(s.cpu_after_pct)).map(s => Number(s.cpu_after_pct));
  const cpuOn = samples.filter(s => s.phase === 'on' && s.success && numOk(s.cpu_after_pct)).map(s => Number(s.cpu_after_pct));
  const powerOff = samples.filter(s => s.phase === 'off' && s.success && numOk(s.power_after_watts)).map(s => Number(s.power_after_watts));
  const powerOn = samples.filter(s => s.phase === 'on' && s.success && numOk(s.power_after_watts)).map(s => Number(s.power_after_watts));
  const sigmaOff = samples.filter(s => s.phase === 'off' && s.success && numOk(s.sigma_effective_after)).map(s => Number(s.sigma_effective_after));
  const sigmaOn = samples.filter(s => s.phase === 'on' && s.success && numOk(s.sigma_effective_after)).map(s => Number(s.sigma_effective_after));
  const cpuTempOff = samples.filter(s => s.phase === 'off' && s.success && numOk(s.cpu_temp_after_c)).map(s => Number(s.cpu_temp_after_c));
  const cpuTempOn = samples.filter(s => s.phase === 'on' && s.success && numOk(s.cpu_temp_after_c)).map(s => Number(s.cpu_temp_after_c));
  const gpuTempOff = samples.filter(s => s.phase === 'off' && s.success && numOk(s.gpu_temp_after_c)).map(s => Number(s.gpu_temp_after_c));
  const gpuTempOn = samples.filter(s => s.phase === 'on' && s.success && numOk(s.gpu_temp_after_c)).map(s => Number(s.gpu_temp_after_c));

  const gainPower = gainLowerIsBetterMedians(powerOff, powerOn, 1);
  const gainSigma = gainLowerIsBetterMedians(sigmaOff, sigmaOn, 0.05);
  const gainCpuTemp = gainLowerIsBetterMedians(cpuTempOff, cpuTempOn, 1);
  const gainGpuTemp = gainLowerIsBetterMedians(gpuTempOff, gpuTempOn, 1);
  const efficiencyScore =
    (Number(gainMedianPct || 0) * 0.45) +
    (Number(gainP95Pct || 0) * 0.20) +
    (Number(gainLowerIsBetterMedians(memOff, memOn, 0.05) || 0) * 0.08) +
    (Number(gainLowerIsBetterMedians(gpuOff, gpuOn, 1) || 0) * 0.07) +
    (Number(gainLowerIsBetterMedians(cpuOff, cpuOn, 2) || 0) * 0.07) +
    (Number(gainPower || 0) * 0.08) +
    (Number(gainSigma || 0) * 0.05);

  return {
    samples_off_ok: off.length,
    samples_on_ok: on.length,
    median_off_ms: medianOff,
    median_on_ms: medianOn,
    p95_off_ms: p95Off,
    p95_on_ms: p95On,
    gain_median_pct: gainMedianPct,
    gain_p95_pct: gainP95Pct,
    gain_mem_median_pct: gainLowerIsBetterMedians(memOff, memOn, 0.05),
    gain_gpu_median_pct: gainLowerIsBetterMedians(gpuOff, gpuOn, 1),
    gain_cpu_median_pct: gainLowerIsBetterMedians(cpuOff, cpuOn, 2),
    gain_power_median_pct: gainPower,
    gain_sigma_median_pct: gainSigma,
    gain_cpu_temp_median_pct: gainCpuTemp,
    gain_gpu_temp_median_pct: gainGpuTemp,
    efficiency_score: efficiencyScore,
  };
}

function renderAbSummary(summary) {
  const el = document.getElementById('kpiBenchSummary');
  if (!el) return;
  if (!summary) {
    el.textContent = 'Aucun benchmark A/B';
    const verdictEl = document.getElementById('benchmarkVerdict');
    if (verdictEl) verdictEl.textContent = 'Aucun benchmark charge.';
    return;
  }
  const f = v => (v == null ? 'N/A' : v.toFixed(1));
  const fg = k => (summary[k] == null || !Number.isFinite(Number(summary[k])) ? null : Number(summary[k]));
  const resBits = [
    fg('gain_mem_median_pct') != null ? `RAM ${f(fg('gain_mem_median_pct'))}%` : null,
    fg('gain_gpu_median_pct') != null ? `GPU ${f(fg('gain_gpu_median_pct'))}%` : null,
    fg('gain_cpu_median_pct') != null ? `CPU ${f(fg('gain_cpu_median_pct'))}%` : null,
    fg('gain_power_median_pct') != null ? `W ${f(fg('gain_power_median_pct'))}%` : null,
    fg('gain_sigma_median_pct') != null ? `Sigma ${f(fg('gain_sigma_median_pct'))}%` : null,
    fg('gain_cpu_temp_median_pct') != null ? `Tcpu ${f(fg('gain_cpu_temp_median_pct'))}%` : null,
    fg('gain_gpu_temp_median_pct') != null ? `Tgpu ${f(fg('gain_gpu_temp_median_pct'))}%` : null,
  ].filter(Boolean);
  const resSuffix = resBits.length ? ` | apres sonde (med.): ${resBits.join(' · ')}` : '';
  el.textContent =
    `OFF median=${f(summary.median_off_ms)}ms p95=${f(summary.p95_off_ms)}ms | ` +
    `ON median=${f(summary.median_on_ms)}ms p95=${f(summary.p95_on_ms)}ms | ` +
    `gain temps median=${f(summary.gain_median_pct)}% p95=${f(summary.gain_p95_pct)}%` +
    resSuffix;
  const verdictEl = document.getElementById('benchmarkVerdict');
  if (verdictEl) {
    const gain = Number(summary.gain_median_pct ?? 0);
    const eff = fg('efficiency_score');
    const verdict = gain > 3 ? 'Gain net' : (gain < -3 ? 'Regression' : 'Neutre');
    verdictEl.textContent = `${verdict} | median=${f(summary.gain_median_pct)}% | p95=${f(summary.gain_p95_pct)}% | score=${eff == null ? 'N/A' : eff.toFixed(2)}`;
  }
}

function renderBenchmarkLearning(advice) {
  const el = document.getElementById('kpiBenchLearned');
  if (!el) return;
  if (!advice) {
    el.textContent = 'Apprentissage benchmark: aucun historique pertinent';
    return;
  }
  const gain = advice.expected_gain_median_pct == null ? 'N/A' : Number(advice.expected_gain_median_pct).toFixed(1) + '%';
  const gainP95 = advice.expected_gain_p95_pct == null ? 'N/A' : Number(advice.expected_gain_p95_pct).toFixed(1) + '%';
  const eff = advice.expected_efficiency_score == null ? 'N/A' : Number(advice.expected_efficiency_score).toFixed(2);
  const policy = String(advice.recommended_policy_mode || 'privileged').toUpperCase();
  const soulram = advice.recommended_soulram_percent == null ? 'N/A' : `${Number(advice.recommended_soulram_percent)}%`;
  const conf = Math.round(Number(advice.confidence || 0) * 100);
  el.textContent =
    `Apprentissage benchmark: κ=${Number(advice.recommended_kappa).toFixed(1)} ` +
    `Σmax=${Number(advice.recommended_sigma_max).toFixed(2)} ` +
    `η=${Number(advice.recommended_eta).toFixed(2)} | ` +
    `policy=${policy} | SoulRAM=${soulram} | gain median=${gain} | p95=${gainP95} | efficiency=${eff} | score=${Number(advice.composite_score || 0).toFixed(2)} | confiance=${conf}% | echantillons=${advice.sample_size}`;
}

function renderBenchmarkTop(topSessions) {
  const el = document.getElementById('benchmarkTopList');
  if (!el) return;
  const top = (Array.isArray(topSessions) ? topSessions : []).slice(0, MAX_BENCH_TOP_UI);
  if (!top.length) {
    el.innerHTML = '<div class="advisor-text">Aucun classement disponible.</div>';
    refreshSoulKernelLucide();
    return;
  }
  const f = v => (v == null ? 'N/A' : Number(v).toFixed(1) + '%');
  el.innerHTML = top.map(item => (
    `<div class="bench-top-item">` +
      `<div class="bench-top-title" style="display:flex;align-items:center;gap:.35rem"><span class="pt-ico" style="width:13px;height:13px"><i data-lucide="award"></i></span><span>#${item.rank} score=${Number(item.composite_score || 0).toFixed(2)} | ${item.started_at}</span></div>` +
      `<div class="bench-top-meta">gain median=${f(item.gain_median_pct)} | p95=${f(item.gain_p95_pct)} | runs=${item.runs_per_state}/etat</div>` +
      `<div class="bench-top-meta">κ=${Number(item.kappa).toFixed(1)} | Σmax=${Number(item.sigma_max).toFixed(2)} | η=${Number(item.eta).toFixed(2)} | wl=${item.workload}</div>` +
    `</div>`
  )).join('');
  refreshSoulKernelLucide();
}

function applyBenchmarkLearning(advice, sourceLabel = 'benchmark-history') {
  if (!advice) return;
  state.kpiBench.tuningAdvice = advice;
  state.kappa = Number(advice.recommended_kappa);
  state.sigmaMax = Number(advice.recommended_sigma_max);
  state.eta = Number(advice.recommended_eta);
  state.policyMode = (advice.recommended_policy_mode === 'safe' || advice.recommended_policy_mode === 'privileged')
    ? advice.recommended_policy_mode
    : state.policyMode;
  state.soulRamPercent = clamp(Number(advice.recommended_soulram_percent || state.soulRamPercent || 20), 10, 60);
  setSlidersFromState();
  const policySel = document.getElementById('policyMode');
  if (policySel) policySel.value = state.policyMode;
  const soulRamSlider = document.getElementById('soulRamPct');
  if (soulRamSlider) soulRamSlider.value = String(state.soulRamPercent);
  const soulRamPctLabel = document.getElementById('soulRamPctLabel');
  if (soulRamPctLabel) soulRamPctLabel.textContent = `${state.soulRamPercent}%`;
  if (state.lastMetrics) renderFormula(state.lastMetrics);
  renderBenchmarkLearning(advice);
  log(
    'Benchmark learning applique: κ=' + state.kappa.toFixed(1) +
    ' sigmaMax=' + state.sigmaMax.toFixed(2) +
    ' eta=' + state.eta.toFixed(2) +
    ' policy=' + state.policyMode +
    ' soulram=' + state.soulRamPercent + '%' +
    ' (' + sourceLabel + ')',
    'info'
  );
  saveRuntimeSettings();
  saveStartupIntent();
}



function detectMachineActivity(m) {
  if (!m || !m.raw) return 'active';
  const cpu = Number(m.raw.cpu_pct || 0);
  const gpuPct = Number(m.raw.gpu_pct || 0);
  const ioR = Number(m.raw.io_read_mb_s || 0);
  const ioW = Number(m.raw.io_write_mb_s || 0);
  const ioTotal = ioR + ioW;
  // WebView2 (Tauri) peut élever la jauge GPU sans lecture vidéo → éviter le faux « media »
  // qui ignorait les gains télémetrie (CPU·h / ∫𝒟).
  const wvMem = Number(m.raw.webview_host_mem_mb || 0);
  const gpuAdj = wvMem >= 48 ? Math.max(0, gpuPct - 18) : gpuPct;
  // Media: décodage vidéo typique (GPU nettement sollicité, CPU modéré)
  if (cpu < 12 && gpuAdj > 34) return 'media';
  // Idle: machine calme (seuils un peu plus stricts sur GPU pour l’UI WebView)
  if (cpu < 8 && ioTotal < 0.5 && gpuPct < 8) return 'idle';
  return 'active';
}

async function ingestTelemetry(m) {
  if (!hasTauri || !m || !m.raw) return;
  const now = Date.now();
  if (now - state.lastTelemetryIngestTs < 5000) return;
  state.lastTelemetryIngestTs = now;
  const gain = state.kpiBench?.lastSummary?.gain_median_pct;
  try {
    await invoke('ingest_telemetry_sample', {
      sample: {
        ts_ms: now,
        power_watts: (m.raw.power_watts != null ? Number(m.raw.power_watts) : null),
        dome_active: !!state.domeActive,
        soulram_active: !!state.soulRamActive,
        kpi_gain_median_pct: (gain != null ? Number(gain) : null),
        cpu_pct: (m.raw.cpu_pct != null ? Number(m.raw.cpu_pct) : null),
        pi: (state.lastPi != null ? Number(state.lastPi) : null),
        machine_activity: state.machineActivity || 'active',
        mem_used_mb: (m.raw.mem_used_mb != null ? Number(m.raw.mem_used_mb) : null),
        mem_total_mb: (m.raw.mem_total_mb != null ? Number(m.raw.mem_total_mb) : null),
        power_source_tag: (m.raw.power_watts_source != null ? String(m.raw.power_watts_source) : null),
      }
    });
  } catch (_) {}
}

function renderTelemetrySummary(s) {
  if (!s) return;
  state.telemetrySummary = s;
  const ccy = s.pricing?.currency || 'EUR';
  const f = v => (v == null ? 'N/A' : Number(v).toFixed(3));
  const fmtEnergy = (w, key) => (w?.has_power_data ? `${f(w?.[key])}` : 'N/A');
  const gain = s.total?.kpi_gain_median_pct;
  set('rawOptReal', gain == null ? 'N/A' : gain.toFixed(2) + '%');
  const mgb = s.total?.mem_gb_hours_differential;
  set('rawMemGbHTel', mgb != null && Number.isFinite(Number(mgb)) ? Number(mgb).toFixed(3) : 'N/A');
  set('rawEnergyTotal', fmtEnergy(s.total, 'energy_kwh') + ' kWh');
  set('rawCostTotal', (s.total?.has_power_data ? f(s.total?.cost) : 'N/A') + ' ' + ccy);
  set('rawEnergyWindows',
    `H:${fmtEnergy(s.hour, 'energy_kwh')} | J:${fmtEnergy(s.day, 'energy_kwh')} | S:${fmtEnergy(s.week, 'energy_kwh')} | M:${fmtEnergy(s.month, 'energy_kwh')} | A:${fmtEnergy(s.year, 'energy_kwh')} kWh`
  );

  const ep = document.getElementById('energyPrice');
  if (ep && s.pricing?.price_per_kwh != null) ep.value = Number(s.pricing.price_per_kwh).toFixed(3);
  const eco2 = document.getElementById('energyCo2');
  if (eco2 && s.pricing?.co2_kg_per_kwh != null) eco2.value = Number(s.pricing.co2_kg_per_kwh).toFixed(3);

  const status = document.getElementById('energyPricingStatus');
  if (status) {
    const sensor = s.total?.has_power_data ? 'capteur puissance: detecte' : 'capteur puissance: indisponible';
    status.textContent = 'Tarif actif: ' + f(s.pricing?.price_per_kwh) + ' ' + ccy + '/kWh | CO2: ' + f(s.pricing?.co2_kg_per_kwh) + ' kg/kWh | ' + sensor;
    status.style.color = s.total?.has_power_data ? 'var(--io)' : 'var(--gpu)';
  }

  // ── GREEN IT panel ──────────────────────────────────────────────────────
  renderGreenItPanel(s);
  renderSoulRamFromTelemetry(s);
}

function renderSoulRamFromTelemetry(s) {
  const el = document.getElementById('soulRamTelemetryLine');
  if (!el) return;
  if (!s || !s.lifetime) {
    el.textContent = 'Lance l’app native et laisse tourner la télémétrie pour cumuler les durées réelles.';
    return;
  }
  const lt = s.lifetime;
  const sh = Number(lt.soulram_active_hours ?? 0);
  const pch = s.total?.passive_clean_h != null ? Number(s.total.passive_clean_h) : null;
  const memGb = lt.total_mem_gb_hours_differential != null ? Number(lt.total_mem_gb_hours_differential) : 0;
  el.innerHTML =
    'Durée cumulée <strong>SoulRAM ON + dôme OFF</strong> (Δt entre échantillons réels) : <strong>' +
    (Number.isFinite(sh) ? sh.toFixed(2) : '0') + ' h</strong> · ' +
    'Même notion sur la fenêtre télémétrie courante : <strong>' +
    (pch != null && Number.isFinite(pch) ? pch.toFixed(2) + ' h</strong>' : 'N/A</strong>') + '. ' +
    '<span style="opacity:.9">Le cumul <strong>RAM·GB·h</strong> affiché ailleurs reflète surtout le <strong>dôme</strong> actif, pas un « gain SoulRAM » isolé.</span>';
}

function renderGreenItPanel(s) {
  if (!s) return;
  const lt = s.lifetime;
  if (!lt) return;
  const f2 = v => (v == null ? '0' : Number(v).toFixed(2));
  const f3 = v => (v == null ? '0' : Number(v).toFixed(3));

  // Source badge
  const srcMap = {
    rapl: 'RAPL',
    battery: 'BATTERIE',
    cpu_differential: 'CPU DELTA',
    meross_wall: 'PRISE (mur)',
    windows_meter: 'WINDOWS',
  };
  set('greenItSource', srcMap[s.power_source] || s.power_source || '--');

  // Main stats
  set('greenCpuH', f2(lt.total_cpu_hours_differential));
  set('greenMemGh', f2(lt.total_mem_gb_hours_differential ?? 0));
  set('greenCo2', f3(lt.total_co2_measured_kg));
  set('greenKwh', lt.has_real_power ? f3(lt.total_energy_kwh) : '--');
  set('greenDomeN', String(lt.total_dome_activations || 0));
  set('greenDomeH', f2(lt.total_dome_hours));
  set('greenDomeD', f2(lt.total_dome_gain_integral));
  set('greenHourKwh', s.hour?.has_power_data ? f3(s.hour?.energy_kwh) : '--');
  set('greenHourEur', s.hour?.has_power_data ? (f2(s.hour?.cost) + ' ' + (s.pricing?.currency || 'EUR')) : '--');
  set('greenDayKwh', s.day?.has_power_data ? f3(s.day?.energy_kwh) : '--');
  set('greenDayEur', s.day?.has_power_data ? (f2(s.day?.cost) + ' ' + (s.pricing?.currency || 'EUR')) : '--');
  set('greenWeekKwh', s.week?.has_power_data ? f3(s.week?.energy_kwh) : '--');
  set('greenWeekEur', s.week?.has_power_data ? (f2(s.week?.cost) + ' ' + (s.pricing?.currency || 'EUR')) : '--');
  set('greenMonthKwh', s.month?.has_power_data ? f3(s.month?.energy_kwh) : '--');
  set('greenMonthEur', s.month?.has_power_data ? (f2(s.month?.cost) + ' ' + (s.pricing?.currency || 'EUR')) : '--');

  // Lifetime text
  const ltEl = document.getElementById('greenItLifetime');
  if (ltEl && lt.first_launch_ts > 0) {
    const firstDate = new Date(lt.first_launch_ts).toLocaleDateString('fr-FR');
    let text = `Depuis le ${firstDate} : `;
    text += `<strong>${lt.total_dome_activations}</strong> activations, `;
    text += `<strong>${f2(lt.total_dome_hours)}</strong> h de dome, `;
      text += `<strong>${f2(lt.total_cpu_hours_differential)}</strong> CPU-h (diff. mesure), `;
    text += `<strong>${f2(lt.total_mem_gb_hours_differential ?? 0)}</strong> RAM·GB·h (diff. mesure)`;
    if (lt.has_real_power) {
      text += ` | <strong>${f3(lt.total_energy_kwh)}</strong> kWh mesures`;
      text += `, <strong>${f3(lt.total_co2_measured_kg)}</strong> kg CO2 mesures`;
      text += `, <strong>${f2(lt.total_energy_cost_measured)}</strong> ${s.pricing?.currency || 'EUR'} cout energie mesure`;
    }
    if (lt.avg_kpi_gain_pct != null) {
      text += ` | gain KPI median: <strong>${Number(lt.avg_kpi_gain_pct).toFixed(1)}%</strong>`;
    }
    const idleH = Number(lt.total_idle_hours || 0);
    const mediaH = Number(lt.total_media_hours || 0);
    if (idleH > 0.01 || mediaH > 0.01) {
      text += ` <span style="color:var(--muted)">(hors ${f2(idleH)}h idle + ${f2(mediaH)}h media)</span>`;
    }
    ltEl.innerHTML = text;
  } else if (ltEl) {
    ltEl.textContent = 'Premier lancement en attente de donnees...';
  }

  // Sigma gauge green indicator
  const sigmaGauge = document.querySelector('.sigma-gauge');
  if (sigmaGauge) {
    sigmaGauge.classList.toggle('green-saving', !!state.domeActive && lt.total_cpu_hours_differential > 0);
  }

  // HUD compact enrichment
  const hudCo2 = document.getElementById('hudCo2');
  if (hudCo2) hudCo2.textContent = lt.has_real_power ? f3(lt.total_co2_measured_kg) + ' kg' : '--';
  const hudCpuH = document.getElementById('hudCpuH');
  if (hudCpuH) hudCpuH.textContent = f2(lt.total_cpu_hours_differential) + ' h';
  const hudMemGh = document.getElementById('hudMemGh');
  if (hudMemGh) hudMemGh.textContent = f2(lt.total_mem_gb_hours_differential ?? 0) + ' GB·h';
}

async function refreshTelemetrySummary(force = false) {
  if (!hasTauri) return;
  const now = Date.now();
  const minDelay = shouldSleepWebview() ? 30000 : 10000;
  if (!force && (now - state.lastTelemetryRefreshTs < minDelay)) return;
  state.lastTelemetryRefreshTs = now;
  try {
    const s = await invoke('get_telemetry_summary');
    renderTelemetrySummary(s);
  } catch (_) {}
}

async function setEnergyPricing(pricePerKwh, currency = 'EUR', co2KgPerKwh = 0.05) {
  if (!hasTauri) return;
  await invoke('set_energy_pricing', {
    pricing: {
      currency,
      price_per_kwh: Number(pricePerKwh),
      co2_kg_per_kwh: Number(co2KgPerKwh),
    }
  });
  await refreshTelemetrySummary(true);
  const status = document.getElementById('energyPricingStatus');
  if (status) {
    const now = new Date().toTimeString().slice(0, 8);
    status.textContent = 'Tarif enregistre a ' + now + ' -> ' + Number(pricePerKwh).toFixed(3) + ' ' + currency + '/kWh | CO2 ' + Number(co2KgPerKwh).toFixed(3) + ' kg/kWh';
    status.style.color = 'var(--io)';
  }
}

function renderExternalPowerConfig(cfg) {
  const enabled = document.getElementById('merossEnabled');
  if (enabled) enabled.checked = !!cfg?.enabled;
  const powerFile = document.getElementById('merossPowerFile');
  if (powerFile) powerFile.value = cfg?.power_file || '';
  const maxAge = document.getElementById('merossMaxAgeMs');
  if (maxAge) maxAge.value = String(Number(cfg?.max_age_ms || 15000));
  const email = document.getElementById('merossEmail');
  if (email) email.value = cfg?.meross_email || '';
  const pwd = document.getElementById('merossPassword');
  if (pwd) pwd.value = cfg?.meross_password || '';
  const region = document.getElementById('merossRegion');
  if (region) region.value = cfg?.meross_region || 'eu';
  const devType = document.getElementById('merossDeviceType');
  if (devType) devType.value = cfg?.meross_device_type || 'mss315';
  const httpProxy = document.getElementById('merossHttpProxy');
  if (httpProxy) httpProxy.value = cfg?.meross_http_proxy || '';
  const mfaCode = document.getElementById('merossMfaCode');
  if (mfaCode) mfaCode.value = cfg?.meross_mfa_code || '';
  const py = document.getElementById('merossPythonBin');
  if (py) py.value = cfg?.python_bin || '';
  const interval = document.getElementById('merossBridgeInterval');
  if (interval) interval.value = String(Number(cfg?.bridge_interval_s || 8));
  const autostart = document.getElementById('merossAutostartBridge');
  if (autostart) autostart.checked = !!cfg?.autostart_bridge;
}

function renderExternalPowerStatus(status) {
  if (!status) return;
  const watts = status.lastWatts != null && Number.isFinite(Number(status.lastWatts))
    ? Number(status.lastWatts).toFixed(2) + ' W'
    : 'N/A';
  const freshness = !status.enabled
    ? 'source OFF'
    : (status.isFresh ? 'frais' : 'stale');
  const fileState = status.powerFileExists ? 'présent' : 'absent';
  const tsLabel = status.lastTsMs ? new Date(Number(status.lastTsMs)).toLocaleString('fr-FR') : '—';

  set('merossSourceTag', status.sourceTag || 'meross_wall');
  set('merossLastWatts', watts);
  set('merossFreshness', freshness);
  set('merossFilePresence', fileState);
  set('merossCredentialsState', status.credentialsPresent ? 'ok' : 'manquant');
  set('merossConfigPath', status.configPath || '—');
  set('merossResolvedPowerFile', status.powerFilePath || '—');
  set('merossCredsCachePath', status.credsCachePath || '—');
  set('merossLastTs', tsLabel);
  set('merossBridgeLogPath', status.bridgeLogPath || '—');
  set('merossHttpProxyStatus', status.merossHttpProxy || 'aucun');
  set('merossMfaState', status.mfaPresent ? 'présent' : 'absent');
  const runtimeChip = document.getElementById('merossPythonRuntime');
  if (runtimeChip) {
    const configured = String(status.pythonBin || '').trim();
    runtimeChip.textContent = configured ? 'Python forcé' : 'Python auto';
  }

  const info = document.getElementById('merossConfigStatus');
  if (info) {
    info.textContent =
      'Config ' + (status.configExists ? 'trouvée' : 'absente') +
      ' | JSON puissance ' + fileState +
      ' | source ' + (status.enabled ? 'active' : 'désactivée');
    info.style.color = status.enabled && status.isFresh ? 'var(--io)' : 'var(--muted)';
  }

  const bridge = document.getElementById('merossBridgeCommand');
  if (bridge) {
    const outPath = status.powerFilePath || '~/.config/soulkernel/meross_power.json';
    const pythonBin = String(status.pythonBin || status.defaultPythonHint || 'python3').trim() || 'python3';
    bridge.textContent = `${pythonBin} scripts/meross_mss315_bridge.py --out "${outPath}"`;
  }
}

function renderExternalBridgeStatus(status) {
  if (!status) return;
  const bridgeOn = !!status.running;
  const hasError = !!String(status.lastError || '').trim();
  const hasMeasure = String(document.getElementById('merossLastWatts')?.textContent || '').trim() !== 'N/A';
  const freshness = String(document.getElementById('merossFreshness')?.textContent || '').trim();
  const overall = document.getElementById('merossOverallStatus');
  const summary = document.getElementById('merossOverallSummary');
  const actionHint = document.getElementById('merossActionHint');
  const runtimeChip = document.getElementById('merossPythonRuntime');
  set('merossBridgeRunning', status.running ? 'ON' : 'OFF');
  set('merossBridgeLogPath', status.bridgeLogPath || '—');
  set('merossBridgeScriptPath', status.scriptPath || '—');
  set('merossBridgeError', status.lastError || '—');
  const bridge = document.getElementById('merossBridgeCommand');
  if (bridge && status.scriptPath) {
    const outPath = document.getElementById('merossResolvedPowerFile')?.textContent || '~/.config/soulkernel/meross_power.json';
    const pythonBin = String(status.resolvedPythonBin || '').trim() || 'python3';
    const sourceLabel = status.pythonSource === 'embedded' ? 'embedded' : 'system';
    bridge.textContent = `${pythonBin} "${status.scriptPath}" --out "${outPath}"  # ${sourceLabel}`;
  }
  if (runtimeChip) {
    const sourceLabel = status.pythonSource === 'embedded' ? 'Runtime embarqué' : (status.pythonSource === 'system' ? 'Python système' : 'Runtime inconnu');
    runtimeChip.textContent = sourceLabel;
    runtimeChip.style.color = status.pythonSource === 'embedded' ? 'var(--io)' : 'var(--warning)';
    runtimeChip.style.borderColor = status.pythonSource === 'embedded' ? 'rgba(0,255,157,.35)' : 'var(--border)';
  }
  if (overall && summary && actionHint) {
    if (bridgeOn && hasMeasure && freshness === 'frais') {
      overall.textContent = 'Mesure murale active';
      overall.style.color = 'var(--io)';
      summary.textContent = 'La prise externe alimente désormais SoulKernel avec une mesure secteur fraîche.';
      actionHint.textContent = 'Aucune action requise. Les watts et les intégrations d’énergie passent par la prise externe.';
      actionHint.style.color = 'var(--io)';
    } else if (bridgeOn && hasError) {
      overall.textContent = 'Bridge lancé, mais bloqué';
      overall.style.color = 'var(--warning)';
      summary.textContent = 'Le bridge tourne, mais il ne livre pas encore de mesure valide. Vérifie l’erreur et le log.';
      actionHint.textContent = String(status.lastError || '').trim();
      actionHint.style.color = 'var(--warning)';
    } else if (bridgeOn) {
      overall.textContent = 'Bridge actif, attente de mesure';
      overall.style.color = 'var(--cpu)';
      summary.textContent = 'Le bridge est démarré, mais aucun JSON de puissance frais n’a encore été reçu.';
      actionHint.textContent = 'Attends quelques secondes puis rafraîchis l’état. Si rien n’apparaît, ouvre le diagnostic technique.';
      actionHint.style.color = 'var(--muted)';
    } else if (hasError) {
      overall.textContent = 'Bridge arrêté avec erreur';
      overall.style.color = 'var(--stress)';
      summary.textContent = 'Le bridge s’est arrêté avant de produire une mesure secteur exploitable.';
      actionHint.textContent = String(status.lastError || '').trim();
      actionHint.style.color = 'var(--stress)';
    } else {
      overall.textContent = 'Bridge non démarré';
      overall.style.color = 'var(--text)';
      summary.textContent = 'La configuration est prête, mais la prise externe ne remontera rien tant que le bridge Meross ne tourne pas.';
      actionHint.textContent = 'Sauvegarde la configuration puis lance le bridge. SoulKernel basculera automatiquement sur la prise dès que le JSON sera frais.';
      actionHint.style.color = 'var(--muted)';
    }
  }
  const info = document.getElementById('merossConfigStatus');
  if (info && status.running) {
    const sourceLabel = status.pythonSource === 'embedded' ? 'runtime embarqué' : 'python système';
    info.textContent = 'Bridge Meross actif' + (status.pid ? ' (PID ' + status.pid + ')' : '') + ' · ' + sourceLabel + '.';
    info.style.color = 'var(--io)';
  }
}

async function loadExternalPowerConfig() {
  const cfg = await invoke('get_external_power_config');
  renderExternalPowerConfig(cfg);
}

async function refreshExternalPowerStatus() {
  const status = await invoke('get_external_power_status');
  renderExternalPowerStatus(status);
}

async function refreshExternalBridgeStatus() {
  const status = await invoke('get_external_bridge_status');
  renderExternalBridgeStatus(status);
}

async function applyExternalPowerConfig() {
  const enabled = !!document.getElementById('merossEnabled')?.checked;
  const rawPowerFile = String(document.getElementById('merossPowerFile')?.value || '').trim();
  const rawMaxAge = parseInt(String(document.getElementById('merossMaxAgeMs')?.value || '15000'), 10);
  const email = String(document.getElementById('merossEmail')?.value || '').trim();
  const password = String(document.getElementById('merossPassword')?.value || '');
  const region = String(document.getElementById('merossRegion')?.value || 'eu').trim() || 'eu';
  const deviceType = String(document.getElementById('merossDeviceType')?.value || 'mss315').trim() || 'mss315';
  const httpProxy = String(document.getElementById('merossHttpProxy')?.value || '').trim();
  const mfaCode = String(document.getElementById('merossMfaCode')?.value || '').trim();
  const pythonBin = String(document.getElementById('merossPythonBin')?.value || '').trim();
  const intervalS = parseFloat(String(document.getElementById('merossBridgeInterval')?.value || '8'));
  const autostartBridge = !!document.getElementById('merossAutostartBridge')?.checked;
  const info = document.getElementById('merossConfigStatus');
  if (!Number.isFinite(rawMaxAge) || rawMaxAge < 1000 || !Number.isFinite(intervalS) || intervalS < 2) {
    if (info) {
      info.textContent = 'Configuration invalide: max_age_ms >= 1000 et intervalle >= 2 s.';
      info.style.color = 'var(--stress)';
    }
    return;
  }
  await invoke('set_external_power_config', {
    config: {
      enabled,
      power_file: rawPowerFile || null,
      max_age_ms: rawMaxAge,
      meross_email: email || null,
      meross_password: password || null,
      meross_region: region,
      meross_device_type: deviceType || null,
      meross_http_proxy: httpProxy || null,
      meross_mfa_code: mfaCode || null,
      python_bin: pythonBin || null,
      bridge_interval_s: intervalS,
      autostart_bridge: autostartBridge,
    }
  });
  await loadExternalPowerConfig();
  await refreshExternalPowerStatus();
  await refreshExternalBridgeStatus();
  if (info) {
    info.textContent = 'Configuration prise externe enregistrée.';
    info.style.color = 'var(--io)';
  }
}

async function startExternalBridge() {
  const status = await invoke('start_external_bridge');
  renderExternalBridgeStatus(status);
}

async function stopExternalBridge() {
  const status = await invoke('stop_external_bridge');
  renderExternalBridgeStatus(status);
}

async function setDomeStateAdaptive(shouldEnable, reason) {
  if (shouldEnable === state.domeActive) return;
  if (shouldEnable) {
    const pidVal = document.getElementById('targetProcess').value;
    state.targetPid = pidVal === '' ? null : parseInt(pidVal, 10);
    const result = await invoke('activate_dome', {
      workload: state.wl,
      kappa: state.kappa,
      sigmaMax: state.sigmaMax,
      eta: state.eta,
      targetPid: state.targetPid,
      policyMode: state.policyMode,
    });
    state.domeActive = !!result.activated;
    updateDomeBadge(state.domeActive);
    log('Adaptive: dome ' + (state.domeActive ? 'ON' : 'bloqué') + ' (' + reason + ')', state.domeActive ? 'ok' : 'warn');
    if (state.domeActive) {
      logDomeActivationGains(result, { context: 'adaptatif', workload: state.wl, targetPid: state.targetPid });
    }
  } else {
    const actions = await invoke('rollback_dome');
    state.domeActive = false;
    updateDomeBadge(false);
    actions.forEach(a => log(a, (a.toLowerCase().includes('failed') || a.toLowerCase().includes('denied') || a.startsWith('[ko]')) ? 'warn' : 'ok'));
    log('Adaptive: dome OFF (' + reason + ')', 'warn');
  }
  state.adaptiveLastDomeSwitchTs = Date.now();
}

function deriveAdaptiveTarget(m, currentWl) {
  const sigma = Number(m.sigma || 0);
  const cpu = Number(m.cpu || 0);
  const gpu = Number(m.gpu || 0);
  const io = Number(m.io_bandwidth || 0);
  const memPressure = clamp(1.0 - Number(m.mem || 0), 0, 1);
  const learned = state.kpiBench?.tuningAdvice || null;
  const effLearned = Number(learned?.expected_efficiency_score ?? 0);

  let wl = currentWl || 'es';
  if (gpu >= 0.52) wl = memPressure >= 0.45 ? 'llm_serving' : 'gamer';
  else if (io >= 0.58) wl = 'cloud_sync';
  else if (cpu >= 0.76) wl = 'ide_dev';
  else if (memPressure >= 0.82) wl = 'postgres_db';
  else if (gpu <= 0.30 && io <= 0.35 && cpu <= 0.62 && memPressure <= 0.65) wl = 'web_browsing';

  const baseKappa = learned ? Number(learned.recommended_kappa) : 2.0;
  const baseSigmaMax = learned ? Number(learned.recommended_sigma_max) : 0.75;
  const baseEta = learned ? Number(learned.recommended_eta) : 0.15;
  const efficiencyBias = clamp((effLearned - 1.5) / 12.0, -0.12, 0.12);

  const kappa = clamp(baseKappa + sigma * 1.6 + memPressure * 0.45 - efficiencyBias * 0.8, 0.8, 4.8);
  const sigmaMax = clamp(baseSigmaMax + 0.03 - sigma * 0.12 - memPressure * 0.06 + efficiencyBias * 0.15, 0.55, 0.9);
  const eta = clamp(baseEta + 0.08 - sigma * 0.10 + (cpu >= 0.7 ? 0.04 : 0) + (io >= 0.55 ? 0.02 : 0) + efficiencyBias * 0.35, 0.08, 0.4);
  const basePolicyMode = learned && (learned.recommended_policy_mode === 'safe' || learned.recommended_policy_mode === 'privileged')
    ? learned.recommended_policy_mode
    : 'privileged';
  const baseSoulRamPercent = learned
    ? clamp(Number(learned.recommended_soulram_percent || 20), 10, 60)
    : 20;

  const policyMode = (sigma > 0.82 || memPressure > 0.88 || effLearned < -1.0)
    ? 'safe'
    : (basePolicyMode === 'safe' && sigma > 0.68 ? 'safe' : 'privileged');
  const soulRamPercent = clamp(
    Math.round(baseSoulRamPercent + memPressure * 18 + (effLearned >= 3 ? 4 : 0) - (sigma > 0.8 ? 6 : 0)),
    10,
    55
  );

  return {
    wl,
    kappa,
    sigmaMax,
    eta,
    policyMode,
    soulRamPercent,
    memPressure,
    sigma,
    learnedEfficiency: effLearned,
  };
}

async function runAdaptiveController(m) {
  if (!state.adaptiveEnabled || state.kpiBench.running) return;
  const now = Date.now();
  if (now - state.adaptiveLastTickTs < 2000) return;
  state.adaptiveLastTickTs = now;

  const t = deriveAdaptiveTarget(m, state.wl);

  if (t.wl !== state.wl) {
    if (state.adaptiveWorkloadCandidate !== t.wl) {
      state.adaptiveWorkloadCandidate = t.wl;
      state.adaptiveWorkloadCandidateCount = 1;
    } else {
      state.adaptiveWorkloadCandidateCount += 1;
    }

    const canSwitchByCooldown = (now - state.workloadLastSwitchTs) >= ADAPTIVE_WORKLOAD_COOLDOWN_MS;
    const canSwitchByConfirm = state.adaptiveWorkloadCandidateCount >= ADAPTIVE_WORKLOAD_CONFIRM_CYCLES;
    if (canSwitchByCooldown && canSwitchByConfirm) {
      setWorkload(t.wl, 'adaptive');
    }
  } else {
    state.adaptiveWorkloadCandidate = null;
    state.adaptiveWorkloadCandidateCount = 0;
  }

  const kChanged = Math.abs(state.kappa - t.kappa) >= 0.1;
  const sChanged = Math.abs(state.sigmaMax - t.sigmaMax) >= 0.03;
  const eChanged = Math.abs(state.eta - t.eta) >= 0.02;
  if (kChanged || sChanged || eChanged) {
    state.kappa = t.kappa;
    state.sigmaMax = t.sigmaMax;
    state.eta = t.eta;
    setSlidersFromState();
    if (state.lastMetrics) renderFormula(state.lastMetrics);
  }

  if (now - state.adaptiveLastSystemApplyTs >= 15000) {
    if (state.policyMode !== t.policyMode) {
      try {
        state.policyMode = await invoke('set_policy_mode', { mode: t.policyMode });
        await loadPolicyStatus();
        log('Adaptive: policy -> ' + state.policyMode, 'info');
      } catch (e) {
        log('Adaptive policy set error: ' + e, 'warn');
      }
    }

    const srDelta = Math.abs(state.soulRamPercent - t.soulRamPercent);
    if ((t.memPressure >= 0.78 && !state.soulRamActive) || (state.soulRamActive && srDelta >= 10)) {
      try {
        state.soulRamPercent = t.soulRamPercent;
        const actions = await invoke('set_soulram', { enabled: true, percent: state.soulRamPercent });
        await syncSoulRamStatus();
        updateSoulRamUi();
        markSoulRamRebootRequirement(actions, 'Adaptive SoulRAM');
        if (state.soulRamActive) {
          log('Adaptive: SoulRAM -> ' + state.soulRamPercent + '%', 'ok');
        } else {
          log('Adaptive: SoulRAM sans effet effectif (Windows: souvent admin requis, ou trim en cooldown).', 'warn');
        }
      } catch (e) {
        log('Adaptive SoulRAM error: ' + e, 'warn');
      }
    }
    state.adaptiveLastSystemApplyTs = now;
  }

  const statusEl = document.getElementById('adaptiveStatus');
  updateAdaptiveStatusText();

  if (state.adaptiveAutoDome && now - state.adaptiveLastDomeSwitchTs >= 10000) {
    if (state.domeActive && m.sigma > state.sigmaMax + 0.08) {
      await setDomeStateAdaptive(false, 'sigma guard');
    } else if (!state.domeActive && m.sigma < state.sigmaMax - 0.12) {
      await setDomeStateAdaptive(true, 'headroom available');
    }
  }
}

const BENCH_LOG_MAX = 16;
let benchLogLines = [];

function resetBenchmarkProgressUi() {
  const wrap = document.getElementById('kpiBenchProgressWrap');
  if (wrap) {
    wrap.classList.remove('visible');
    wrap.hidden = true;
  }
  const bar = document.getElementById('kpiBenchProgressBar');
  if (bar) bar.style.width = '0%';
  benchLogLines = [];
  const logEl = document.getElementById('kpiBenchProgressLog');
  if (logEl) logEl.textContent = '';
  const title = document.getElementById('kpiBenchProgressTitle');
  if (title) title.textContent = 'Benchmark en attente';
  const sub = document.getElementById('kpiBenchProgressSub');
  if (sub) sub.textContent = '';
}

function showBenchmarkProgressUi() {
  const wrap = document.getElementById('kpiBenchProgressWrap');
  if (wrap) {
    wrap.hidden = false;
    wrap.classList.add('visible');
    try {
      wrap.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
    } catch (_) {}
  }
  benchLogLines = [];
  const logEl = document.getElementById('kpiBenchProgressLog');
  if (logEl) logEl.textContent = '';
  const bar = document.getElementById('kpiBenchProgressBar');
  if (bar) bar.style.width = '0%';
  const title = document.getElementById('kpiBenchProgressTitle');
  if (title) title.textContent = 'Benchmark A/B en cours…';
  const sub = document.getElementById('kpiBenchProgressSub');
  if (sub) sub.textContent = '';
}

function updateBenchmarkProgressUi(payload) {
  if (!payload) return;
  const p = payload;
  const wrap = document.getElementById('kpiBenchProgressWrap');
  if (wrap) {
    wrap.hidden = false;
    wrap.classList.add('visible');
  }
  const bar = document.getElementById('kpiBenchProgressBar');
  if (bar && typeof p.progress_percent === 'number' && Number.isFinite(p.progress_percent)) {
    bar.style.width = Math.min(100, Math.max(0, p.progress_percent)) + '%';
  }
  const sub = document.getElementById('kpiBenchProgressSub');
  if (sub) {
    const cur = p.current != null ? String(p.current) : '—';
    const tot = p.total != null ? String(p.total) : '—';
    const ph = p.phase != null ? String(p.phase) : '—';
    const st = p.step != null ? String(p.step) : '—';
    sub.textContent = `Échantillon ${cur}/${tot} · ${ph} · ${st}`;
  }
  const title = document.getElementById('kpiBenchProgressTitle');
  if (title && p.message != null) title.textContent = String(p.message);
  if (p.step === 'done' && (p.stdout_tail || p.stderr_tail)) {
    const extra = [];
    if (p.stdout_tail) extra.push('stdout: ' + String(p.stdout_tail).trim());
    if (p.stderr_tail) extra.push('stderr: ' + String(p.stderr_tail).trim());
    const line = extra.join('\n');
    if (line) {
      benchLogLines.push(line);
      if (benchLogLines.length > BENCH_LOG_MAX) benchLogLines.shift();
      const logEl = document.getElementById('kpiBenchProgressLog');
      if (logEl) logEl.textContent = benchLogLines.join('\n---\n');
    }
  }
  if (p.finished) {
    const t = document.getElementById('kpiBenchProgressTitle');
    if (t && p.ok === false) {
      t.textContent = t.textContent || 'Benchmark interrompu';
    } else if (t && p.ok === true) {
      t.textContent = String(p.message || 'Benchmark terminé');
    }
  }
}

let benchProgressHideTimer = null;
function scheduleBenchmarkProgressHide() {
  if (benchProgressHideTimer) clearTimeout(benchProgressHideTimer);
  benchProgressHideTimer = setTimeout(() => {
    benchProgressHideTimer = null;
    resetBenchmarkProgressUi();
  }, 2800);
}

function warnRiskyBenchmarkCommand(command, args) {
  const cmd = String(command || '').trim().toLowerCase();
  if (cmd === 'system') return false;
  const a0 = args && args.length ? String(args[0]).trim().toLowerCase() : '';
  if (cmd === 'cargo' && /^(check|build|run|test|clippy|fix|miri|expand)$/.test(a0)) {
    log(
      'Benchmark: cargo ' + a0 + ' sur ce dépôt peut faire recompiler le projet et, avec « cargo tauri dev », ' +
        'redémarrer l’app (watcher sur target/). Utilisez plutôt la sonde « system » ou une commande hors repo.',
      'warn'
    );
    return true;
  }
  if (cmd === 'cargo' && args.length === 0) {
    log('Benchmark: « cargo » sans sous-commande peut être lent ou interactif — préférez une commande explicite.', 'warn');
    return true;
  }
  return false;
}

async function runAbBenchmark() {
  if (state.kpiBench.running) return;
  const runsInput = parseInt(document.getElementById('kpiRuns')?.value || '5', 10);
  const runs = Math.max(5, Math.min(20, isNaN(runsInput) ? 5 : runsInput));
  let command = (document.getElementById('kpiCommand')?.value || '').trim();
  if (!command) {
    command = 'system';
    const kEl = document.getElementById('kpiCommand');
    if (kEl) kEl.value = 'system';
  }
  const args = tokenizeArgs(document.getElementById('kpiArgs')?.value || '');
  const benchWorkload = inferWorkloadFromBenchmarkCommand(command, args) || state.wl;
  warnRiskyBenchmarkCommand(command, args);
  if (benchWorkload !== state.wl) {
    setWorkload(benchWorkload, 'benchmark');
    log('Benchmark workload auto: ' + benchWorkload, 'info');
  }
  log('BENCH DIAG start ' + JSON.stringify(benchmarkDiagPayload({
    bench_command: command,
    bench_args: args,
    bench_workload: benchWorkload,
    runs_per_state: runs,
  })), 'info');

  const wasAdaptive = state.adaptiveEnabled;
  state.kpiBench.running = true;
  showBenchmarkProgressUi();
  const runBtn = document.getElementById('btnRunAB');
  if (runBtn) runBtn.disabled = true;
  if (wasAdaptive) {
    state.adaptiveEnabled = false;
    const cb = document.getElementById('adaptiveEnabled');
    if (cb) cb.checked = false;
    updateAdaptiveStatusText('paused during benchmark');
  }

  try {
    const session = await invoke('run_ab_benchmark', {
      request: {
        command,
        args,
        cwd: null,
        runs_per_state: runs,
        workload: benchWorkload,
        kappa: state.kappa,
        sigma_max: state.sigmaMax,
        eta: state.eta,
        target_pid: state.targetPid,
        policy_mode: state.policyMode,
        soulram_percent: state.soulRamPercent,
        settle_ms: 1200,
      },
    });
    (session.samples || []).forEach(sample => {
      const phase = String(sample.phase || '').toUpperCase();
      const extra = [];
      if (sample.sigma_before != null) extra.push('sigma=' + Number(sample.sigma_before).toFixed(3));
      if (sample.cpu_before_pct != null) extra.push('cpu=' + Number(sample.cpu_before_pct).toFixed(1) + '%');
      log(
        `A/B #${sample.idx} ${phase} -> ${sample.duration_ms} ms (exit=${sample.exit_code ?? 'null'})` +
        (extra.length ? ' | ' + extra.join(' | ') : ''),
        sample.success ? 'ok' : 'warn'
      );
    });
    state.kpiBench.sessions.unshift(session);
    state.kpiBench.lastSummary = session.summary;
    if (state.kpiBench.sessions.length > 20) state.kpiBench.sessions.pop();
    renderAbSummary(session.summary);
    log('BENCH DIAG end ' + JSON.stringify(benchmarkDiagPayload({
      bench_command: command,
      bench_args: args,
      bench_workload: benchWorkload,
      summary: session.summary,
      session_samples: Array.isArray(session.samples) ? session.samples.length : 0,
    })), 'info');
    logBenchGainsHuman(session.summary);
    await loadBenchmarkHistory(true);
  } catch (e) {
    log('A/B error: ' + e, 'err');
    const t = document.getElementById('kpiBenchProgressTitle');
    if (t) t.textContent = 'Erreur: ' + e;
    scheduleBenchmarkProgressHide();
  } finally {
    state.kpiBench.running = false;
    if (runBtn) runBtn.disabled = false;
    try {
      if (hasTauri) {
        const s = await invoke('get_soulram_status');
        state.soulRamActive = !!s?.active;
      }
    } catch (_) {}
    if (wasAdaptive) {
      state.adaptiveEnabled = true;
      const cb = document.getElementById('adaptiveEnabled');
      if (cb) cb.checked = true;
      updateAdaptiveStatusText();
    } else {
      updateAdaptiveStatusText();
    }
    saveRuntimeSettings();
    saveStartupIntent();
    scheduleBenchmarkProgressHide();
  }
}

async function loadBenchmarkHistory(applyAdvice = true) {
  try {
    const command = (document.getElementById('kpiCommand')?.value || '').trim();
    const args = tokenizeArgs(document.getElementById('kpiArgs')?.value || '');
    const history = await invoke('get_benchmark_history', {
      query: {
        command: command || null,
        args,
        cwd: null,
        workload: state.wl,
      },
    });
    let sessions = Array.isArray(history?.sessions) ? history.sessions : [];
    if (sessions.length > MAX_KPI_SESSIONS_IN_MEMORY) {
      sessions = sessions.slice(0, MAX_KPI_SESSIONS_IN_MEMORY);
    }
    state.kpiBench.sessions = sessions.map(trimSessionSamplesForUi);
    state.kpiBench.lastSummary = history?.last_summary || (state.kpiBench.sessions[0]?.summary ?? null);
    state.kpiBench.tuningAdvice = history?.advice || null;
    let topSessions = Array.isArray(history?.top_sessions) ? history.top_sessions : [];
    if (topSessions.length > MAX_BENCH_TOP_UI) {
      topSessions = topSessions.slice(0, MAX_BENCH_TOP_UI);
    }
    state.kpiBench.topSessions = topSessions;
    renderAbSummary(state.kpiBench.lastSummary);
    renderBenchmarkLearning(state.kpiBench.tuningAdvice);
    renderBenchmarkTop(state.kpiBench.topSessions);
    if (applyAdvice && state.kpiBench.tuningAdvice) {
      applyBenchmarkLearning(state.kpiBench.tuningAdvice);
    }
  } catch (e) {
    renderBenchmarkLearning(null);
    renderBenchmarkTop([]);
    log('Benchmark history: ' + e, 'warn');
  }
}
function pickAutoProcess(list, selectedValue) {
  if (!Array.isArray(list) || list.length === 0) return '';
  const excluded = /^(soulkernel(\.exe)?|system|system idle process|idle|msedgewebview2(\.exe)?)$/i;
  const candidates = list.filter(p => {
    const n = String(p.name || '').trim().toLowerCase();
    if (excluded.test(n)) return false;
    if (n.includes('soulkernel') || n.includes('msedgewebview2')) return false;
    // Tauri / WebKit : ne pas cibler le moteur de rendu de l'app elle-même
    if (n.includes('webkit') || n.includes('wpe-web') || n.includes('wpeweb')) return false;
    return true;
  });
  const pool = candidates.length ? candidates : list;
  if (selectedValue && pool.some(p => String(p.pid) === String(selectedValue))) return String(selectedValue);
  const firstHot = pool.find(p => (p.cpu_usage || 0) >= 1.0);
  const chosen = firstHot || pool[0];
  return chosen ? String(chosen.pid) : '';
}

function capProcessListForSelect(list, preserveValue) {
  if (!Array.isArray(list) || list.length <= MAX_PROCESS_SELECT_OPTIONS) return list;
  const sorted = [...list].sort(
    (a, b) => (Number(b.cpu_usage) || 0) - (Number(a.cpu_usage) || 0)
  );
  const out = sorted.slice(0, MAX_PROCESS_SELECT_OPTIONS);
  const pv = preserveValue != null && preserveValue !== '' ? String(preserveValue) : '';
  if (pv && !out.some(p => String(p.pid) === pv)) {
    const keep = list.find(p => String(p.pid) === pv);
    if (keep) {
      out.pop();
      out.push(keep);
    }
  }
  return out;
}

function trimSessionSamplesForUi(s) {
  if (!s || !Array.isArray(s.samples) || s.samples.length <= MAX_SAMPLES_PER_SESSION_UI) return s;
  return Object.assign({}, s, { samples: s.samples.slice(-MAX_SAMPLES_PER_SESSION_UI) });
}

function inferWorkloadFromBenchmarkCommand(command, args) {
  if (String(command || '').trim().toLowerCase() === 'system') return 'idle_desktop';
  const joined = [command, ...(args || [])].join(' ').toLowerCase();
  if (/(^|\s)(cargo|rustc|cl|clang|gcc|msbuild|ninja|cmake|link\.exe|javac|gradle|mvn|go|dotnet|tsc|webpack)(\s|$)/.test(joined)) {
    return 'compile';
  }
  if (/(elastic|kibana|logstash)/.test(joined)) return 'es';
  if (/(sqlite|dbbrowser|litecli)/.test(joined)) return 'sqlite';
  if (/(postgres|psql)/.test(joined)) return 'postgres_db';
  if (/(mysql|mariadb)/.test(joined)) return 'mysql_db';
  if (/(mongo|mongod)/.test(joined)) return 'mongodb_db';
  if (/(redis|redis-server)/.test(joined)) return 'redis_cache';
  if (/(kafka)/.test(joined)) return 'kafka_stream';
  if (/(spark)/.test(joined)) return 'spark_etl';
  if (/(docker|podman|nerdctl)/.test(joined)) return 'docker_dev';
  if (/(kubectl|k3s|kind)/.test(joined)) return 'kubernetes_edge';
  if (/(oracle|sqlplus|sqlservr)/.test(joined)) return 'oracle';
  if (/(backup|veeam|acronis|robocopy|rsync|sync)/.test(joined)) return 'backup';
  if (/(ollama|llama|llm|vllm)/.test(joined)) return 'llm_serving';
  if (/(python|pytorch|tensorflow|cuda)/.test(joined)) return 'ai';
  return null;
}

function inferWorkloadFromProcess(proc, metrics) {
  const name = String(proc?.name || '').toLowerCase();
  if (!name) return null;

  if (/(bf|battlefield|cod|mw2|mw3|warzone|cs2|valorant|fortnite|apex|overwatch|pubg|rocketleague|dota|leagueoflegends|r5apex|eldenring|cyberpunk|witcher)/.test(name)) {
    return 'gamer';
  }
  if (/(cl|clang|gcc|rustc|cargo|msbuild|ninja|cmake|link\.exe|javac|gradle|mvn|go\s|go\.exe|dotnet|node|tsc|webpack)/.test(name)) {
    return 'compile';
  }
  if (/(python|ollama|llama|stable\s?diffusion|pytorch|tensorflow|cuda)/.test(name)) {
    return 'ai';
  }
  if (/(oracle|tns|sqlplus|sqlservr|postgres|mysql|mariadb)/.test(name)) {
    return 'oracle';
  }
  if (/(sqlite|dbbrowser|litecli)/.test(name)) {
    return 'sqlite';
  }
  if (/(backup|veeam|acronis|robocopy|rsync|sync|onedrive|dropbox)/.test(name)) {
    return 'backup';
  }
  if (/(elastic|kibana|logstash)/.test(name)) {
    return 'es';
  }
  if (/(docker|podman|com\.docker)/.test(name)) return 'docker_dev';
  if (/(postgres|postmaster)/.test(name)) return 'postgres_db';
  if (/(redis-server|redis\.exe)/.test(name)) return 'redis_cache';
  if (/(obs|obs64)/.test(name)) return 'twitch_stream';

  const cpu = Number(metrics?.raw?.cpu_pct || 0);
  const gpuNorm = Number(metrics?.gpu ?? 0);
  if (cpu >= 70 && gpuNorm >= 0.30) return 'gamer';
  if (cpu >= 80) return 'compile';
  return null;
}
function setProcessRefreshInfo() {
  const info = document.getElementById('processRefreshInfo');
  if (!info) return;
  if (!state.lastProcessRefreshTs) {
    info.textContent = 'maj auto: --';
    return;
  }
  info.textContent = `maj auto: ${state.lastProcessRefreshTs}`;
}

function renderProcessImpactPanel() {
  const tbody = document.getElementById('processImpactRows');
  const summary = document.getElementById('processImpactSummary');
  const overhead = document.getElementById('overheadAuditSummary');
  if (!tbody || !summary) return;
  const report = state.processImpactReport && typeof state.processImpactReport === 'object'
    ? state.processImpactReport
    : { processes: [], top_processes: [], top_process_rows: [], grouped_processes: [], overhead_audit: null, summary: null };
  const rows = Array.isArray(report.top_process_rows) ? report.top_process_rows : [];
  const meta = report.summary && typeof report.summary === 'object' ? report.summary : null;
  const audit = report.overhead_audit && typeof report.overhead_audit === 'object' ? report.overhead_audit : null;
  if (!rows.length) {
    summary.textContent = 'Aucune donnée processus collectée.';
    if (overhead) overhead.textContent = 'Audit overhead SoulKernel/WebView indisponible.';
    tbody.innerHTML = '<tr><td colspan="11" class="process-impact-empty">Aucune donnée processus.</td></tr>';
    return;
  }
  const selectedPid = state.targetPid;
  if (meta) {
    summary.textContent = `${meta.process_count} processus | top ${meta.top_count} affichés | CPU observé ${meta.observed_cpu_count}/${meta.top_count} | GPU observé ${meta.observed_gpu_count}/${meta.top_count} | RAM observée ${meta.observed_memory_count}/${meta.top_count}`;
  } else {
    summary.textContent = `${rows.length} processus impact affichés`;
  }
  if (overhead) {
    if (audit) {
      const fmtW = (v) => v == null || !Number.isFinite(Number(v)) ? '—' : `${Number(v).toFixed(2)} W est.`;
      const fmtMem = (kb) => Number.isFinite(Number(kb)) && Number(kb) > 0 ? `${(Number(kb) / 1024).toFixed(0)} MiB` : '—';
      overhead.textContent =
        `Overhead cumulé SoulKernel + WebView: ${Number(audit.combined_cpu_usage_pct || 0).toFixed(1)} % CPU · ${Number(audit.combined_gpu_usage_pct || 0).toFixed(1)} % GPU · ${fmtMem(audit.combined_memory_kb)} · ${fmtW(audit.combined_estimated_power_w)} | ` +
        `SoulKernel: ${Number(audit.soulkernel_cpu_usage_pct || 0).toFixed(1)} % CPU / ${Number(audit.soulkernel_gpu_usage_pct || 0).toFixed(1)} % GPU · ${fmtMem(audit.soulkernel_memory_kb)} | ` +
        `WebView: ${Number(audit.webview_cpu_usage_pct || 0).toFixed(1)} % CPU / ${Number(audit.webview_gpu_usage_pct || 0).toFixed(1)} % GPU · ${fmtMem(audit.webview_memory_kb)} (${Number(audit.webview_process_count || 0)} proc)`;
    } else {
      overhead.textContent = 'Audit overhead SoulKernel/WebView en attente...';
    }
  }
  tbody.innerHTML = rows.map(p => {
    const pills = [];
    if (selectedPid != null && Number(p.pid) === Number(selectedPid)) pills.push('<span class="process-pill process-pill--target">TARGET</span>');
    if (p.is_self_process) pills.push('<span class="process-pill process-pill--self">SELF</span>');
    if (p.is_embedded_webview) pills.push('<span class="process-pill process-pill--wv">WV</span>');
    const exe = p.exe ? escapeHtml(String(p.exe)) : '—';
    const cmd = p.cmd_preview ? escapeHtml(String(p.cmd_preview)) : '—';
    return `<tr>
      <td>
        <div class="process-impact-name">
          <div class="process-impact-main">${escapeHtml(p.name || '—')}</div>
          <div>${pills.join('')}</div>
          <div class="process-impact-meta" title="${cmd}">exe: ${exe}</div>
        </div>
      </td>
      <td>${escapeHtml(p.pid)}</td>
      <td>${escapeHtml(p.cpu_label)}</td>
      <td>${escapeHtml(p.gpu_label)}</td>
      <td>${escapeHtml(p.ram_label)}<br><span class="process-impact-meta">${escapeHtml(p.ram_share_label)}</span></td>
      <td>${escapeHtml(p.io_label)}<br><span class="process-impact-meta">${escapeHtml(p.io_split_label)}</span></td>
      <td>${escapeHtml(p.power_label)}</td>
      <td>${escapeHtml(p.impact_label)}</td>
      <td>${escapeHtml(p.duration_label)}</td>
      <td>${escapeHtml(p.status_label)}</td>
      <td><span class="process-impact-meta">${escapeHtml(p.attribution_method || '—')}</span></td>
    </tr>`;
  }).join('');
}

function startClockLoop() {
  if (clockIntervalId != null) return;
  clockIntervalId = setInterval(() => {
    const clk = document.getElementById('clk');
    if (clk) clk.textContent = new Date().toTimeString().slice(0, 8);
  }, 1000);
}

function stopClockLoop() {
  if (clockIntervalId != null) {
    clearInterval(clockIntervalId);
    clockIntervalId = null;
  }
}

function startProcessRefreshLoop() {
  if (processRefreshIntervalId != null || document.hidden) return;
  processRefreshIntervalId = setTimeout(() => {
    processRefreshIntervalId = null;
    refreshProcesses({ userInitiated: false });
  }, nextProcessRefreshDelayMs());
}

function stopProcessRefreshLoop() {
  if (processRefreshIntervalId != null) {
    clearTimeout(processRefreshIntervalId);
    processRefreshIntervalId = null;
  }
}

async function refreshProcesses(options = {}) {
  const userInitiated = options.userInitiated === true;
  if (!userInitiated && shouldSleepWebview()) {
    startProcessRefreshLoop();
    return;
  }
  if (processRefreshInFlight) return;
  processRefreshInFlight = true;
  try {
    const rawReport = await invoke('list_processes');
    const rawList = Array.isArray(rawReport)
      ? rawReport
      : Array.isArray(rawReport?.processes)
        ? rawReport.processes
        : [];
    const topProcesses = Array.isArray(rawReport?.top_processes) ? rawReport.top_processes : rawList.slice(0, 12);
    const topProcessRows = Array.isArray(rawReport?.top_process_rows) ? rawReport.top_process_rows : [];
    const groupedProcesses = Array.isArray(rawReport?.grouped_processes) ? rawReport.grouped_processes : [];
    const overheadAudit = rawReport?.overhead_audit && typeof rawReport.overhead_audit === 'object' ? rawReport.overhead_audit : null;
    const summary = rawReport?.summary && typeof rawReport.summary === 'object' ? rawReport.summary : null;
    const reportRevision = summary?.report_revision || null;
    const uiRevision = summary?.ui_revision || reportRevision || null;
    const reportChanged = userInitiated || reportRevision == null || reportRevision !== lastProcessReportRevision;
    const uiChanged = userInitiated || uiRevision == null || uiRevision !== lastProcessUiRevision;
    if (reportChanged) {
      state.processList = rawList;
      state.processImpactReport = { processes: rawList, top_processes: topProcesses, top_process_rows: topProcessRows, grouped_processes: groupedProcesses, overhead_audit: overheadAudit, summary };
      lastProcessReportRevision = reportRevision;
    } else if (state.processImpactReport && typeof state.processImpactReport === 'object') {
      state.processImpactReport.summary = summary;
      state.processImpactReport.overhead_audit = overheadAudit;
    }
    const sel = document.getElementById('targetProcess');
    const current = sel.value;
    const list = reportChanged ? capProcessListForSelect(rawList, current) : capProcessListForSelect(state.processList, current);
    if (reportChanged) {
      sel.innerHTML = '<option value="">Ce processus (SoulKernel)</option>';
      list.forEach(p => {
        const opt = document.createElement('option');
        opt.value = p.pid;
        const rss = (p.memory_kb != null) ? ` · ${(Number(p.memory_kb) / 1024).toFixed(0)} MiB` : '';
        const par = (p.parent_pid != null) ? ` ←${p.parent_pid}` : '';
        const impact = p.impact_score_pct_estimated != null ? ` · impact ${Number(p.impact_score_pct_estimated).toFixed(1)}%` : '';
        const pw = p.estimated_power_w != null ? ` · ~${Number(p.estimated_power_w).toFixed(1)} W est.` : '';
        opt.textContent = `${p.name} (PID ${p.pid})${par} — ${p.cpu_usage.toFixed(1)}% CPU${rss}${impact}${pw}`;
        sel.appendChild(opt);
      });
    }
    if (state.autoProcessTarget) {
      const pick = pickAutoProcess(list, current);
      if (pick) sel.value = pick;
      const chosen = list.find(p => String(p.pid) === String(sel.value));
      const prevTargetPid = state.targetPid;
      state.targetPid = sel.value === '' ? null : parseInt(sel.value, 10);
      if (prevTargetPid !== state.targetPid) {
        logStateDiagTarget({
          source: 'auto-process-refresh',
          previous_target_pid: prevTargetPid,
          next_target_pid: state.targetPid,
          next_target_label: chosen ? `${chosen.name} (PID ${chosen.pid}) - ${Number(chosen.cpu_usage || 0).toFixed(1)}% CPU` : 'Ce processus (SoulKernel)',
          auto_process_target: !!state.autoProcessTarget,
          workload: state.wl,
        });
      }
      if (!state.adaptiveEnabled) {
        const inferred = inferWorkloadFromProcess(chosen, state.lastMetrics);
        if (inferred) setWorkload(inferred, 'process:' + (chosen?.name || 'n/a'));
      }
    } else if (current) {
      sel.value = current;
      state.targetPid = current === '' ? null : parseInt(current, 10);
    }
    const now = new Date();
    state.lastProcessRefreshTs = now.toTimeString().slice(0, 8);
    setProcessRefreshInfo();

    if (userInitiated || list.length !== state.lastProcessCount) {
      log(`Liste processus : ${list.length} affichées (${rawList.length} au total)`, 'ok');
    }
    state.lastProcessCount = rawList.length;
    if (uiChanged && !document.hidden) {
      renderProcessImpactPanel();
      lastProcessUiRevision = uiRevision;
    }
  } catch (e) {
    if (userInitiated) log(`list_processes: ${e}`, 'err');
  } finally {
    processRefreshInFlight = false;
    if (!document.hidden) startProcessRefreshLoop();
  }
}
function updateSoulRamUi() {
  const st = document.getElementById('soulRamStatus');
  const pct = document.getElementById('soulRamPctNum');
  if (st) {
    st.textContent = state.soulRamActive ? 'ON' : 'OFF';
    st.style.color = state.soulRamActive ? 'var(--io)' : 'var(--stress)';
  }
  if (pct) pct.textContent = `${state.soulRamPercent}%`;
  const onBtn = document.getElementById('btnSoulRamOn');
  const offBtn = document.getElementById('btnSoulRamOff');
  if (onBtn) onBtn.disabled = !!state.soulRamActive;
  if (offBtn) offBtn.disabled = !state.soulRamActive;
}

async function syncSoulRamStatus() {
  try {
    const s = await invoke('get_soulram_status');
    state.soulRamActive = !!s.active;
    state.soulRamPercent = Number(s.percent || 20);
    state.soulRamBackend = (s.backend && String(s.backend)) || '';
    const slider = document.getElementById('soulRamPct');
    if (slider) slider.value = String(state.soulRamPercent);
    updateSoulRamUi();
    if (s.backend) log(`SoulRAM backend: ${s.backend}`, 'info');
  } catch (_) {}
}

function saveRuntimeSettings() {
  try {
    localStorage.setItem('soulkernel.runtime_settings', JSON.stringify({
      policyMode: state.policyMode,
      autoReapplyIntent: state.autoReapplyIntent,
      adaptiveEnabled: state.adaptiveEnabled,
      adaptiveAutoDome: state.adaptiveAutoDome,
      kpiCommand: (document.getElementById('kpiCommand')?.value || 'system'),
      kpiArgs: (document.getElementById('kpiArgs')?.value || '4000'),
      kpiRuns: parseInt(document.getElementById('kpiRuns')?.value || '5', 10) || 5,
      viewMode: state.viewMode,
      hudVisible: !!state.hudVisible,
      hudInteractive: !!state.hudInteractive,
      hudPreset: state.hudPreset,
      hudDisplayIndex: state.hudDisplayIndex,
      hudOpacity: Number(state.hudOpacity || 0.82),
      hudSizeMode: state.hudSizeMode || 'screen',
      hudScreenWidthPct: Number(state.hudScreenWidthPct) || 22,
      hudScreenHeightPct: Number(state.hudScreenHeightPct) || 28,
      hudManualWidth: Number(state.hudManualWidth) || 420,
      hudManualHeight: Number(state.hudManualHeight) || 260,
      hudVisibleMetrics: Array.isArray(state.hudVisibleMetrics) && state.hudVisibleMetrics.length
        ? state.hudVisibleMetrics.slice()
        : HUD_METRIC_DEFAULTS.slice(),
    }));
  } catch (_) {}
}

function loadRuntimeSettings() {
  try {
    const raw = localStorage.getItem('soulkernel.runtime_settings');
    if (!raw) return;
    const cfg = JSON.parse(raw);
    if (cfg.policyMode === 'safe' || cfg.policyMode === 'privileged') {
      state.policyMode = cfg.policyMode;
    }
    if (typeof cfg.autoReapplyIntent === 'boolean') {
      state.autoReapplyIntent = cfg.autoReapplyIntent;
    }
    if (typeof cfg.adaptiveEnabled === 'boolean') state.adaptiveEnabled = cfg.adaptiveEnabled;
    if (typeof cfg.adaptiveAutoDome === 'boolean') state.adaptiveAutoDome = cfg.adaptiveAutoDome;
    if (typeof cfg.kpiCommand === 'string') { const el = document.getElementById('kpiCommand'); if (el) el.value = cfg.kpiCommand; }
    if (typeof cfg.kpiArgs === 'string') { const el = document.getElementById('kpiArgs'); if (el) el.value = cfg.kpiArgs; }
    if (typeof cfg.kpiRuns === 'number') { const el = document.getElementById('kpiRuns'); if (el) el.value = String(Math.max(5, Math.min(20, Math.floor(cfg.kpiRuns)))); }
    if (cfg.viewMode === 'compact' || cfg.viewMode === 'detailed' || cfg.viewMode === 'benchmark' || cfg.viewMode === 'external') state.viewMode = cfg.viewMode;
    if (typeof cfg.hudVisible === 'boolean') state.hudVisible = cfg.hudVisible;
    if (typeof cfg.hudInteractive === 'boolean') state.hudInteractive = cfg.hudInteractive;
    if (cfg.hudPreset === 'mini' || cfg.hudPreset === 'compact' || cfg.hudPreset === 'detailed') state.hudPreset = cfg.hudPreset;
    if (typeof cfg.hudDisplayIndex === 'number' || cfg.hudDisplayIndex === null) state.hudDisplayIndex = cfg.hudDisplayIndex;
    if (typeof cfg.hudOpacity === 'number') state.hudOpacity = Math.max(0.3, Math.min(1.0, cfg.hudOpacity));
    if (cfg.hudSizeMode === 'screen' || cfg.hudSizeMode === 'content' || cfg.hudSizeMode === 'manual') {
      state.hudSizeMode = cfg.hudSizeMode;
    }
    if (typeof cfg.hudScreenWidthPct === 'number') state.hudScreenWidthPct = Math.max(8, Math.min(50, cfg.hudScreenWidthPct));
    if (typeof cfg.hudScreenHeightPct === 'number') state.hudScreenHeightPct = Math.max(8, Math.min(50, cfg.hudScreenHeightPct));
    if (typeof cfg.hudManualWidth === 'number') state.hudManualWidth = Math.max(240, Math.min(1600, cfg.hudManualWidth));
    if (typeof cfg.hudManualHeight === 'number') state.hudManualHeight = Math.max(120, Math.min(1200, cfg.hudManualHeight));
    if (Array.isArray(cfg.hudVisibleMetrics) && cfg.hudVisibleMetrics.length) state.hudVisibleMetrics = cfg.hudVisibleMetrics.filter(Boolean);
  } catch (_) {}
}

function saveStartupIntent() {
  try {
    localStorage.setItem('soulkernel.startup_intent', JSON.stringify({
      wl: state.wl,
      kappa: state.kappa,
      sigmaMax: state.sigmaMax,
      eta: state.eta,
      policyMode: state.policyMode,
      targetPid: state.targetPid,
      domeActive: state.domeActive,
      soulRamActive: state.soulRamActive,
      soulRamPercent: state.soulRamPercent,
      adaptiveEnabled: state.adaptiveEnabled,
      adaptiveAutoDome: state.adaptiveAutoDome,
      updatedAt: new Date().toISOString(),
    }));
  } catch (_) {}
}

function loadStartupIntent() {
  try {
    const raw = localStorage.getItem('soulkernel.startup_intent');
    if (!raw) return null;
    return JSON.parse(raw);
  } catch (_) { return null; }
}

async function loadPolicyStatus() {
  const sel = document.getElementById('policyMode');
  const badge = document.getElementById('policyBadge');
  const hint = document.getElementById('policyStatusHint');
  if (!sel || !badge || !hint) return;
  sel.value = state.policyMode;
  try {
    const status = await invoke('get_policy_status');
    if (status?.mode) {
      state.policyMode = status.mode;
      sel.value = state.policyMode;
    }
    badge.textContent = state.policyMode.toUpperCase();
    badge.style.color = state.policyMode === 'safe' ? 'var(--gpu)' : 'var(--io)';
    hint.textContent = 'admin: ' + (status?.is_admin ? 'oui' : 'non') + ' | reboot pending: ' + (status?.reboot_pending ? 'oui' : 'non');
    state.rebootPending = !!status?.reboot_pending;
    state.memoryCompressionEnabled = (status?.memory_compression_enabled ?? null);
    state.soulRamNeedsReboot = !!(state.rebootPending && state.memoryCompressionEnabled !== true);
    const pol = document.getElementById('soulRamPolicyLine');
    if (pol) {
      const mc = status?.memory_compression_enabled;
      pol.textContent =
        'Backend SoulRAM : ' + (state.soulRamBackend || '—') +
        ' · politique : ' + (state.policyMode || '—').toUpperCase() +
        ' · admin : ' + (status?.is_admin ? 'oui' : 'non') +
        ' · compression mémoire (OS) : ' +
        (mc === true ? 'active' : mc === false ? 'inactive' : 'inconnu') +
        (state.rebootPending ? ' · redémarrage signalé par l’OS' : '');
      pol.style.color = 'var(--muted)';
    }
    if (state.soulRamNeedsReboot) {
      log('Redemarrage Windows requis pour finaliser SoulRAM (Memory Compression).', 'warn');
    }
  } catch (e) {
    hint.textContent = 'admin/reboot status indisponible';
    log('Policy status: ' + e, 'warn');
  }
}

function markSoulRamRebootRequirement(actions, contextLabel) {
  const joined = Array.isArray(actions) ? actions.join(' | ').toLowerCase() : '';
  const explicitRestart = joined.includes('restart may be required') || joined.includes('restart pending') || joined.includes('reboot pending');
  const policyRestart = state.rebootPending && state.memoryCompressionEnabled !== true;
  const nextNeeds = !!(explicitRestart || policyRestart);
  const changedToTrue = !state.soulRamNeedsReboot && nextNeeds;
  state.soulRamNeedsReboot = nextNeeds;
  if (changedToTrue) {
    log((contextLabel || 'SoulRAM') + ': redemarrage requis pour stabiliser la compression memoire.', 'warn');
  }
}
async function applyStartupIntentIfAny() {
  if (!state.autoReapplyIntent) return;
  const intent = loadStartupIntent();
  if (!intent) return;
  if (intent.policyMode === 'safe' || intent.policyMode === 'privileged') {
    state.policyMode = intent.policyMode;
    const sel = document.getElementById('policyMode');
    if (sel) sel.value = state.policyMode;
    try { await invoke('set_policy_mode', { mode: state.policyMode }); } catch (_) {}
  }
  if (typeof intent.adaptiveEnabled === 'boolean') state.adaptiveEnabled = intent.adaptiveEnabled;
  if (typeof intent.adaptiveAutoDome === 'boolean') state.adaptiveAutoDome = intent.adaptiveAutoDome;
  if (typeof intent.kappa === 'number') state.kappa = intent.kappa;
  if (typeof intent.sigmaMax === 'number') state.sigmaMax = intent.sigmaMax;
  if (typeof intent.eta === 'number') state.eta = intent.eta;
  if (typeof intent.wl === 'string' && WORKLOADS[intent.wl]) {
    state.wl = intent.wl;
    syncWorkloadUiHighlight();
  }
  setSlidersFromState();
  if (intent.soulRamActive) {
    state.soulRamPercent = Number(intent.soulRamPercent || state.soulRamPercent || 20);
    try {
      const actions = await invoke('set_soulram', { enabled: true, percent: state.soulRamPercent });
      await syncSoulRamStatus();
      updateSoulRamUi();
      actions.forEach(a => log(a, a.startsWith('[ok]') ? 'ok' : 'warn'));
      await loadPolicyStatus();
      markSoulRamRebootRequirement(actions, 'Startup SoulRAM');
      if (!state.soulRamActive) {
        log('Startup SoulRAM: aucun effet effectif (Windows: lancer en administrateur pour la compression memoire).', 'warn');
      }
    } catch (e) {
      log('Startup SoulRAM: ' + e, 'warn');
    }
  }
  if (intent.domeActive) {
    try {
      const result = await invoke('activate_dome', {
        workload: state.wl,
        kappa: state.kappa,
        sigmaMax: state.sigmaMax,
        eta: state.eta,
        targetPid: state.targetPid,
        policyMode: state.policyMode,
      });
      state.domeActive = !!result.activated;
      updateDomeBadge(state.domeActive);
      result.actions.forEach(a => log(a, a.startsWith('\u2713') ? 'ok' : 'warn'));
      if (state.domeActive) {
        logDomeActivationGains(result, { context: 'démarrage (intention)', workload: state.wl, targetPid: state.targetPid });
      } else {
        log('Démarrage : réapplication dôme — ' + result.message, 'warn');
      }
    } catch (e) {
      log('Startup reapply dome error: ' + e, 'warn');
    }
  }
  if (state.lastMetrics) renderFormula(state.lastMetrics);
  log('Intention session reappliquee au lancement', 'info');
}


function updateSystemStatus(m = null) {
  const el = document.getElementById('sysStatus');
  if (!el) return;
  if (!hasTauri) {
    el.textContent = 'NO BACKEND';
    el.className = 'status-pill pill-warn';
    el.title = 'Backend Tauri indisponible';
    return;
  }
  if (!m || !m.raw) {
    el.textContent = 'CONNECTED';
    el.className = 'status-pill pill-ok';
    el.title = 'Connexion backend OK';
    return;
  }

  const isWindows = String(m.raw.platform || '').toLowerCase().includes('windows');
  const hasCore = (m.cpu != null) && (m.mem != null);
  const hasIoOrGpu = (m.io_bandwidth != null) || (m.gpu != null);

  if (!hasCore || !hasIoOrGpu) {
    el.textContent = 'PARTIAL DATA';
    el.className = 'status-pill pill-warn';
    el.title = 'Certaines metriques live sont absentes';
    return;
  }

  el.textContent = 'SYSTEM OK';
  el.className = 'status-pill pill-ok';
  el.title = isWindows
    ? 'Windows: PSI/compression peuvent etre indisponibles via API'
    : 'Metriques principales disponibles';
}
function renderMetrics(m) {
  const isWindows = String(m?.raw?.platform || '').toLowerCase().includes('windows');
  const na = isWindows ? 'N/A (Windows API)' : 'N/A';
  state.naLabel = na;
  const opt = (v, fmt) => (v != null ? fmt(v) : na);

  // r(t) bars — val/label optionnels : pas de simulation, on affiche N/A si pas de donnée
  setBar('CPU', m.cpu, m.raw.cpu_pct.toFixed(1)+'%');
  setBar('MEM', m.mem, (m.raw.mem_used_mb/1024).toFixed(1)+' GB used');
  setBar('LAM', m.compression, m.compression != null ? 'Λ(t)' : na);
  setBar('IO',  m.io_bandwidth, (m.raw.io_read_mb_s != null && m.raw.io_write_mb_s != null)
    ? (m.raw.io_read_mb_s.toFixed(0)+' + '+m.raw.io_write_mb_s.toFixed(0)+' MB/s') : na);
  setBar('GPU', m.gpu, opt(m.raw.gpu_pct, v => v.toFixed(1)+'%'));

  // labels
  document.getElementById('lCPU').textContent = `α_C=${WORKLOADS[state.wl][0]} · cpu=${m.raw.cpu_pct.toFixed(1)}%`;
  document.getElementById('lMEM').textContent = `${(m.raw.mem_used_mb/1024).toFixed(1)}/${(m.raw.mem_total_mb/1024).toFixed(1)} GB`;
  document.getElementById('lIO').textContent  = (m.raw.io_read_mb_s != null && m.raw.io_write_mb_s != null)
    ? `R:${m.raw.io_read_mb_s.toFixed(0)} W:${m.raw.io_write_mb_s.toFixed(0)} MB/s` : na;

  // sigma
  const sigmaFill = document.getElementById('sigmaFill');
  if (sigmaFill) sigmaFill.style.transform = `scaleY(${Number(m.sigma || 0).toFixed(4)})`;
  set('sigmaVal', m.sigma.toFixed(3));

  // Dynamic sigma sub-label based on platform
  const sigmaLbl = document.getElementById('sigmaSubLabel');
  if (sigmaLbl) {
    const p = String(m.raw.platform || '').toLowerCase();
    if (p.includes('windows')) sigmaLbl.textContent = 'CPU + MEM + I/O PRESSURE';
    else if (p.includes('macos')) sigmaLbl.textContent = 'CPU + MEM PRESSURE';
    else if (p.includes('linux')) sigmaLbl.textContent = 'PSI + MEM + SWAP PRESSURE';
  }

  // Machine activity badge in header
  const actBadge = document.getElementById('activityBadge');
  if (actBadge) {
    const a = state.machineActivity || 'active';
    if (a === 'idle') {
      actBadge.textContent = 'IDLE';
      actBadge.className = 'status-pill pill-warn';
      actBadge.style.display = '';
    } else if (a === 'media') {
      actBadge.textContent = 'MEDIA';
      actBadge.className = 'status-pill pill-warn';
      actBadge.style.display = '';
    } else {
      actBadge.style.display = 'none';
    }
  }

  // raw metrics — uniquement valeurs réelles, N/A si indisponible
  set('rawMemUsed',  (m.raw.mem_used_mb/1024).toFixed(2)+' GB');
  set('rawMemTotal', (m.raw.mem_total_mb/1024).toFixed(2)+' GB');
  set('rawZram',     opt(m.raw.zram_used_mb, v => v + ' MB'));
  set('rawPsiCpu',   opt(m.raw.psi_cpu, v => (v*100).toFixed(1)+'%'));
  set('rawPsiMem',   opt(m.raw.psi_mem, v => (v*100).toFixed(1)+'%'));
  set('rawCpuPct',   m.raw.cpu_pct.toFixed(1)+'%');
  set('rawCpuClock', opt(m.raw.cpu_clock_mhz, v => v.toFixed(0)+' MHz'));
  set('rawCpuMaxClock', opt(m.raw.cpu_max_clock_mhz, v => v.toFixed(0)+' MHz'));
  set('rawCpuFreqRatio', opt(m.raw.cpu_freq_ratio, v => (v*100).toFixed(1)+'%'));
  set('rawCpuTemp',  opt(m.raw.cpu_temp_c, v => v.toFixed(1)+' C'));
  set('rawRamClock', opt(m.raw.ram_clock_mhz, v => v.toFixed(0)+' MHz'));
  set('rawLoadAvg',  opt(m.raw.load_avg_1m_norm, v => v.toFixed(2)+' x/core'));
  set('rawRunnable', opt(m.raw.runnable_tasks, v => String(v)));
  set('rawIoRw',     (m.raw.io_read_mb_s != null && m.raw.io_write_mb_s != null) ? `${m.raw.io_read_mb_s.toFixed(3)}/${m.raw.io_write_mb_s.toFixed(3)} MB/s` : na);
  set('rawGpuPct',   opt(m.raw.gpu_pct, v => v.toFixed(2)+'%'));
  set('rawGpuCoreClock', opt(m.raw.gpu_core_clock_mhz, v => v.toFixed(0)+' MHz'));
  set('rawGpuMemClock', opt(m.raw.gpu_mem_clock_mhz, v => v.toFixed(0)+' MHz'));
  set('rawGpuTemp',  opt(m.raw.gpu_temp_c, v => v.toFixed(1)+' C'));
  set('rawGpuPower', opt(m.raw.gpu_power_watts, v => v.toFixed(1)+' W'));
  set('rawGpuVram',  (m.raw.gpu_mem_used_mb != null && m.raw.gpu_mem_total_mb != null)
    ? `${Number(m.raw.gpu_mem_used_mb)} / ${Number(m.raw.gpu_mem_total_mb)} MiB`
    : na);
  set('rawPowerW',   opt(m.raw.power_watts, v => v.toFixed(1)+' W'));
  set('rawWebviewCpu', (m.raw.webview_host_cpu_sum != null && m.raw.webview_host_cpu_sum !== undefined)
    ? (Number(m.raw.webview_host_cpu_sum).toFixed(1) + ' Σ%')
    : na);
  set('rawWebviewMem', (m.raw.webview_host_mem_mb != null && m.raw.webview_host_mem_mb !== undefined)
    ? (Number(m.raw.webview_host_mem_mb) + ' MiB')
    : na);

  // Activity state in raw metrics
  const actRaw = document.getElementById('rawActivity');
  if (actRaw) {
    const a = state.machineActivity || 'active';
    actRaw.textContent = a.toUpperCase();
    actRaw.style.color = a === 'active' ? 'var(--io)' : 'var(--gpu)';
  }
  updateSystemStatus(m);
  updateMetricsStrip(m);
}

/** Barre σ / π / mini r(t) sous le header */
function updateMetricsStrip(m) {
  const strip = document.getElementById('metricsStrip');
  const sSig = document.getElementById('stripSigma');
  const sPi = document.getElementById('stripPi');
  const piWrap = document.querySelector('.ms-pi-wrap');
  if (sSig) sSig.textContent = (m && m.sigma != null) ? Number(m.sigma).toFixed(2) : '—';
  const pi = typeof state.lastPi === 'number' ? state.lastPi : null;
  if (sPi) sPi.textContent = pi != null ? pi.toFixed(3) : '—';
  if (piWrap && pi != null) {
    const p = Math.min(1, Math.max(0, pi));
    piWrap.style.setProperty('--pi-pct', p.toFixed(4));
  }
  if (strip && m && m.sigma != null) {
    const sig = Number(m.sigma);
    strip.classList.toggle('ms-danger', sig >= 0.85);
    strip.classList.toggle('ms-warn', sig >= 0.55 && sig < 0.85);
  }
  const sigmaSeg = document.querySelector('.ms-seg.ms-sigma');
  if (sigmaSeg && m && m.sigma != null) {
    sigmaSeg.classList.toggle('ms-pulse', Number(m.sigma) >= 0.65);
  }
  const sigWrap = document.getElementById('stripSigmaIconWrap');
  if (sigWrap && m && m.sigma != null) {
    const sg = Number(m.sigma);
    sigWrap.classList.toggle('ms-ico-wrap--danger', sg >= 0.65);
    sigWrap.classList.toggle('ms-ico-wrap--success', sg < 0.35);
  }
  const fcpu = document.getElementById('stripCpuFill');
  const fmem = document.getElementById('stripMemFill');
  const fio = document.getElementById('stripIoFill');
  if (fcpu) fcpu.style.transform = 'scaleX(' + (m && m.cpu != null ? Number(m.cpu).toFixed(4) : '0') + ')';
  if (fmem) fmem.style.transform = 'scaleX(' + (m && m.mem != null ? Number(m.mem).toFixed(4) : '0') + ')';
  if (fio) fio.style.transform = 'scaleX(' + (m && m.io_bandwidth != null ? Number(m.io_bandwidth).toFixed(4) : '0') + ')';
}

function setBar(id, val, _label) {
  const vEl = document.getElementById('v'+id);
  const bEl = document.getElementById('b'+id);
  const num = (val != null && typeof val === 'number') ? val : 0;
  if (vEl) {
    const txt = val != null ? num.toFixed(3) : (state.naLabel || 'N/A');
    if (vEl.textContent !== txt) vEl.textContent = txt;
  }
  if (bEl) bEl.style.transform = `scaleX(${num.toFixed(4)})`;
}

function renderFormula(m) {
  const alpha = WORKLOADS[state.wl];
  const r     = [m.cpu, m.mem, m.compression ?? 0, m.io_bandwidth ?? 0, m.gpu ?? 0];
  const eps   = m.epsilon;

  const cpuFreqRatio = Number(m.raw?.cpu_freq_ratio ?? NaN);
  const cpuHeadroom = Number.isFinite(cpuFreqRatio) ? Math.max(0, 1 - cpuFreqRatio) : Math.max(0, 1 - Number(m.cpu || 0));
  const memHeadroom = Math.max(0, Math.min(1, Number(m.mem || 0)));
  const ioHeadroom = Math.max(0, 1 - Number(m.io_bandwidth ?? 0));
  const gpuHeadroom = Math.max(0, 1 - Number(m.gpu ?? 0));
  const uiPenalty = Math.min(0.25, Math.max(0, Number(m.raw?.webview_host_cpu_sum ?? 0) / 100));
  const opportunity = Math.max(0.70, Math.min(1.35,
    0.85 + 0.40 * (
      alpha[0] * cpuHeadroom +
      alpha[1] * memHeadroom +
      alpha[3] * ioHeadroom +
      alpha[4] * gpuHeadroom
    ) - 0.20 * uiPenalty
  ));

  const cpuHot = Number.isFinite(Number(m.raw?.cpu_temp_c)) ? Math.max(0, Math.min(1, (Number(m.raw.cpu_temp_c) - 80) / 18)) : 0;
  const gpuHot = Number.isFinite(Number(m.raw?.gpu_temp_c)) ? Math.max(0, Math.min(1, (Number(m.raw.gpu_temp_c) - 76) / 16)) : 0;
  const vramPressure = (m.raw?.gpu_mem_used_mb != null && Number(m.raw?.gpu_mem_total_mb || 0) > 0)
    ? Math.max(0, Math.min(1, Number(m.raw.gpu_mem_used_mb) / Number(m.raw.gpu_mem_total_mb)))
    : 0;
  const loadPressure = Number.isFinite(Number(m.raw?.load_avg_1m_norm))
    ? Math.max(0, Math.min(1, (Number(m.raw.load_avg_1m_norm) - 0.9) / 1.3))
    : 0;
  const advancedGuard = Math.max(0.45, Math.min(1,
    1 - 0.30 * cpuHot - 0.25 * gpuHot - 0.18 * Math.max(alpha[4] * vramPressure, alpha[0] * loadPressure)
  ));

  const sigmaEffective = Math.max(0, Math.min(1,
    Number(m.sigma || 0) +
    0.16 * (Number.isFinite(Number(m.raw?.load_avg_1m_norm)) ? Math.max(0, Math.min(1, (Number(m.raw.load_avg_1m_norm) - 1.0) / 1.5)) : 0) +
    0.12 * ((Number(m.raw?.gpu_power_watts ?? 0) / 220) * alpha[4]) +
    0.10 * ((Math.max(0, Number(m.raw?.runnable_tasks ?? 0) - 2) / 10) * alpha[0])
  ));

  const brut    = Math.min(1.2, alpha.reduce((s,a,i) => s + a*r[i], 0) * opportunity);
  const fric    = Math.min(1, alpha.reduce((p,a,i) => p * Math.pow(Math.max(0,1-eps[i]), a), 1) * advancedGuard);
  const brake   = Math.exp(-state.kappa * sigmaEffective);
  const pi      = brut * fric * brake;

  state.lastPi = pi;
  const deltaK  = [0.9,0.85,0.95,0.8,0.7];
  const bIdle   = alpha.reduce((s,a,i) => s + a*(1-r[i]*0.7)*deltaK[i], 0);

  const piEl = document.getElementById('piVal');
  piEl.textContent = 'π = ' + pi.toFixed(5);
  piEl.className   = 'eq-result' + (pi>.5?' high':pi<.2?' low':'');

  // 𝒟 = real accumulated integral (not projection)
  const domeEl = document.getElementById('domeVal');
  if (state.domeActive) {
    const realD = state.domeRealIntegral - 0.04 - 0.02;
    domeEl.textContent = '𝒟 = ' + realD.toFixed(4);
    state.lastDomeModel = realD;
  } else {
    domeEl.textContent = '𝒟 = —';
    state.lastDomeModel = null;
  }

  set('mBrut',  brut.toFixed(4));
  set('mFric',  fric.toFixed(4));
  set('mFrein', brake.toFixed(4));

  // RENTABLE: based on real action success + real integral (when dome active)
  const rEl = document.getElementById('mRenta');
  if (state.domeActive) {
    const act = state.machineActivity || 'active';
    if (act === 'idle' || act === 'media') {
      rEl.textContent = 'PAUSE (' + act + ')';
      rEl.style.color = 'var(--gpu)';
    } else if (state.domeActionsOk === 0 && state.domeActionsTotal > 0) {
      rEl.textContent = 'NON ✗ (refusé)';
      rEl.style.color = 'var(--stress)';
    } else if (state.domeRealIntegral > 0.06) {
      rEl.textContent = 'OUI ✓';
      rEl.style.color = 'var(--io)';
    } else {
      rEl.textContent = '...';
      rEl.style.color = 'var(--muted)';
    }
  } else {
    // Dome off: show theoretical projection as before
    const T = { es:60, compile:120, gamer:90, ai:30, backup:300, sqlite:20, oracle:180 }[state.wl] || 60;
    const dGain = pi * T - 0.04 - 0.02;
    rEl.textContent = dGain > 0 ? 'OUI ✓' : 'NON ✗';
    rEl.style.color = dGain > 0 ? 'var(--io)' : 'var(--stress)';
  }

  // dome bars = α_i · r_i · 50px
  alpha.forEach((a,i) => {
    const el = document.getElementById('dc'+i);
    if (el) el.style.transform = `scaleY(${Math.max(0.06, a * r[i]).toFixed(4)})`;
  });

  updateMetricsStrip(m);
}

// ─── Platform info ────────────────────────────────────────────────────────────
async function loadPlatformInfo() {
  const badge = document.getElementById('platformBadge');
  try {
    const info = await invoke('platform_info');
    if (!badge) return;
    if (hasTauri) {
      badge.textContent = info.os + ' · ' + info.kernel;
      badge.title = 'Métriques réelles (runtime Tauri)';
    } else {
      badge.textContent = info.os + ' · ' + info.kernel;
      badge.title = 'Hors Tauri : lancez "cargo tauri dev" ou l\'exécutable pour les métriques réelles.';
    }
    const tags = document.getElementById('featTags');
    if (tags) {
      tags.innerHTML = '';
      info.features.forEach(f => {
        const t = document.createElement('span');
        t.className = 'feat-tag active';
        t.textContent = f;
        tags.appendChild(t);
      });
    }
    log(`Platform: ${info.os} · cgroups-v2:${info.has_cgroups_v2} · zRAM:${info.has_zram} · root:${info.is_root}`, 'ok');
  } catch(e) {
    if (badge) {
      badge.textContent = hasTauri ? 'Connected' : 'Hors Tauri — lancez l\'app native';
      badge.title = hasTauri ? '' : 'Ouvrez via cargo tauri dev pour les métriques réelles.';
    }
    log(hasTauri ? `platform_info error: ${e}` : 'Mode navigateur — pas de métriques réelles (lancez cargo tauri dev)', 'warn');
  }
}

// ─── Dome activation ──────────────────────────────────────────────────────────
function wireLegacyDomListeners() {
  const btnDome = document.getElementById('btnDome');
  if (!btnDome) {
    console.warn('SoulKernel: shell DOM absent (btnDome) — écouteurs non branchés');
    return;
  }

  btnDome.addEventListener('click', async () => {
  try {
    const pidVal = document.getElementById('targetProcess').value;
    state.targetPid = pidVal === '' ? null : parseInt(pidVal, 10);
    log('Activation dôme : profil ' + state.wl + ' · κ=' + state.kappa + ' · Σmax=' + state.sigmaMax.toFixed(2) + (state.targetPid ? ' · cible PID ' + state.targetPid : ' · cible SoulKernel'), 'info');
    const result = await invoke('activate_dome', {
      workload:  state.wl,
      kappa:     state.kappa,
      sigmaMax:  state.sigmaMax,
      eta:       state.eta,
      targetPid: state.targetPid,
      policyMode: state.policyMode,
    });
    state.domeActive = result.activated;
    if (result.activated) {
      state.domeActionsOk = result.actions_ok || 0;
      state.domeActionsTotal = result.actions_total || 0;
      state.domeRealIntegral = 0;
      state.domeRealLastTs = Date.now();
      try { state.snapshotBefore = await invoke('get_snapshot_before_dome') || null; } catch (_) { state.snapshotBefore = null; }
      state.domeHistory.unshift({
        ts: new Date().toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit', second: '2-digit' }),
        workload: state.wl,
        pi: result.pi,
        domeGain: 0,
        targetPid: state.targetPid,
        actionsOk: state.domeActionsOk,
        actionsTotal: state.domeActionsTotal,
      });
      if (state.domeHistory.length > MAX_DOME_HISTORY) state.domeHistory.pop();
      renderGainsHistory();
      logDomeActivationGains(result, { context: 'bouton', workload: state.wl, targetPid: state.targetPid });
    } else {
      log(result.message, 'warn');
    }
    result.actions.forEach(a => log(a, a.startsWith('✓') ? 'ok' : 'warn'));
    updateDomeBadge(result.activated);
    document.getElementById('fZone').classList.toggle('active', result.activated);
    document.getElementById('dZone').classList.toggle('active', result.activated);
    renderProofPanel();
    renderCompactHud(state.lastMetrics);
    saveStartupIntent();
  } catch(e) {
    log(`Dome error: ${e}`, 'err');
  }
});

// ─── Rollback ─────────────────────────────────────────────────────────────────
document.getElementById('btnReset').addEventListener('click', async () => {
  try {
    const actions = await invoke('rollback_dome');
    if (state.domeHistory.length > 0 && state.domeRealIntegral > 0) {
      state.domeHistory[0].domeGain = state.domeRealIntegral - 0.06;
    }
    if (state.domeRealIntegral > 0) {
      logDomeSessionIntegralEnd(state.domeRealIntegral - 0.06, { reason: 'rollback' });
    }
    state.domeActive = false;
    state.domeRealIntegral = 0;
    state.domeRealLastTs = null;
    state.domeActionsOk = 0;
    state.domeActionsTotal = 0;
    actions.forEach(a => log(a, a.startsWith('✓') ? 'ok' : 'warn'));
    state.snapshotBefore = null;
    log('ROLLBACK — kernel params restored', 'ok');
    updateDomeBadge(false);
    document.getElementById('fZone').classList.remove('active');
    document.getElementById('dZone').classList.remove('active');
    renderProofPanel();
    renderCompactHud(state.lastMetrics);
    saveStartupIntent();
  } catch(e) {
    log(`Rollback error: ${e}`, 'err');
  }
});

document.getElementById('btnInfo').addEventListener('click', loadPlatformInfo);
}

function formatMetricsShort(m) {
  if (!m || !m.raw) return '�';
  const r = m.raw;
  const mem = (r.mem_used_mb / 1024).toFixed(1) + '/' + (r.mem_total_mb / 1024).toFixed(1) + ' GB';
  return 'CPU ' + r.cpu_pct.toFixed(1) + '% | RAM ' + mem + ' | sigma ' + m.sigma.toFixed(3);
}

function formatDelta(delta, unit, invertGood) {
  if (unit == null) unit = '';
  if (invertGood == null) invertGood = false;
  if (delta == null || !Number.isFinite(delta)) return 'N/A';
  const sign = delta > 0 ? '+' : '';
  const val = sign + delta.toFixed(2) + unit;
  const better = invertGood ? (delta < 0) : (delta > 0);
  if (Math.abs(delta) < 0.01) return val + ' (stable)';
  return val + (better ? ' (mieux)' : ' (moins bien)');
}

function renderProofPanel() {
  const fold = document.getElementById('proofFold');
  if (fold && !fold.open && !state.domeActive) return;
  const placeholder = document.getElementById('proofPlaceholder');
  const compare = document.getElementById('proofCompare');
  const beforeEl = document.getElementById('proofBefore');
  const afterEl = document.getElementById('proofAfter');
  const summaryEl = document.getElementById('proofSummary');
  if (!compare || !placeholder || !beforeEl || !afterEl || !summaryEl) return;

  if (state.domeActive && state.snapshotBefore && state.lastMetrics) {
    const b = state.snapshotBefore;
    const a = state.lastMetrics;

    const cpuDelta = Number(a.raw.cpu_pct || 0) - Number(b.raw.cpu_pct || 0);
    const memBeforeGb = Number(b.raw.mem_used_mb || 0) / 1024;
    const memAfterGb = Number(a.raw.mem_used_mb || 0) / 1024;
    const memDelta = memAfterGb - memBeforeGb;
    const sigmaDelta = Number(a.sigma || 0) - Number(b.sigma || 0);

    placeholder.style.display = 'none';
    compare.style.display = 'flex';
    summaryEl.style.display = 'block';

    beforeEl.textContent = formatMetricsShort(b);
    afterEl.textContent =
      formatMetricsShort(a) +
      ' | delta CPU ' + formatDelta(cpuDelta, '%', true) +
      ' | delta RAM ' + formatDelta(memDelta, ' GB', true) +
      ' | delta sigma ' + formatDelta(sigmaDelta, '', true);

    let verdict = 'Impact limite (lecture instantanee).';
    let score = 0;
    if (cpuDelta <= -2.0) score += 1;
    if (memDelta <= -0.2) score += 1;
    if (sigmaDelta <= -0.02) score += 1;
    if (cpuDelta >= 2.0) score -= 1;
    if (memDelta >= 0.2) score -= 1;
    if (sigmaDelta >= 0.02) score -= 1;

    if (score >= 2) verdict = 'Tendance favorable apres activation du dome.';
    else if (score <= -2) verdict = 'Tendance defavorable: verifier profil/cible.';

    summaryEl.textContent = verdict + ' Cette preuve reste instantanee; le benchmark A/B fait foi pour la performance reelle.';
  } else {
    placeholder.style.display = 'block';
    compare.style.display = 'none';
    summaryEl.style.display = 'none';
  }
}
function renderGainsHistory() {
  const summary = document.getElementById('gainsSummary');
  const listEl = document.getElementById('gainsList');
  if (state.domeHistory.length === 0) {
    const lt = state.telemetrySummary?.lifetime;
    if (lt && lt.total_dome_activations > 0) {
      summary.textContent = `Vie entiere : ${lt.total_dome_activations} activation(s), D cumule = ${Number(lt.total_dome_gain_integral || 0).toFixed(2)}`;
    } else {
      summary.textContent = '— Aucune activation pour l’instant';
    }
    listEl.innerHTML = '';
    return;
  }
  const n = state.domeHistory.length;
  const avgD = state.domeHistory.reduce((s, e) => s + e.domeGain, 0) / n;
  const lt = state.telemetrySummary?.lifetime;
  const lifetimeN = lt?.total_dome_activations || n;
  summary.textContent = `Vie entiere : ${lifetimeN} activation(s), D cumule = ${Number(lt?.total_dome_gain_integral || 0).toFixed(2)} | Session: ${n} dome(s), D moy = ${avgD.toFixed(2)}`;
  listEl.innerHTML = state.domeHistory.map(e => {
    const dVal = (e.domeGain != null && e.domeGain !== 0) ? e.domeGain.toFixed(2) : '...';
    const actInfo = (e.actionsOk != null && e.actionsTotal != null) ? ` [${e.actionsOk}/${e.actionsTotal}]` : '';
    return `<div class="gains-row"><span>${e.ts}</span><span>${e.workload}</span><span class="gains-pi">pi=${e.pi.toFixed(4)}</span><span class="gains-d">D=${dVal}${actInfo}</span><span>${e.targetPid ? 'PID ' + e.targetPid : '-'}</span></div>`;
  }).join('');
  // Persist dome history to localStorage
  saveDomeHistory();
}

function saveDomeHistory() {
  try {
    localStorage.setItem('soulkernel.dome_history', JSON.stringify(state.domeHistory.slice(0, MAX_DOME_HISTORY)));
  } catch (_) {}
}

function loadDomeHistory() {
  try {
    const raw = localStorage.getItem('soulkernel.dome_history');
    if (!raw) return;
    const arr = JSON.parse(raw);
    if (Array.isArray(arr) && arr.length > 0) {
      state.domeHistory = arr.slice(0, MAX_DOME_HISTORY);
      renderGainsHistory();
    }
  } catch (_) {}
}

function collectVisibleLogLines() {
  const panel = document.getElementById('logPanel');
  if (!panel) return [];
  return Array.from(panel.querySelectorAll('.log-entry'))
    .map(el => (el.textContent || '').replace(/\s+/g, ' ').trim())
    .filter(Boolean);
}

function selectedTargetSnapshot() {
  const sel = document.getElementById('targetProcess');
  const value = sel ? sel.value : '';
  const label = sel && sel.selectedIndex >= 0
    ? (sel.options[sel.selectedIndex].text || '')
    : '';
  return {
    target_pid: value === '' ? null : Number(value),
    target_label: label || 'Ce processus (SoulKernel)',
  };
}

function collectProcessImpactExport() {
  const report = state.processImpactReport && typeof state.processImpactReport === 'object'
    ? state.processImpactReport
    : { processes: [], top_processes: [], top_process_rows: [], grouped_processes: [], overhead_audit: null, summary: null };
  const list = Array.isArray(report.processes) ? report.processes : [];
  const target = selectedTargetSnapshot();
  const top = Array.isArray(report.top_processes) && report.top_processes.length
    ? report.top_processes.slice(0, 20)
    : [...list]
      .sort((a, b) => Number(b.impact_score_pct_estimated || 0) - Number(a.impact_score_pct_estimated || 0))
      .slice(0, 20);
  return {
    exported_at: new Date().toISOString(),
    process_count: list.length,
    selected_target: target,
    summary: report.summary || null,
    grouped_processes: Array.isArray(report.grouped_processes) ? report.grouped_processes : [],
    top_process_rows: Array.isArray(report.top_process_rows) ? report.top_process_rows : [],
    overhead_audit: report.overhead_audit || null,
    attribution_notice:
      'CPU, GPU, RAM et I/O par processus sont observés selon disponibilité plateforme. impact_score_pct_estimated et estimated_power_w restent des attributions estimées, pas une mesure énergétique directe par processus.',
    processes: list,
    top_contributors: top,
  };
}

function benchmarkDiagPayload(extra = {}) {
  const target = selectedTargetSnapshot();
  return {
    workload: state.wl,
    kappa: Number(state.kappa.toFixed(2)),
    sigma_max: Number(state.sigmaMax.toFixed(2)),
    eta: Number(state.eta.toFixed(2)),
    dome_active: !!state.domeActive,
    auto_process_target: !!state.autoProcessTarget,
    state_target_pid: state.targetPid,
    target_pid: target.target_pid,
    target_label: target.target_label,
    dome_history_len: state.domeHistory.length,
    dome_actions_ok: state.domeActionsOk,
    dome_actions_total: state.domeActionsTotal,
    ...extra,
  };
}

function collectEnergyMeterExport() {
  const t = state.telemetrySummary || null;
  const livePowerW = t?.live_power_w != null ? Number(t.live_power_w) : null;
  const powerSource = t?.power_source || null;
  const hasRealPower = !!t?.data_real_power;
  const pricing = t?.pricing ? {
    currency: t.pricing.currency || 'EUR',
    price_per_kwh: t.pricing.price_per_kwh ?? null,
    co2_kg_per_kwh: t.pricing.co2_kg_per_kwh ?? null,
  } : null;
  const windows = t ? {
    total_kwh: t.total?.energy_kwh ?? null,
    total_cost: t.total?.cost ?? null,
    total_co2_kg: t.total?.co2_kg ?? null,
    hour_kwh: t.hour?.energy_kwh ?? null,
    day_kwh: t.day?.energy_kwh ?? null,
    week_kwh: t.week?.energy_kwh ?? null,
    month_kwh: t.month?.energy_kwh ?? null,
    year_kwh: t.year?.energy_kwh ?? null,
  } : null;
  const lifetime = t?.lifetime ? {
    total_energy_kwh: t.lifetime.total_energy_kwh ?? null,
    total_energy_cost_measured: t.lifetime.total_energy_cost_measured ?? null,
    total_co2_measured_kg: t.lifetime.total_co2_measured_kg ?? null,
    has_real_power: !!t.lifetime.has_real_power,
  } : null;
  const external = {
    source_tag: String(document.getElementById('merossSourceTag')?.textContent || '').trim() || null,
    last_watts_label: String(document.getElementById('merossLastWatts')?.textContent || '').trim() || null,
    freshness: String(document.getElementById('merossFreshness')?.textContent || '').trim() || null,
    file_presence: String(document.getElementById('merossFilePresence')?.textContent || '').trim() || null,
    bridge_state: String(document.getElementById('merossBridgeRunning')?.textContent || '').trim() || null,
    runtime: String(document.getElementById('merossPythonRuntime')?.textContent || '').trim() || null,
    config_path: String(document.getElementById('merossConfigPath')?.textContent || '').trim() || null,
    power_file_path: String(document.getElementById('merossResolvedPowerFile')?.textContent || '').trim() || null,
    cache_path: String(document.getElementById('merossCredsCachePath')?.textContent || '').trim() || null,
    last_ts_label: String(document.getElementById('merossLastTs')?.textContent || '').trim() || null,
    last_error: String(document.getElementById('merossBridgeError')?.textContent || '').trim() || null,
  };
  return {
    has_real_power: hasRealPower,
    live_power_w: livePowerW,
    power_source: powerSource,
    is_external_wall_source: powerSource === 'meross_wall',
    pricing,
    windows,
    lifetime,
    external_power: external,
  };
}

function collectStrictEvidenceExport() {
  const t = state.telemetrySummary || null;
  const meter = collectEnergyMeterExport();
  const lastBench = state.kpiBench?.lastSummary || null;
  const live = state.lastMetrics || null;
  const hasMeasuredEnergy = !!t?.total?.has_power_data;
  const hasRealBenchmark = !!(lastBench && lastBench.samples_off_ok > 0 && lastBench.samples_on_ok > 0);
  const allowedClaims = [
    'Mesures OS natives: CPU, RAM, GPU, I/O, sigma selon disponibilité plateforme.',
    hasMeasuredEnergy
      ? 'Consommation énergétique mesurée: kWh, coût et CO2 calculés depuis une puissance réelle.'
      : 'Aucune affirmation énergétique stricte sans capteur de puissance réel.',
    hasRealBenchmark
      ? 'Le benchmark A/B permet une comparaison OFF vs ON reproductible sur une commande donnée.'
      : 'Aucune affirmation stricte de gain sans benchmark A/B exploitable.'
  ];
  const forbiddenClaims = [
    'Ne pas présenter π(t) ou ∫𝒟 comme une mesure physique.',
    'Ne pas présenter CPU·h ou RAM·GB·h différentielles comme des économies matérielles absolues.',
    'Ne pas présenter coût ou CO2 mesurés comme des gains évités sans baseline énergétique OFF vs ON.'
  ];
  return {
    mode: 'strict_evidence',
    exported_at: new Date().toISOString(),
    machine_activity: state.machineActivity || 'active',
    assertions: {
      measured_os_metrics: {
        cpu_pct: live?.raw?.cpu_pct ?? null,
        mem_used_mb: live?.raw?.mem_used_mb ?? null,
        mem_total_mb: live?.raw?.mem_total_mb ?? null,
        io_read_mb_s: live?.raw?.io_read_mb_s ?? null,
        io_write_mb_s: live?.raw?.io_write_mb_s ?? null,
        gpu_pct: live?.raw?.gpu_pct ?? null,
        sigma: live?.sigma ?? null,
      },
      measured_energy: hasMeasuredEnergy ? {
        power_source: t?.power_source || null,
        live_power_w: t?.live_power_w ?? null,
        total_energy_kwh: t?.total?.energy_kwh ?? null,
        total_cost: t?.total?.cost ?? null,
        total_co2_kg: t?.total?.co2_kg ?? null,
        period_windows: {
          hour_kwh: t?.hour?.energy_kwh ?? null,
          day_kwh: t?.day?.energy_kwh ?? null,
          week_kwh: t?.week?.energy_kwh ?? null,
          month_kwh: t?.month?.energy_kwh ?? null,
          year_kwh: t?.year?.energy_kwh ?? null,
        },
      } : null,
      measured_differentials: t ? {
        cpu_hours_differential: t.total?.cpu_hours_differential ?? null,
        mem_gb_hours_differential: t.total?.mem_gb_hours_differential ?? null,
        lifetime_cpu_hours_differential: t.lifetime?.total_cpu_hours_differential ?? null,
        lifetime_mem_gb_hours_differential: t.lifetime?.total_mem_gb_hours_differential ?? null,
        idle_ratio: t.total?.idle_ratio ?? null,
        media_ratio: t.total?.media_ratio ?? null,
      } : null,
      benchmark_ab: lastBench ? {
        samples_off_ok: lastBench.samples_off_ok ?? 0,
        samples_on_ok: lastBench.samples_on_ok ?? 0,
        gain_median_pct: lastBench.gain_median_pct ?? null,
        gain_p95_pct: lastBench.gain_p95_pct ?? null,
        gain_power_median_pct: lastBench.gain_power_median_pct ?? null,
        gain_cpu_median_pct: lastBench.gain_cpu_median_pct ?? null,
        gain_mem_median_pct: lastBench.gain_mem_median_pct ?? null,
        gain_sigma_median_pct: lastBench.gain_sigma_median_pct ?? null,
      } : null,
    },
    allowed_claims: allowedClaims,
    forbidden_claims: forbiddenClaims,
    external_power: meter.external_power,
  };
}

function collectEnergyPeriodReport(periodKey) {
  const t = state.telemetrySummary || null;
  const w = t?.[periodKey] || null;
  return {
    period: periodKey,
    exported_at: new Date().toISOString(),
    currency: t?.pricing?.currency || 'EUR',
    power_source: t?.power_source || null,
    live_power_w: t?.live_power_w ?? null,
    has_power_data: !!w?.has_power_data,
    energy_kwh: w?.energy_kwh ?? null,
    cost: w?.cost ?? null,
    co2_kg: w?.co2_kg ?? null,
    duration_h: w?.duration_h ?? null,
    avg_power_w: w?.avg_power_w ?? null,
    samples: w?.samples ?? null,
    external_power: collectEnergyMeterExport().external_power,
    strict_evidence: collectStrictEvidenceExport(),
    process_impact_report: collectProcessImpactExport(),
  };
}

async function exportEnergyPeriodReport(periodKey) {
  const labelMap = { hour: 'hourly', day: 'daily', week: 'weekly', month: 'monthly' };
  const path = await invoke('export_gains_to_file', {
    content: JSON.stringify({
      product: 'SoulKernel',
      report_type: 'energy_period',
      period: periodKey,
      period_label: labelMap[periodKey] || periodKey,
      report: collectEnergyPeriodReport(periodKey),
      telemetry_summary: state.telemetrySummary,
      strict_evidence: collectStrictEvidenceExport(),
      process_impact_report: collectProcessImpactExport(),
    }, null, 2),
  });
  return path;
}

function buildSessionReportText() {
  const now = new Date().toISOString();
  const diag = benchmarkDiagPayload({
    report_type: 'session-report',
    exported_at: now,
    machine_activity: state.machineActivity || 'active',
    has_snapshot_before: !!state.snapshotBefore,
    last_benchmark_gain_median_pct: state.kpiBench.lastSummary?.gain_median_pct ?? null,
    last_benchmark_gain_p95_pct: state.kpiBench.lastSummary?.gain_p95_pct ?? null,
  });
  const avgD = state.domeHistory.length
    ? state.domeHistory.reduce((s, e) => s + e.domeGain, 0) / state.domeHistory.length
    : 0;
  const meter = collectEnergyMeterExport();
  const proc = collectProcessImpactExport();
  const lines = [];

  lines.push('SoulKernel - Rapport complet de session');
  lines.push('Export: ' + now);
  lines.push('');

  lines.push('Parametres actifs');
  lines.push('Workload: ' + state.wl);
  lines.push('kappa: ' + state.kappa.toFixed(2) + ' | sigmaMax: ' + state.sigmaMax.toFixed(2) + ' | eta: ' + state.eta.toFixed(2));
  lines.push('Dome actif: ' + (state.domeActive ? 'oui' : 'non'));
  lines.push('Machine activite: ' + (state.machineActivity || 'active'));
  lines.push('D reel (integral): ' + (state.domeActive ? (state.domeRealIntegral - 0.06).toFixed(4) : 'N/A'));
  lines.push('Actions dome: ' + state.domeActionsOk + '/' + state.domeActionsTotal + ' ok');
  lines.push('Cible: ' + diag.target_label);
  lines.push('Auto-cible: ' + (state.autoProcessTarget ? 'on' : 'off'));
  lines.push('SoulRAM: ' + (state.soulRamActive ? 'on' : 'off') + ' (' + state.soulRamPercent + '%)');
  lines.push('SoulRAM reboot requis: ' + (state.soulRamNeedsReboot ? 'oui' : 'non'));
  lines.push('Adaptive: ' + (state.adaptiveEnabled ? 'on' : 'off') + ' | auto-dome: ' + (state.adaptiveAutoDome ? 'on' : 'off'));
  lines.push('Audit JSONL: ' + (state.auditLogPath || 'N/A')); 

  lines.push('');
  lines.push('Diagnostic export');
  lines.push('Workload/state: ' + diag.workload + ' | auto-cible=' + (diag.auto_process_target ? 'on' : 'off'));
  lines.push('Target state/select: ' + (diag.state_target_pid ?? 'null') + ' / ' + (diag.target_pid ?? 'null'));
  lines.push('Target label: ' + diag.target_label);
  lines.push('Dome history len: ' + diag.dome_history_len + ' | snapshot before=' + (diag.has_snapshot_before ? 'oui' : 'non'));
  lines.push('Last benchmark gain median/p95: ' + (diag.last_benchmark_gain_median_pct == null ? 'N/A' : Number(diag.last_benchmark_gain_median_pct).toFixed(1) + '%') + ' / ' + (diag.last_benchmark_gain_p95_pct == null ? 'N/A' : Number(diag.last_benchmark_gain_p95_pct).toFixed(1) + '%'));
  lines.push('Diag JSON: ' + JSON.stringify(diag));

  if (state.lastMetrics && state.lastMetrics.raw) {
    const r = state.lastMetrics.raw;
    const na = v => (v != null ? String(v) : 'N/A');
    lines.push('');
    lines.push('Metriques');
    lines.push('CPU: ' + r.cpu_pct.toFixed(1) + '% | RAM: ' + (r.mem_used_mb / 1024).toFixed(2) + '/' + (r.mem_total_mb / 1024).toFixed(2) + ' GB');
    lines.push('Swap: ' + r.swap_used_mb + '/' + r.swap_total_mb + ' MB | Sigma: ' + state.lastMetrics.sigma.toFixed(3));
    lines.push('Clocks: CPU=' + (r.cpu_clock_mhz != null ? Number(r.cpu_clock_mhz).toFixed(0) + ' MHz' : 'N/A') + ' / max=' + (r.cpu_max_clock_mhz != null ? Number(r.cpu_max_clock_mhz).toFixed(0) + ' MHz' : 'N/A') + ' | RAM=' + (r.ram_clock_mhz != null ? Number(r.ram_clock_mhz).toFixed(0) + ' MHz' : 'N/A') + ' | GPU core=' + (r.gpu_core_clock_mhz != null ? Number(r.gpu_core_clock_mhz).toFixed(0) + ' MHz' : 'N/A') + ' | GPU mem=' + (r.gpu_mem_clock_mhz != null ? Number(r.gpu_mem_clock_mhz).toFixed(0) + ' MHz' : 'N/A'));
    lines.push('Advanced: CPU temp=' + (r.cpu_temp_c != null ? Number(r.cpu_temp_c).toFixed(1) + ' C' : 'N/A') + ' | load1/core=' + (r.load_avg_1m_norm != null ? Number(r.load_avg_1m_norm).toFixed(2) : 'N/A') + ' | runnable=' + na(r.runnable_tasks) + ' | GPU temp=' + (r.gpu_temp_c != null ? Number(r.gpu_temp_c).toFixed(1) + ' C' : 'N/A') + ' | GPU power=' + (r.gpu_power_watts != null ? Number(r.gpu_power_watts).toFixed(1) + ' W' : 'N/A'));
    lines.push('Plateforme: ' + r.platform);

    lines.push('');
    lines.push('Hardware r(t) detail');
    lines.push('C(t) CPU: ' + (state.lastMetrics.cpu != null ? state.lastMetrics.cpu.toFixed(3) : 'N/A') + ' | alpha_C=0.2 | cpu=' + r.cpu_pct.toFixed(1) + '%');
    lines.push('M(t) RAM: ' + (state.lastMetrics.mem != null ? state.lastMetrics.mem.toFixed(3) : 'N/A') + ' | ' + (r.mem_used_mb / 1024).toFixed(1) + '/' + (r.mem_total_mb / 1024).toFixed(1) + ' GB');
    lines.push('Lambda(t) Compression: ' + (state.lastMetrics.compression != null ? state.lastMetrics.compression.toFixed(3) : 'N/A') + ' | zRAM/zSWAP=' + na(r.zram_used_mb));
    lines.push('Bio(t) I/O: ' + (state.lastMetrics.io_bandwidth != null ? state.lastMetrics.io_bandwidth.toFixed(3) : 'N/A') + ' | R/W=' + na(r.io_read_mb_s) + '/' + na(r.io_write_mb_s) + ' MB/s');
    lines.push('G(t) GPU: ' + (state.lastMetrics.gpu != null ? state.lastMetrics.gpu.toFixed(3) : 'N/A') + ' | gpu=' + na(r.gpu_pct) + '% | core=' + (r.gpu_core_clock_mhz != null ? Number(r.gpu_core_clock_mhz).toFixed(0) + ' MHz' : 'N/A') + ' | mem=' + (r.gpu_mem_clock_mhz != null ? Number(r.gpu_mem_clock_mhz).toFixed(0) + ' MHz' : 'N/A'));
    lines.push('PSI cpu/mem: ' + na(r.psi_cpu != null ? (r.psi_cpu * 100).toFixed(1) : null) + '%/' + na(r.psi_mem != null ? (r.psi_mem * 100).toFixed(1) : null) + '%');
  }

  lines.push('');
  lines.push('Gains dome - session');
  lines.push('Heure\tWorkload\tpi\tD reel\tActions\tCible');
  if (state.domeHistory.length === 0) {
    if (state.domeActive) {
      lines.push('Activation en cours - historique session pas encore persiste');
    } else {
      lines.push('Aucune activation');
    }
  } else {
    state.domeHistory.forEach(e => {
      const dVal = (e.domeGain != null && e.domeGain !== 0) ? e.domeGain.toFixed(2) : '...';
      const actTxt = (e.actionsOk != null && e.actionsTotal != null) ? (e.actionsOk + '/' + e.actionsTotal) : '-';
      lines.push(e.ts + '\t' + e.workload + '\t' + e.pi.toFixed(4) + '\t' + dVal + '\t' + actTxt + '\t' + (e.targetPid || '-'));
    });
    lines.push('Session: ' + state.domeHistory.length + ' activation(s), D moyen = ' + avgD.toFixed(2));
  }

  if (state.kpiBench.lastSummary) {
    const s = state.kpiBench.lastSummary;
    const f = v => (v == null ? 'N/A' : v.toFixed(1));
    lines.push('');
    lines.push('A/B KPI (dernier benchmark)');
    lines.push('OFF median/p95: ' + f(s.median_off_ms) + '/' + f(s.p95_off_ms) + ' ms');
    lines.push('ON  median/p95: ' + f(s.median_on_ms) + '/' + f(s.p95_on_ms) + ' ms');
    lines.push('Gain temps median/p95: ' + f(s.gain_median_pct) + '% / ' + f(s.gain_p95_pct) + '%');
    const fr = k => (s[k] == null ? 'N/A' : Number(s[k]).toFixed(1) + '%');
    lines.push('Gain RAM/GPU/CPU apres sonde (med.): ' + fr('gain_mem_median_pct') + ' / ' + fr('gain_gpu_median_pct') + ' / ' + fr('gain_cpu_median_pct'));
  }


  if (state.telemetrySummary) {
    const t = state.telemetrySummary;
    const f = v => (v == null ? 'N/A' : Number(v).toFixed(3));
    const ccy = t.pricing?.currency || 'EUR';
    lines.push('');
    lines.push('Telemetry energie (reelle si Power Meter disponible)');
    lines.push('Prix electricite: ' + f(t.pricing?.price_per_kwh) + ' ' + ccy + '/kWh | CO2: ' + f(t.pricing?.co2_kg_per_kwh) + ' kg/kWh');
    lines.push('Power live: ' + (t.live_power_w == null ? 'N/A' : Number(t.live_power_w).toFixed(1) + ' W'));
    if (t.total?.has_power_data) {
      lines.push('Total: ' + f(t.total?.energy_kwh) + ' kWh | cout=' + f(t.total?.cost) + ' ' + ccy + ' | CO2=' + f(t.total?.co2_kg) + ' kg');
      lines.push('Fenetres kWh H/J/S/M/A: ' + [f(t.hour?.energy_kwh), f(t.day?.energy_kwh), f(t.week?.energy_kwh), f(t.month?.energy_kwh), f(t.year?.energy_kwh)].join('/'));
    } else {
      lines.push('Total: N/A (aucune mesure de puissance reelle)');
      lines.push('Fenetres kWh H/J/S/M/A: N/A');
    }
    lines.push('Optimisation reelle mediane A/B (historique global): ' + (t.total?.kpi_gain_median_pct == null ? 'N/A' : Number(t.total.kpi_gain_median_pct).toFixed(2) + '%'));
    lines.push('CPU·h / RAM·GB·h diff. (fenetre telemetrie): ' + f(t.total?.cpu_hours_differential) + ' / ' + f(t.total?.mem_gb_hours_differential));
    lines.push('Clean passif (h): ' + f(t.total?.passive_clean_h) + ' | ratio dome actif: ' + f((t.total?.dome_active_ratio ?? 0) * 100) + '%');
    const lt = t.lifetime;
    if (lt) {
      lines.push('Vie entiere GREEN IT: CPU·h diff.=' + Number(lt.total_cpu_hours_differential || 0).toFixed(3) + ' | RAM·GB·h diff.=' + Number(lt.total_mem_gb_hours_differential || 0).toFixed(3));
      const idleH = Number(lt.total_idle_hours || 0);
      const mediaH = Number(lt.total_media_hours || 0);
      if (idleH > 0.01 || mediaH > 0.01) {
        lines.push('Periodes exclues: idle=' + idleH.toFixed(2) + 'h | media=' + mediaH.toFixed(2) + 'h');
      }
    }
  }
  lines.push('');
  lines.push('Mesure murale / export conso');
  lines.push('Source energie: ' + (meter.power_source || 'N/A') + (meter.is_external_wall_source ? ' (prise externe)' : ''));
  lines.push('Puissance live: ' + (meter.live_power_w == null ? 'N/A' : Number(meter.live_power_w).toFixed(2) + ' W'));
  lines.push('Conso totale integree: ' + (meter.windows?.total_kwh == null ? 'N/A' : Number(meter.windows.total_kwh).toFixed(6) + ' kWh'));
  lines.push('Cout total integre: ' + (meter.windows?.total_cost == null ? 'N/A' : Number(meter.windows.total_cost).toFixed(4) + ' ' + (meter.pricing?.currency || 'EUR')));
  lines.push('CO2 total integre: ' + (meter.windows?.total_co2_kg == null ? 'N/A' : Number(meter.windows.total_co2_kg).toFixed(6) + ' kg'));
  lines.push('Fenetres kWh H/J/S/M/A: ' + [
    meter.windows?.hour_kwh,
    meter.windows?.day_kwh,
    meter.windows?.week_kwh,
    meter.windows?.month_kwh,
    meter.windows?.year_kwh,
  ].map(v => v == null ? 'N/A' : Number(v).toFixed(6)).join('/'));
  if (meter.external_power) {
    lines.push('Etat prise externe: watts=' + (meter.external_power.last_watts_label || 'N/A') +
      ' | fraicheur=' + (meter.external_power.freshness || 'N/A') +
      ' | bridge=' + (meter.external_power.bridge_state || 'N/A') +
      ' | runtime=' + (meter.external_power.runtime || 'N/A'));
    lines.push('Fichier puissance: ' + (meter.external_power.power_file_path || 'N/A'));
  }
  lines.push('');
  lines.push('Impact processus (observe + attribution estimee)');
  lines.push('Processus suivis: ' + proc.process_count);
  lines.push('Cible courante: ' + (proc.selected_target?.target_label || 'N/A'));
  if (proc.overhead_audit) {
    lines.push(
      'Overhead SoulKernel+WebView: CPU=' + Number(proc.overhead_audit.combined_cpu_usage_pct || 0).toFixed(1) +
      '% | GPU=' + Number(proc.overhead_audit.combined_gpu_usage_pct || 0).toFixed(1) +
      '% | RAM=' + ((Number(proc.overhead_audit.combined_memory_kb || 0)) / 1024).toFixed(0) +
      ' MiB | W est.=' + (proc.overhead_audit.combined_estimated_power_w == null ? 'N/A' : Number(proc.overhead_audit.combined_estimated_power_w).toFixed(2))
    );
  }
  proc.top_contributors.slice(0, 10).forEach(p => {
    lines.push(
      `- ${p.name} PID=${p.pid} | CPU=${Number(p.cpu_usage || 0).toFixed(1)}% | GPU=${p.gpu_usage_pct == null ? 'N/A' : Number(p.gpu_usage_pct).toFixed(1) + '%'} | RAM=${((Number(p.memory_kb || 0)) / 1024).toFixed(0)} MiB | I/O=${Number((p.disk_read_bytes || 0) + (p.disk_written_bytes || 0)).toFixed(0)} B | impact est.=${p.impact_score_pct_estimated == null ? 'N/A' : Number(p.impact_score_pct_estimated).toFixed(2) + '%'} | W est.=${p.estimated_power_w == null ? 'N/A' : Number(p.estimated_power_w).toFixed(2)}`
    );
  });
  const logs = collectVisibleLogLines();
  lines.push('');
  lines.push('Logs (' + logs.length + ')');
  if (logs.length === 0) {
    lines.push('Aucun log');
  } else {
    logs.forEach(l => lines.push(l));
  }

  return lines.join('\n');
}

async function buildEvidencePackText() {
  const lines = [];
  const meter = collectEnergyMeterExport();
  const proc = collectProcessImpactExport();
  lines.push('SoulKernel — dossier de preuve (méthode courte)');
  lines.push('Généré: ' + new Date().toISOString());
  lines.push('');
  lines.push('=== Définitions (à citer tel quel) ===');
  lines.push('• Mesures OS : collecte native (sysinfo + APIs plateforme). Pas de valeurs inventées : champ absent = non disponible.');
  lines.push('• CPU·h / RAM·GB·h diff. : modèle différentiel (baseline dôme OFF vs échantillons dôme ON), fenêtre ~10 min, activité ACTIF uniquement.');
  lines.push('• ∫𝒟 (télémétrie) : Σ π·Δt avec π issu de la formule affichée (κ, Σmax, η, profil α).');
  lines.push('• kWh, kg CO₂, € : intégrale de la puissance (W) mesurée × tarifs saisis — empreinte du suivi, pas un « gain dôme » sans double mesure énergétique.');
  lines.push('• Benchmark A/B : alternance reproductible OFF/ON sur UNE commande KPI de votre choix ; export JSON des sessions.');
  lines.push('');
  lines.push('=== Mode preuve stricte ===');
  collectStrictEvidenceExport().allowed_claims.forEach(line => lines.push('• ' + line));
  collectStrictEvidenceExport().forbidden_claims.forEach(line => lines.push('• Limite: ' + line));
  lines.push('');
  lines.push('=== Mesure energetique exportable ===');
  lines.push('Source energie: ' + (meter.power_source || 'N/A') + (meter.is_external_wall_source ? ' (prise externe)' : ''));
  lines.push('Puissance live: ' + (meter.live_power_w == null ? 'N/A' : Number(meter.live_power_w).toFixed(2) + ' W'));
  lines.push('Conso totale integree: ' + (meter.windows?.total_kwh == null ? 'N/A' : Number(meter.windows.total_kwh).toFixed(6) + ' kWh'));
  lines.push('Cout total integre: ' + (meter.windows?.total_cost == null ? 'N/A' : Number(meter.windows.total_cost).toFixed(4) + ' ' + (meter.pricing?.currency || 'EUR')));
  lines.push('CO2 total integre: ' + (meter.windows?.total_co2_kg == null ? 'N/A' : Number(meter.windows.total_co2_kg).toFixed(6) + ' kg'));
  if (meter.external_power) {
    lines.push('Etat prise externe: ' + (meter.external_power.last_watts_label || 'N/A') + ' | ' + (meter.external_power.freshness || 'N/A') + ' | bridge=' + (meter.external_power.bridge_state || 'N/A'));
    lines.push('Fichier puissance: ' + (meter.external_power.power_file_path || 'N/A'));
  }
  lines.push('');
  lines.push('=== Processus observes / attribution estimee ===');
  lines.push('Methode: CPU/GPU/RAM/I/O sont lus par processus selon disponibilite plateforme; la part energetique par processus reste une estimation ponderee sur la puissance machine mesuree.');
  lines.push('Processus suivis: ' + proc.process_count);
  if (proc.overhead_audit) {
    lines.push(
      'Overhead SoulKernel+WebView: CPU=' + Number(proc.overhead_audit.combined_cpu_usage_pct || 0).toFixed(1) +
      '% | GPU=' + Number(proc.overhead_audit.combined_gpu_usage_pct || 0).toFixed(1) +
      '% | RAM=' + ((Number(proc.overhead_audit.combined_memory_kb || 0)) / 1024).toFixed(0) +
      ' MiB | W est.=' + (proc.overhead_audit.combined_estimated_power_w == null ? 'N/A' : Number(proc.overhead_audit.combined_estimated_power_w).toFixed(2))
    );
  }
  proc.top_contributors.slice(0, 12).forEach(p => {
    lines.push(
      `• ${p.name} (PID ${p.pid}) | CPU ${Number(p.cpu_usage || 0).toFixed(1)}% | GPU ${p.gpu_usage_pct == null ? 'N/A' : Number(p.gpu_usage_pct).toFixed(1) + '%'} | RAM ${((Number(p.memory_kb || 0)) / 1024).toFixed(0)} MiB | impact est. ${p.impact_score_pct_estimated == null ? 'N/A' : Number(p.impact_score_pct_estimated).toFixed(2) + '%'} | puissance est. ${p.estimated_power_w == null ? 'N/A' : Number(p.estimated_power_w).toFixed(2) + ' W'}`
    );
  });
  lines.push('');
  lines.push('=== Fichiers persistants (audit externe) ===');
  if (!hasTauri) {
    lines.push('(Lancer l’application Tauri pour obtenir les chemins réels sur cette machine.)');
    return lines.join('\n');
  }
  try {
    const p = await invoke('get_evidence_data_paths');
    lines.push('Échantillons télémetrie (JSONL) : ' + (p.telemetrySamplesJsonl || ''));
    lines.push('Cumul lifetime (JSON)     : ' + (p.lifetimeGainsJson || ''));
    lines.push('Tarif énergie (JSON)      : ' + (p.energyPricingJson || ''));
    lines.push('Sessions benchmark (JSONL): ' + (p.benchmarkSessionsJsonl || ''));
    lines.push('Journal d’audit (JSONL)   : ' + (p.auditLogJsonl || ''));
  } catch (e) {
    lines.push('Erreur lecture chemins : ' + e);
  }
  lines.push('');
  lines.push('Recommandation : joindre une exportation « Exporter JSON » (session) + une session benchmark + ces chemins pour revue indépendante.');
  return lines.join('\n');
}

/** Suite des addEventListener (après les helpers DOM, pour ne pas les imbriquer dans wireLegacyDomListeners). */
function wireLegacyDomMore() {
const btnCopyEvidence = document.getElementById('btnCopyEvidence');
if (btnCopyEvidence) {
  btnCopyEvidence.addEventListener('click', async () => {
    try {
      const txt = await buildEvidencePackText();
      await navigator.clipboard.writeText(txt);
      log('Preuve (méthode + chemins fichiers) copiée dans le presse-papier', 'ok');
    } catch (e) {
      log('Copie preuve échouée : ' + e, 'err');
    }
  });
}

document.getElementById('btnCopyGains').addEventListener('click', () => {
  const diag = benchmarkDiagPayload({
    export_mode: 'clipboard',
    has_snapshot_before: !!state.snapshotBefore,
    last_benchmark_gain_median_pct: state.kpiBench.lastSummary?.gain_median_pct ?? null,
    last_benchmark_gain_p95_pct: state.kpiBench.lastSummary?.gain_p95_pct ?? null,
  });
  log('EXPORT DIAG clipboard ' + JSON.stringify(diag), 'info');
  const report = buildSessionReportText();
  navigator.clipboard.writeText(report).then(() => {
    log('Rapport complet copie dans le presse-papier', 'ok');
  }).catch(() => log('Copie echouee', 'err'));
});

const btnExportEnergyHour = document.getElementById('btnExportEnergyHour');
if (btnExportEnergyHour) {
  btnExportEnergyHour.addEventListener('click', async () => {
    try {
      const path = await exportEnergyPeriodReport('hour');
      log('Export energie heure enregistre : ' + path, 'ok');
    } catch (e) {
      if (String(e).includes('Annulé')) return;
      log('Export energie heure: ' + e, 'err');
    }
  });
}
const btnExportEnergyDay = document.getElementById('btnExportEnergyDay');
if (btnExportEnergyDay) {
  btnExportEnergyDay.addEventListener('click', async () => {
    try {
      const path = await exportEnergyPeriodReport('day');
      log('Export energie jour enregistre : ' + path, 'ok');
    } catch (e) {
      if (String(e).includes('Annulé')) return;
      log('Export energie jour: ' + e, 'err');
    }
  });
}
const btnExportEnergyWeek = document.getElementById('btnExportEnergyWeek');
if (btnExportEnergyWeek) {
  btnExportEnergyWeek.addEventListener('click', async () => {
    try {
      const path = await exportEnergyPeriodReport('week');
      log('Export energie semaine enregistre : ' + path, 'ok');
    } catch (e) {
      if (String(e).includes('Annulé')) return;
      log('Export energie semaine: ' + e, 'err');
    }
  });
}
const btnExportEnergyMonth = document.getElementById('btnExportEnergyMonth');
if (btnExportEnergyMonth) {
  btnExportEnergyMonth.addEventListener('click', async () => {
    try {
      const path = await exportEnergyPeriodReport('month');
      log('Export energie mois enregistre : ' + path, 'ok');
    } catch (e) {
      if (String(e).includes('Annulé')) return;
      log('Export energie mois: ' + e, 'err');
    }
  });
}

document.getElementById('btnExportGains').addEventListener('click', async () => {
  try {
    let snapshotBefore = state.snapshotBefore;
    if (state.domeActive && !snapshotBefore) {
      try { snapshotBefore = await invoke('get_snapshot_before_dome') || null; } catch (_) {}
    }
    const diag = benchmarkDiagPayload({
      export_mode: 'file',
      has_snapshot_before: !!snapshotBefore,
      last_benchmark_gain_median_pct: state.kpiBench.lastSummary?.gain_median_pct ?? null,
      last_benchmark_gain_p95_pct: state.kpiBench.lastSummary?.gain_p95_pct ?? null,
    });
    log('EXPORT DIAG file ' + JSON.stringify(diag), 'info');
    const payload = {
      exported_at: new Date().toISOString(),
      product: 'SoulKernel',
      version: '1.1.7',
      dome_active: state.domeActive,
      machine_activity: state.machineActivity || 'active',
      dome_real_integral: state.domeActive ? state.domeRealIntegral : null,
      dome_actions_ok: state.domeActionsOk,
      dome_actions_total: state.domeActionsTotal,
      snapshot_before: snapshotBefore,
      history: state.domeHistory,
      adaptive: { enabled: state.adaptiveEnabled, auto_dome: state.adaptiveAutoDome },
      audit_log_path: state.auditLogPath,
      kpi_bench_sessions: state.kpiBench.sessions,
      telemetry_summary: state.telemetrySummary,
      energy_meter_export: collectEnergyMeterExport(),
      strict_evidence: collectStrictEvidenceExport(),
      process_impact_report: collectProcessImpactExport(),
      diagnostic: diag,
      session_summary: state.domeHistory.length ? {
        count: state.domeHistory.length,
        avg_dome_gain: state.domeHistory.reduce((s, e) => s + e.domeGain, 0) / state.domeHistory.length,
      } : null,
    };
    try {
      payload.evidence_data_paths = await invoke('get_evidence_data_paths');
    } catch (_) {
      payload.evidence_data_paths = null;
    }
    const path = await invoke('export_gains_to_file', { content: JSON.stringify(payload, null, 2) });
    log('Export enregistré : ' + path, 'ok');
  } catch (e) {
    if (String(e).includes('Annulé')) return;
    log('Export : ' + e, 'err');
  }
});

document.getElementById('btnRefreshProcesses').addEventListener('click', () => {
  refreshProcesses({ userInitiated: true });
});

document.getElementById('autoProcessTarget').addEventListener('change', e => {
  state.autoProcessTarget = !!e.target.checked;
  log(state.autoProcessTarget ? 'Auto-cible active' : 'Auto-cible desactivee', 'info');
  refreshProcesses({ userInitiated: false });
});

document.getElementById('targetProcess').addEventListener('change', e => {
  state.targetPid = e.target.value === '' ? null : parseInt(e.target.value, 10);
  const label = e.target.selectedIndex >= 0 ? (e.target.options[e.target.selectedIndex].text || '') : 'Ce processus (SoulKernel)';
  logStateDiagTarget({
    source: 'manual-target-change',
    next_target_pid: state.targetPid,
    next_target_label: label || 'Ce processus (SoulKernel)',
    auto_process_target: !!state.autoProcessTarget,
    workload: state.wl,
    dome_history_len: state.domeHistory.length,
  });
  if (state.autoProcessTarget && e.target.value) {
    state.autoProcessTarget = false;
    document.getElementById('autoProcessTarget').checked = false;
    log('Selection manuelle detectee: auto-cible desactivee', 'info');
  }
  renderProcessImpactPanel();
});


const policyModeSel = document.getElementById('policyMode');
if (policyModeSel) {
  policyModeSel.addEventListener('change', async e => {
    const next = (e.target.value === 'safe') ? 'safe' : 'privileged';
    state.policyMode = next;
    saveRuntimeSettings();
    saveStartupIntent();
    try {
      const applied = await invoke('set_policy_mode', { mode: next });
      state.policyMode = applied || next;
      await loadPolicyStatus();
      log('Policy moteur -> ' + state.policyMode, 'info');
    } catch (err) {
      log('Policy set error: ' + err, 'warn');
    }
  });
}

const btnApplyAdvice = document.getElementById('btnApplyAdvice');
if (btnApplyAdvice) {
  btnApplyAdvice.addEventListener('click', () => {
    if (!state.pendingAdvice) {
      log('Aucun ajustement recommande pour l instant', 'warn');
      return;
    }
    state.kappa = state.pendingAdvice.kappa;
    state.sigmaMax = state.pendingAdvice.sigmaMax;
    state.eta = state.pendingAdvice.eta;
    setSlidersFromState();
    if (state.lastMetrics) renderFormula(state.lastMetrics);
    log('Reglage recommande applique', 'ok');
    state.adviceLastAcceptedTs = Date.now();
    state.pendingAdvice = null;
    state.adviceCandidateKey = null;
    state.adviceCandidateCount = 0;
    state.adviceCurrentKey = null;
    saveStartupIntent();
  });
}
const adaptiveEnabledEl = document.getElementById('adaptiveEnabled');
if (adaptiveEnabledEl) {
  adaptiveEnabledEl.checked = !!state.adaptiveEnabled;
  adaptiveEnabledEl.addEventListener('change', e => {
    state.adaptiveEnabled = !!e.target.checked;
    updateAdaptiveStatusText(state.adaptiveEnabled ? 'warming up...' : 'OFF');
    log('Adaptive mode ' + (state.adaptiveEnabled ? 'active' : 'desactive'), 'info');
    saveRuntimeSettings();
    saveStartupIntent();
  });
}

const adaptiveAutoDomeEl = document.getElementById('adaptiveAutoDome');
if (adaptiveAutoDomeEl) {
  adaptiveAutoDomeEl.checked = !!state.adaptiveAutoDome;
  adaptiveAutoDomeEl.addEventListener('change', e => {
    state.adaptiveAutoDome = !!e.target.checked;
    log('Adaptive auto-dome ' + (state.adaptiveAutoDome ? 'on' : 'off'), 'info');
    saveRuntimeSettings();
    saveStartupIntent();
  });
}

const btnRunAB = document.getElementById('btnRunAB');
if (btnRunAB) {
  btnRunAB.addEventListener('click', () => {
    runAbBenchmark();
  });
}

const btnExportAB = document.getElementById('btnExportAB');
if (btnExportAB) {
  btnExportAB.addEventListener('click', async () => {
    try {
      const command = (document.getElementById('kpiCommand')?.value || '').trim();
      const args = tokenizeArgs(document.getElementById('kpiArgs')?.value || '');
      const payload = await invoke('get_benchmark_history', {
        query: {
          command: command || null,
          args,
          cwd: null,
          workload: state.wl,
        },
      });
      const path = await invoke('export_benchmark_to_file', {
        content: JSON.stringify({
          exported_at: new Date().toISOString(),
          product: 'SoulKernel',
          workload: state.wl,
          command,
          args,
          current_params: {
            kappa: state.kappa,
            sigma_max: state.sigmaMax,
            eta: state.eta,
            policy_mode: state.policyMode,
            target_pid: state.targetPid,
          },
          benchmark_history: payload,
          benchmark_top: payload?.top_sessions || [],
          telemetry_summary: state.telemetrySummary,
          energy_meter_export: collectEnergyMeterExport(),
          strict_evidence: collectStrictEvidenceExport(),
          process_impact_report: collectProcessImpactExport(),
        }, null, 2),
      });
      log('Export benchmark enregistre : ' + path, 'ok');
    } catch (e) {
      if (String(e).includes('Annule')) return;
      log('Export benchmark: ' + e, 'err');
    }
  });
}

const btnClearAB = document.getElementById('btnClearAB');
if (btnClearAB) {
  btnClearAB.addEventListener('click', async () => {
    try {
      await invoke('clear_benchmark_history');
      state.kpiBench.sessions = [];
      state.kpiBench.lastSummary = null;
      state.kpiBench.tuningAdvice = null;
      state.kpiBench.topSessions = [];
      renderAbSummary(null);
      renderBenchmarkLearning(null);
      renderBenchmarkTop([]);
      log('A/B history reset', 'info');
    } catch (e) {
      log('A/B history reset error: ' + e, 'err');
    }
  });
}
const kpiCommandEl = document.getElementById('kpiCommand');
if (kpiCommandEl) {
  kpiCommandEl.addEventListener('change', () => {
    loadBenchmarkHistory(true).catch(() => {});
    saveRuntimeSettings();
  });
}
const kpiArgsEl = document.getElementById('kpiArgs');
if (kpiArgsEl) {
  kpiArgsEl.addEventListener('change', () => {
    loadBenchmarkHistory(true).catch(() => {});
    saveRuntimeSettings();
  });
}
const soulRamPct = document.getElementById('soulRamPct');
if (soulRamPct) {
  soulRamPct.addEventListener('input', e => {
    state.soulRamPercent = parseInt(e.target.value, 10) || 20;
    updateSoulRamUi();
  });
}



const btnApplyEnergyPricing = document.getElementById('btnApplyEnergyPricing');
if (btnApplyEnergyPricing) {
  btnApplyEnergyPricing.addEventListener('click', async () => {
    const status = document.getElementById('energyPricingStatus');
    const normNum = v => String(v ?? '').trim().replace(',', '.');
    const p = parseFloat(normNum(document.getElementById('energyPrice')?.value || '0.22'));
    const co2 = parseFloat(normNum(document.getElementById('energyCo2')?.value || '0.05'));
    if (!Number.isFinite(p) || p < 0 || !Number.isFinite(co2) || co2 < 0) {
      if (status) {
        status.textContent = 'Tarif invalide: utiliser des nombres >= 0 (ex: 0,194 et 0,024).';
        status.style.color = 'var(--stress)';
      }
      log('Tarif energie invalide', 'warn');
      return;
    }
    try {
      await setEnergyPricing(p, 'EUR', co2);
      log('Tarif energie applique: ' + p.toFixed(3) + ' EUR/kWh | CO2 ' + co2.toFixed(3) + ' kg/kWh', 'ok');
    } catch (e) {
      if (status) {
        status.textContent = 'Erreur enregistrement tarif energie';
        status.style.color = 'var(--stress)';
      }
      log('Tarif energie error: ' + e, 'err');
    }
  });
}
const btnApplyMerossConfig = document.getElementById('btnApplyMerossConfig');
if (btnApplyMerossConfig) {
  btnApplyMerossConfig.addEventListener('click', async () => {
    try {
      await applyExternalPowerConfig();
      log('Configuration prise externe enregistrée', 'ok');
    } catch (e) {
      const info = document.getElementById('merossConfigStatus');
      if (info) {
        info.textContent = 'Erreur enregistrement prise externe';
        info.style.color = 'var(--stress)';
      }
      log('Prise externe error: ' + e, 'err');
    }
  });
}
const btnRefreshMerossStatus = document.getElementById('btnRefreshMerossStatus');
if (btnRefreshMerossStatus) {
  btnRefreshMerossStatus.addEventListener('click', async () => {
    try {
      await refreshExternalPowerStatus();
      await refreshExternalBridgeStatus();
      log('État prise externe rafraîchi', 'info');
    } catch (e) {
      log('Prise externe refresh: ' + e, 'err');
    }
  });
}
const btnStartMerossBridge = document.getElementById('btnStartMerossBridge');
if (btnStartMerossBridge) {
  btnStartMerossBridge.addEventListener('click', async () => {
    try {
      await startExternalBridge();
      await refreshExternalPowerStatus();
      log('Bridge Meross démarré', 'ok');
    } catch (e) {
      log('Bridge Meross start: ' + e, 'err');
    }
  });
}
const btnStopMerossBridge = document.getElementById('btnStopMerossBridge');
if (btnStopMerossBridge) {
  btnStopMerossBridge.addEventListener('click', async () => {
    try {
      await stopExternalBridge();
      log('Bridge Meross arrêté', 'info');
    } catch (e) {
      log('Bridge Meross stop: ' + e, 'err');
    }
  });
}
const btnSoulRamOn = document.getElementById('btnSoulRamOn');
if (btnSoulRamOn) {
  btnSoulRamOn.addEventListener('click', async () => {
    const requestedPct = parseInt(document.getElementById('soulRamPct')?.value, 10) || state.soulRamPercent || 20;
    state.soulRamPercent = requestedPct;
    updateSoulRamUi();
    btnSoulRamOn.disabled = true;
    try {
      const actions = await invoke('set_soulram', { enabled: true, percent: requestedPct });
      await syncSoulRamStatus();
      updateSoulRamUi();
      actions.forEach(a => log(a, a.startsWith('[ok]') ? 'ok' : 'warn'));
      await loadPolicyStatus();
      markSoulRamRebootRequirement(actions, 'SoulRAM');
      if (state.soulRamActive) {
        log('SoulRAM actif (' + requestedPct + '%)', 'ok');
      } else {
        log('SoulRAM: aucun effet effectif — sous Windows, lancez SoulKernel en administrateur pour activer la compression memoire (ou attendez la fin du cooldown trim).', 'warn');
      }
    saveStartupIntent();
    } catch (e) {
      log('SoulRAM on: ' + e, 'err');
    } finally {
      updateSoulRamUi();
    }
  });
}

const btnSoulRamOff = document.getElementById('btnSoulRamOff');
if (btnSoulRamOff) {
  btnSoulRamOff.addEventListener('click', async () => {
    try {
      const actions = await invoke('set_soulram', { enabled: false, percent: null });
      await syncSoulRamStatus();
      updateSoulRamUi();
      actions.forEach(a => log(a, a.startsWith('[ok]') ? 'ok' : 'warn'));
      log('SoulRAM desactive', 'ok');
    saveStartupIntent();
    } catch (e) {
      log(`SoulRAM off: ${e}`, 'err');
    }
  });
}






const btnViewCompact = document.getElementById('btnViewCompact');
if (btnViewCompact) {
  btnViewCompact.addEventListener('click', () => setViewMode('compact'));
}
const btnViewDetailed = document.getElementById('btnViewDetailed');
if (btnViewDetailed) {
  btnViewDetailed.addEventListener('click', () => setViewMode('detailed'));
}
const btnViewBenchmark = document.getElementById('btnViewBenchmark');
if (btnViewBenchmark) {
  btnViewBenchmark.addEventListener('click', () => setViewMode('benchmark'));
}
const btnViewExternal = document.getElementById('btnViewExternal');
if (btnViewExternal) {
  btnViewExternal.addEventListener('click', () => setViewMode('external'));
}
const btnHudToggle = document.getElementById('btnHudToggle');
if (btnHudToggle) {
  btnHudToggle.addEventListener('click', () => setHudVisible(!state.hudVisible));
}
const btnHudEdit = document.getElementById('btnHudEdit');
if (btnHudEdit) {
  btnHudEdit.addEventListener('click', () => setHudInteractive(!state.hudInteractive));
}
const hudPresetEl = document.getElementById('hudPreset');
if (hudPresetEl) {
  hudPresetEl.addEventListener('change', e => {
    state.hudPreset = e.target.value;
    applyHudPresentation();
  });
}
const hudOpacityEl = document.getElementById('hudOpacity');
if (hudOpacityEl) {
  hudOpacityEl.addEventListener('input', e => {
    state.hudOpacity = Math.max(0.3, Math.min(1.0, parseFloat(e.target.value) || 0.82));
    applyHudPresentation();
  });
}
const hudDisplayEl = document.getElementById('hudDisplay');
if (hudDisplayEl) {
  hudDisplayEl.addEventListener('change', async e => {
    const raw = e.target.value;
    state.hudDisplayIndex = (raw === '' ? null : Number(raw));
    if (hasTauri && state.hudVisible) {
      try { await invoke('set_system_hud_display', { display_index: state.hudDisplayIndex }); } catch (_) {}
    }
    try { await updateHudActiveResHint(); } catch (_) {}
    saveRuntimeSettings();
  });
}
// ─── Sliders ──────────────────────────────────────────────────────────────────
const wlGridEl = document.getElementById('wlGrid');
if (wlGridEl) {
  wlGridEl.addEventListener('click', e => {
    const btn = e.target.closest('.wl-btn');
    if (!btn) return;
    setWorkload(btn.dataset.wl, 'manuel');
  });
}
const wlSelectEl = document.getElementById('wlSelect');
if (wlSelectEl) {
  wlSelectEl.addEventListener('change', e => {
    const v = e.target && e.target.value;
    if (v) setWorkload(v, 'manuel');
  });
}

document.addEventListener('click', e => {
  const el = e.target.closest('button, .wl-btn, input[type="checkbox"]');
  if (!el) return;
  const id = el.id || null;
  const txt = truncateAuditString((el.textContent || '').trim(), 120);
  auditEmit('interaction', 'click', 'info', { id, text: txt, class: el.className || null });
});

if (window.__TAURI__?.event?.listen) {
  window.__TAURI__.event.listen('soulkernel://benchmark-progress', ev => {
    try {
      updateBenchmarkProgressUi(ev.payload);
    } catch (_) {}
  }).catch(() => {});
}
if (!HUD_ONLY && window.__TAURI__?.event?.listen) {
  window.__TAURI__.event.listen('soulkernel://hud-state', ev => {
    state.hudVisible = !!ev.payload;
    const hud = document.getElementById('compactHud');
    if (hud) hud.classList.toggle('visible', HUD_ONLY && state.hudVisible);
    const bHud = document.getElementById('btnHudToggle');
    if (bHud) {
      bHud.classList.toggle('active', state.hudVisible);
      bHud.innerHTML = '<span class="view-ico"><i data-lucide="panel-top"></i></span> ' + (state.hudVisible ? 'HUD ON' : 'HUD OFF');
      refreshSoulKernelLucide();
    }
    saveRuntimeSettings();
  }).catch(e => log('HUD event.listen blocked: ' + e, 'warn'));
  window.__TAURI__.event.listen('soulkernel://hud-interactive', ev => {
    setHudInteractive(!!ev.payload);
  }).catch(e => log('HUD event.listen blocked: ' + e, 'warn'));
}
document.addEventListener('change', e => {
  const el = e.target;
  if (!(el instanceof HTMLInputElement || el instanceof HTMLSelectElement || el instanceof HTMLTextAreaElement)) return;
  const id = el.id || null;
  let value = null;
  if (el instanceof HTMLInputElement && el.type === 'checkbox') value = !!el.checked;
  else value = truncateAuditString(String(el.value ?? ''), 200);
  auditEmit('interaction', 'change', 'info', { id, tag: el.tagName.toLowerCase(), value });
});
document.getElementById('kappaSlider').addEventListener('input', e => {
  state.kappa = parseFloat(e.target.value);
  document.getElementById('kappaNum').textContent = state.kappa.toFixed(1);
});
document.getElementById('sigmaMaxSlider').addEventListener('input', e => {
  state.sigmaMax = parseFloat(e.target.value);
  document.getElementById('sigmaMaxNum').textContent = state.sigmaMax.toFixed(2);
  document.getElementById('smaxBot').textContent = state.sigmaMax.toFixed(2);
});
document.getElementById('etaSlider').addEventListener('input', e => {
  state.eta = parseFloat(e.target.value);
  document.getElementById('etaNum').textContent = state.eta.toFixed(2);
});
}

async function setHudVisible(on) {
  state.hudVisible = !!on;
  const hud = document.getElementById('compactHud');
  if (hud) hud.classList.toggle('visible', HUD_ONLY && state.hudVisible);
  const bHud = document.getElementById('btnHudToggle');
  if (bHud) {
    bHud.classList.toggle('active', state.hudVisible);
    bHud.innerHTML = '<span class="view-ico"><i data-lucide="panel-top"></i></span> ' + (state.hudVisible ? 'HUD ON' : 'HUD OFF');
    refreshSoulKernelLucide();
  }

  if (!HUD_ONLY && hasTauri) {
    try {
      if (state.hudVisible) {
        await applyHudPresentation();
        await invoke('open_system_hud');
        await invoke('set_system_hud_display', { display_index: state.hudDisplayIndex });
      }
      else await invoke('close_system_hud');
    } catch (e) {
      log('HUD overlay: ' + e, 'warn');
    }
  } else if (!HUD_ONLY && !hasTauri) {
    log('HUD indisponible: runtime Tauri non detecte', 'warn');
  }

  if (state.hudVisible && state.lastMetrics) renderCompactHud(state.lastMetrics);
  saveRuntimeSettings();
}

let lastHudPushTs = 0;

async function pushSystemHudData(payload) {
  if (HUD_ONLY || !hasTauri || !state.hudVisible) return;
  const now = Date.now();
  if (now - lastHudPushTs < 220) return;
  lastHudPushTs = now;
  try { await invoke('set_system_hud_data', { payload }); } catch (_) {}
}

function applyHudPayload(payload) {
  if (!payload) return;
  set('hudDome', payload.dome ?? 'N/A');
  set('hudSigma', payload.sigma ?? 'N/A');
  set('hudPi', payload.pi ?? 'N/A');
  set('hudCpu', payload.cpu ?? 'N/A');
  set('hudRam', payload.ram ?? 'N/A');
  set('hudIo', payload.io ?? 'N/A');
  set('hudGpu', payload.gpu ?? 'N/A');
  set('hudWl', payload.workload ? payload.workload.toUpperCase() : 'N/A');
  set('hudTarget', payload.target ?? 'N/A');
  set('hudDreal', payload.d_real ?? '—');
  set('hudRenta', payload.rentable ?? '—');
  set('hudPower', payload.power ?? 'N/A');
  set('hudEnergy', payload.energy ?? 'N/A');
  const actEl = document.getElementById('hudActivity');
  if (actEl) {
    actEl.textContent = payload.activity ?? 'ACTIF';
    actEl.style.color = (payload.activity === 'ACTIF') ? 'var(--io)' : 'var(--gpu)';
  }
}

async function initHudOnlyMode() {
  document.body.classList.add('hud-only');
  const hud = document.getElementById('compactHud');
  if (hud) {
    hud.classList.add('visible');
    applyHudPresetClass(hud, state.hudPreset);
  }
  const drag = document.getElementById('hudDrag');
  const t = window.__TAURI__;
  if (drag && t?.window?.getCurrentWindow) {
    drag.addEventListener('mousedown', async (ev) => {
      if (!state.hudInteractive) return;
      if (ev.button !== 0) return;
      try { await t.window.getCurrentWindow().startDragging(); } catch (_) {}
    });
  }
  if (t?.event?.listen) {
    try {
      await t.event.listen('soulkernel://hud', (ev) => applyHudPayload(ev.payload || {}));
    } catch (e) {
      log('HUD event.listen blocked: ' + e, 'warn');
    }
    try {
      await t.event.listen('soulkernel://hud-config', (ev) => {
        const cfg = ev.payload || {};
        if (cfg.preset) state.hudPreset = cfg.preset;
        if (typeof cfg.interactive === 'boolean') state.hudInteractive = cfg.interactive;
        if (typeof cfg.opacity === 'number') state.hudOpacity = cfg.opacity;
        document.body.classList.toggle('hud-edit', !!state.hudInteractive);
        const mode = document.getElementById('hudModeBadge');
        if (mode) mode.textContent = state.hudInteractive ? 'EDIT' : 'LOCK';
        applyHudPresetClass(document.getElementById('compactHud'), state.hudPreset);
      });
    } catch (e) {
      log('HUD event.listen blocked: ' + e, 'warn');
    }
  }
}
function applyHudPresetClass(target, preset) {
  if (!target) return;
  target.classList.remove('hud-mini', 'hud-compact', 'hud-detailed');
  target.classList.add(preset === 'mini' ? 'hud-mini' : (preset === 'detailed' ? 'hud-detailed' : 'hud-compact'));
}

function buildHudPresentationPayload() {
  const vm = (state.hudVisibleMetrics && state.hudVisibleMetrics.length)
    ? state.hudVisibleMetrics.slice()
    : HUD_METRIC_DEFAULTS.slice();
  return {
    preset: state.hudPreset,
    opacity: Number(state.hudOpacity || 0.82),
    size_mode: state.hudSizeMode || 'screen',
    screen_width_pct: Number(state.hudScreenWidthPct) || 22,
    screen_height_pct: Number(state.hudScreenHeightPct) || 28,
    manual_width: Number(state.hudManualWidth) || 420,
    manual_height: Number(state.hudManualHeight) || 260,
    visible_metrics: vm,
  };
}

function readHudVisibleMetricsFromDom() {
  const cbs = document.querySelectorAll('.hud-metric-cb');
  const out = [];
  cbs.forEach((cb) => {
    if (cb.checked) out.push(cb.dataset.metric);
  });
  return out.length ? out : HUD_METRIC_DEFAULTS.slice();
}

function syncHudSizePanels() {
  const mode = state.hudSizeMode || 'screen';
  const screenEl = document.getElementById('hudSizeScreen');
  const manualEl = document.getElementById('hudSizeManual');
  if (screenEl) screenEl.style.display = mode === 'screen' ? '' : 'none';
  if (manualEl) manualEl.style.display = mode === 'manual' ? '' : 'none';
}

function syncHudPanelFromState() {
  const sm = document.getElementById('hudSizeMode');
  if (sm) sm.value = state.hudSizeMode || 'screen';
  const sw = document.getElementById('hudScreenW');
  const swn = document.getElementById('hudScreenWNum');
  const wv = Math.max(8, Math.min(50, Number(state.hudScreenWidthPct) || 22));
  if (sw) sw.value = String(wv);
  if (swn) swn.textContent = wv + '%';
  const sh = document.getElementById('hudScreenH');
  const shn = document.getElementById('hudScreenHNum');
  const hv = Math.max(8, Math.min(50, Number(state.hudScreenHeightPct) || 28));
  if (sh) sh.value = String(hv);
  if (shn) shn.textContent = hv + '%';
  const mw = document.getElementById('hudManW');
  const mh = document.getElementById('hudManH');
  if (mw) mw.value = String(Math.max(240, Math.min(1600, Number(state.hudManualWidth) || 420)));
  if (mh) mh.value = String(Math.max(120, Math.min(1200, Number(state.hudManualHeight) || 260)));
  const set = new Set(state.hudVisibleMetrics && state.hudVisibleMetrics.length ? state.hudVisibleMetrics : HUD_METRIC_DEFAULTS);
  document.querySelectorAll('.hud-metric-cb').forEach((cb) => {
    cb.checked = set.has(cb.dataset.metric);
  });
  syncHudSizePanels();
}

async function updateHudActiveResHint() {
  const hint = document.getElementById('hudActiveResHint');
  if (!hint || !hasTauri) return;
  try {
    const list = await invoke('list_displays');
    if (!list || !list.length) {
      hint.textContent = 'Ecran actif : —';
      return;
    }
    let d = null;
    if (state.hudDisplayIndex != null) d = list.find(x => x.index === state.hudDisplayIndex);
    if (!d) d = list.find(x => x.is_primary) || list[0];
    if (!d) return;
    const sf = (d.scale_factor != null) ? Number(d.scale_factor).toFixed(2) : '?';
    const wp = Math.round(Number(state.hudScreenWidthPct) || 22);
    const hp = Math.round(Number(state.hudScreenHeightPct) || 28);
    hint.textContent = 'Ecran ref. : ' + d.width + '×' + d.height + ' px, echelle ' + sf + ' — fenetre ~ ' + wp + '% × ' + hp + '% de cette surface (mode % ecran).';
  } catch (_) {}
}

function wireHudLayoutControls() {
  const sm = document.getElementById('hudSizeMode');
  if (sm) {
    sm.addEventListener('change', () => {
      state.hudSizeMode = sm.value === 'content' || sm.value === 'manual' ? sm.value : 'screen';
      syncHudSizePanels();
      applyHudPresentation();
    });
  }
  const sw = document.getElementById('hudScreenW');
  const swn = document.getElementById('hudScreenWNum');
  if (sw && swn) {
    sw.addEventListener('input', () => {
      state.hudScreenWidthPct = Math.max(8, Math.min(50, parseInt(sw.value, 10) || 22));
      swn.textContent = state.hudScreenWidthPct + '%';
    });
    sw.addEventListener('change', () => applyHudPresentation());
  }
  const sh = document.getElementById('hudScreenH');
  const shn = document.getElementById('hudScreenHNum');
  if (sh && shn) {
    sh.addEventListener('input', () => {
      state.hudScreenHeightPct = Math.max(8, Math.min(50, parseInt(sh.value, 10) || 28));
      shn.textContent = state.hudScreenHeightPct + '%';
    });
    sh.addEventListener('change', () => applyHudPresentation());
  }
  const mw = document.getElementById('hudManW');
  const mh = document.getElementById('hudManH');
  if (mw) mw.addEventListener('change', () => {
    state.hudManualWidth = Math.max(240, Math.min(1600, parseInt(mw.value, 10) || 420));
    applyHudPresentation();
  });
  if (mh) mh.addEventListener('change', () => {
    state.hudManualHeight = Math.max(120, Math.min(1200, parseInt(mh.value, 10) || 260));
    applyHudPresentation();
  });
  document.querySelectorAll('.hud-metric-cb').forEach((cb) => {
    cb.addEventListener('change', () => {
      state.hudVisibleMetrics = readHudVisibleMetricsFromDom();
      applyHudPresentation();
    });
  });
}

async function refreshHudDisplays() {
  const sel = document.getElementById('hudDisplay');
  if (!sel) return;
  try {
    const list = await invoke('list_displays');
    const prev = state.hudDisplayIndex;
    sel.innerHTML = '<option value="">Auto</option>';
    (list || []).forEach(d => {
      const o = document.createElement('option');
      o.value = String(d.index);
      const primary = d.is_primary ? ' *' : '';
      o.textContent = d.name + ' (' + d.width + 'x' + d.height + ')' + primary;
      sel.appendChild(o);
    });
    sel.value = (prev == null ? '' : String(prev));
    if (sel.value === '' && prev != null) state.hudDisplayIndex = null;
    await updateHudActiveResHint();
  } catch (_) {}
}
async function applyHudPresentation() {
  state.hudPreset = (state.hudPreset === 'mini' || state.hudPreset === 'detailed') ? state.hudPreset : 'compact';
  state.hudOpacity = Math.max(0.3, Math.min(1.0, Number(state.hudOpacity || 0.82)));
  if (document.querySelectorAll('.hud-metric-cb').length) {
    state.hudVisibleMetrics = readHudVisibleMetricsFromDom();
  }

  const h = document.getElementById('compactHud');
  applyHudPresetClass(h, state.hudPreset);
  if (h && !HUD_ONLY) h.style.opacity = String(state.hudOpacity);

  const p = document.getElementById('hudPreset'); if (p) p.value = state.hudPreset;
  const o = document.getElementById('hudOpacity'); if (o) o.value = String(state.hudOpacity.toFixed(2));
  const on = document.getElementById('hudOpacityNum'); if (on) on.textContent = Math.round(state.hudOpacity * 100) + '%';

  if (!HUD_ONLY && hasTauri && state.hudVisible) {
    try {
      await invoke('set_system_hud_presentation', { payload: buildHudPresentationPayload() });
    } catch (_) {}
  }
  saveRuntimeSettings();
}

async function setHudInteractive(on) {
  state.hudInteractive = !!on;
  document.body.classList.toggle('hud-edit', state.hudInteractive);
  const m = document.getElementById('hudModeBadge');
  if (m) m.textContent = state.hudInteractive ? 'EDIT' : 'LOCK';
  const b = document.getElementById('btnHudEdit');
  if (b) {
    b.innerHTML = '<span class="btn-ico"><i data-lucide="mouse-pointer-2"></i></span> ' + (state.hudInteractive ? 'Edition ON' : 'Edition OFF');
    b.classList.toggle('active', state.hudInteractive);
    refreshSoulKernelLucide();
  }
  if (!HUD_ONLY && hasTauri && state.hudVisible) {
    try { await invoke('set_system_hud_interactive', { interactive: state.hudInteractive }); } catch (_) {}
  }
  saveRuntimeSettings();
}
function setViewMode(mode) {
  state.viewMode = (mode === 'compact' || mode === 'benchmark' || mode === 'external') ? mode : 'detailed';
  document.body.classList.toggle('view-compact', state.viewMode === 'compact');
  document.body.classList.toggle('view-detailed', state.viewMode === 'detailed');
  document.body.classList.toggle('view-benchmark', state.viewMode === 'benchmark');
  document.body.classList.toggle('view-external', state.viewMode === 'external');

  const bCompact = document.getElementById('btnViewCompact');
  const bDetailed = document.getElementById('btnViewDetailed');
  const bBenchmark = document.getElementById('btnViewBenchmark');
  const bExternal = document.getElementById('btnViewExternal');
  if (bCompact) bCompact.classList.toggle('active', state.viewMode === 'compact');
  if (bDetailed) bDetailed.classList.toggle('active', state.viewMode === 'detailed');
  if (bBenchmark) bBenchmark.classList.toggle('active', state.viewMode === 'benchmark');
  if (bExternal) bExternal.classList.toggle('active', state.viewMode === 'external');

  if (state.lastMetrics) renderCompactHud(state.lastMetrics);
  saveRuntimeSettings();
}

function renderCompactHud(m) {
  if (!HUD_ONLY && !state.hudVisible) return;
  if (!m || !m.raw) return;
  const t = state.telemetrySummary || null;
  const ccy = (t && t.pricing && t.pricing.currency) ? t.pricing.currency : 'EUR';
  const targetSel = document.getElementById('targetProcess');
  const targetTxt = (targetSel && targetSel.selectedIndex >= 0)
    ? (targetSel.options[targetSel.selectedIndex].text || 'Auto')
    : 'Auto';

  set('hudDome', state.domeActive ? 'ACTIF' : 'IDLE');
  const act = state.machineActivity || 'active';
  const actEl = document.getElementById('hudActivity');
  if (actEl) {
    actEl.textContent = act === 'idle' ? 'IDLE' : (act === 'media' ? 'MEDIA' : 'ACTIF');
    actEl.style.color = act === 'active' ? 'var(--io)' : 'var(--gpu)';
  }
  const sigmaTxt = Number(m.sigma || 0).toFixed(3);
  const piTxt = state.lastPi == null ? 'N/A' : Number(state.lastPi).toFixed(4);
  const cpuTxt = Number(m.raw.cpu_pct || 0).toFixed(1) + '%';
  const ramTxt = (Number(m.raw.mem_used_mb || 0) / 1024).toFixed(1) + '/' + (Number(m.raw.mem_total_mb || 0) / 1024).toFixed(1) + ' GB';
  const ioTxt = (m.raw.io_read_mb_s != null && m.raw.io_write_mb_s != null)
    ? Number(m.raw.io_read_mb_s).toFixed(0) + '/' + Number(m.raw.io_write_mb_s).toFixed(0) + ' MB/s'
    : 'N/A';
  const gpuTxt = m.raw.gpu_pct != null ? Number(m.raw.gpu_pct).toFixed(1) + '%' : 'N/A';
  const targetOut = targetTxt.replace(/\s+/g, ' ').slice(0, 42);
  const powerTxt = m.raw.power_watts == null ? 'N/A' : Number(m.raw.power_watts).toFixed(1) + ' W';

  // Real dome integral & rentable
  let drealTxt = '—';
  let rentaTxt = '—';
  if (state.domeActive) {
    const realD = state.domeRealIntegral - 0.06;
    drealTxt = realD.toFixed(4);
    if (act === 'idle' || act === 'media') {
      rentaTxt = 'PAUSE';
    } else if (state.domeActionsOk === 0 && state.domeActionsTotal > 0) {
      rentaTxt = 'NON (refuse)';
    } else if (state.domeRealIntegral > 0.06) {
      rentaTxt = 'OUI';
    } else {
      rentaTxt = '...';
    }
  }

  set('hudSigma', sigmaTxt);
  set('hudPi', piTxt);
  set('hudCpu', cpuTxt);
  set('hudRam', ramTxt);
  set('hudIo', ioTxt);
  set('hudGpu', gpuTxt);
  set('hudWl', state.wl.toUpperCase());
  set('hudTarget', targetOut);
  set('hudDreal', drealTxt);
  set('hudRenta', rentaTxt);
  set('hudPower', powerTxt);

  let energyTxt = 'N/A';
  if (t && t.total && t.total.has_power_data) {
    energyTxt = Number(t.day && t.day.energy_kwh || 0).toFixed(3) + ' kWh/j (' + ccy + ')';
  }
  set('hudEnergy', energyTxt);

  pushSystemHudData({
    dome: state.domeActive ? 'ACTIF' : 'IDLE',
    activity: act.toUpperCase(),
    sigma: sigmaTxt,
    pi: piTxt,
    cpu: cpuTxt,
    ram: ramTxt,
    io: ioTxt,
    gpu: gpuTxt,
    workload: state.wl,
    target: targetOut,
    d_real: drealTxt,
    rentable: rentaTxt,
    power: powerTxt,
    energy: energyTxt,
  });
}
// ─── Utils ────────────────────────────────────────────────────────────────────
function set(id, txt) {
  const el = document.getElementById(id);
  if (!el) return;
  const next = String(txt);
  if (el.textContent !== next) el.textContent = next;
}
function updateDomeBadge(on) {
  document.getElementById('domeStatus').textContent = on ? 'DOME ACTIF' : 'DOME IDLE';
  document.getElementById('domeStatus').className   = 'status-pill ' + (on ? 'pill-run' : 'pill-off');
  document.getElementById('domeBadge').classList.toggle('show', on);
}
startClockLoop();

function soulKernelDomCleanup() {
  try {
    if (pollTimer) {
      clearTimeout(pollTimer);
      pollTimer = null;
    }
    if (clockIntervalId != null) {
      clearInterval(clockIntervalId);
      clockIntervalId = null;
    }
    stopProcessRefreshLoop();
  } catch (_) {}
}
if (typeof window !== 'undefined') {
  window.addEventListener('focus', () => {
    state.windowFocused = true;
    state.lastUserInteractionTs = Date.now();
    scheduleNextPoll();
    startProcessRefreshLoop();
  });
  window.addEventListener('blur', () => {
    state.windowFocused = false;
  });
  ['pointerdown', 'keydown', 'mousedown', 'touchstart'].forEach(evt => {
    window.addEventListener(evt, () => {
      state.windowFocused = true;
      state.lastUserInteractionTs = Date.now();
    }, { passive: true });
  });
  window.addEventListener('beforeunload', soulKernelDomCleanup);
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) {
      stopClockLoop();
      stopProcessRefreshLoop();
    } else {
      startClockLoop();
      startProcessRefreshLoop();
      if (state.lastMetrics) {
        lastRenderedMetricKey = null;
        scheduleMetricRender(state.lastMetrics);
      }
      state.windowFocused = true;
      state.lastUserInteractionTs = Date.now();
      refreshProcesses({ userInitiated: false });
      refreshTelemetrySummary(true);
      scheduleNextPoll();
    }
  });
}

function log(msg, lvl='info') {
  const panel = document.getElementById('logPanel');
  if (!panel) return;
  const ts = new Date().toTimeString().slice(0,8);
  const clsMap = {ok:'lvl-ok',warn:'lvl-warn',info:'lvl-info',err:'lvl-err'};
  const lblMap = {ok:'OK ',warn:'WRN',info:'INF',err:'ERR'};
  const entry = document.createElement('div');
  entry.className = 'log-entry';
  const tsSpan = document.createElement('span');
  tsSpan.className = 'log-ts';
  tsSpan.textContent = ts;
  const lvlSpan = document.createElement('span');
  lvlSpan.className = 'log-lvl ' + (clsMap[lvl] || 'lvl-info');
  lvlSpan.textContent = lblMap[lvl] || 'INF';
  const msgSpan = document.createElement('span');
  msgSpan.className = 'log-msg';
  msgSpan.textContent = String(msg);
  entry.append(tsSpan, lvlSpan, msgSpan);
  panel.prepend(entry);
  while (panel.children.length > 80) panel.removeChild(panel.lastChild);
  if (lvl === 'err' || lvl === 'warn') {
    auditEmit('ui_log', 'panel', lvl, { message: String(msg) });
  }
}

// ─── Mode navigateur (hors Tauri) : pas de métriques réelles, pas de simulation ──
// Quand la page est ouverte en direct dans le navigateur, il n'y a pas de backend Rust.
// Il faut lancer l'app avec "cargo tauri dev" (ou l'exécutable) pour avoir les vraies métriques.
function fallbackInvoke(cmd, args) {
  if (cmd === 'get_metrics') {
    return Promise.resolve({
      cpu: 0, mem: 0,
      compression: null, io_bandwidth: null, gpu: null,
      sigma: 0, epsilon: [0,0,0,0,0],
      raw: {
        cpu_pct: 0, cpu_clock_mhz: null, mem_used_mb: 0, mem_total_mb: 0, ram_clock_mhz: null,
        cpu_max_clock_mhz: null, cpu_freq_ratio: null, cpu_temp_c: null,
        swap_used_mb: 0, swap_total_mb: 0,
        zram_used_mb: null, io_read_mb_s: null, io_write_mb_s: null,
        gpu_pct: null, gpu_core_clock_mhz: null, gpu_mem_clock_mhz: null, gpu_temp_c: null, gpu_power_watts: null, gpu_mem_used_mb: null, gpu_mem_total_mb: null,
        power_watts: null, psi_cpu: null, psi_mem: null, load_avg_1m_norm: null, runnable_tasks: null,
        platform: 'Hors Tauri — lancez cargo tauri dev',
      }
      });
  }
  if (cmd === 'platform_info') return Promise.resolve({
    os: 'Hors Tauri', kernel: 'lancez l\'app native',
    features: ['pas de métriques réelles'],
    has_cgroups_v2: false, has_zram: false, has_gpu_sysfs: false, is_root: false
  });
  if (cmd === 'list_processes') return Promise.resolve({ processes: [], top_processes: [], top_process_rows: [], grouped_processes: [], overhead_audit: null, summary: null });
  if (cmd === 'list_displays') return Promise.resolve([{ index: 0, name: 'Primary', width: 1920, height: 1080, x: 0, y: 0, scale_factor: 1.0, is_primary: true }]);
  if (cmd === 'activate_dome') return Promise.resolve({
    activated: false, pi: 0, dome_gain: 0, b_idle: 0,
    message: 'Non disponible hors Tauri.',
    actions: [], actions_ok: 0, actions_total: 0
  });
  if (cmd === 'rollback_dome') return Promise.resolve(['Non disponible hors Tauri.']);
  if (cmd === 'get_snapshot_before_dome') return Promise.resolve(null);
  if (cmd === 'set_soulram') return Promise.resolve(['[ko] Non disponible hors Tauri.']);
  if (cmd === 'get_soulram_status') return Promise.resolve({ active: false, percent: 20, backend: 'Hors Tauri' });
  if (cmd === 'set_policy_mode') return Promise.resolve('privileged');
  if (cmd === 'get_policy_status') return Promise.resolve({ mode: state.policyMode || 'privileged', is_admin: false, reboot_pending: false, memory_compression_enabled: null });
  if (cmd === 'set_taskbar_gauge') return Promise.resolve(null);
  if (cmd === 'run_kpi_probe') {
    const c = String(args?.command || '').trim().toLowerCase();
    if (c === 'system') {
      return Promise.resolve({
        command: 'system',
        args: args?.args || ['4000'],
        cwd: args?.cwd || null,
        duration_ms: 1200,
        success: true,
        exit_code: 0,
        stdout_tail: 'OS 1200ms | CPU~0% RAM~50% | I/O~0MB/s | n=5',
        stderr_tail: '',
      });
    }
    return Promise.resolve({ command: args?.command || 'demo', args: args?.args || [], cwd: args?.cwd || null, duration_ms: 1200, success: true, exit_code: 0, stdout_tail: 'fallback', stderr_tail: ' ' });
  }
  if (cmd === 'run_ab_benchmark') {
    const request = args?.request || {};
    const samples = [];
    const runs = Math.max(1, Number(request.runs_per_state || 5));
    for (let i = 0; i < runs * 2; i += 1) {
      const off = i % 2 === 0;
      samples.push({
        idx: i + 1,
        phase: off ? 'off' : 'on',
        ts: new Date().toISOString(),
        duration_ms: off ? 1200 + i * 10 : 1050 + i * 10,
        success: true,
        exit_code: 0,
        dome_active: !off,
        workload: request.workload || 'es',
        kappa: Number(request.kappa || 2),
        sigma_max: Number(request.sigma_max || 0.8),
        eta: Number(request.eta || 0.2),
        sigma_before: off ? 0.28 : 0.24,
        sigma_after: off ? 0.29 : 0.25,
        cpu_before_pct: off ? 42 : 36,
        cpu_after_pct: off ? 44 : 37,
        mem_before_gb: off ? 8.5 : 8.8,
        mem_after_gb: off ? 8.6 : 8.9,
        gpu_before_pct: off ? 31 : 27,
        gpu_after_pct: off ? 32 : 26,
        io_before_mb_s: off ? 420 : 360,
        io_after_mb_s: off ? 430 : 340,
        power_before_watts: off ? 118 : 105,
        power_after_watts: off ? 121 : 101,
        cpu_temp_before_c: off ? 68 : 64,
        cpu_temp_after_c: off ? 70 : 63,
        gpu_temp_before_c: off ? 66 : 61,
        gpu_temp_after_c: off ? 67 : 60,
        sigma_effective_before: off ? 0.33 : 0.27,
        sigma_effective_after: off ? 0.35 : 0.26,
        stdout_tail: 'fallback',
        stderr_tail: '',
      });
    }
    return Promise.resolve({
      started_at: new Date().toISOString(),
      finished_at: new Date().toISOString(),
      command: request.command || 'demo',
      args: request.args || [],
      cwd: request.cwd || null,
      runs_per_state: runs,
      settle_ms: Number(request.settle_ms || 1200),
      workload: request.workload || 'es',
      kappa: Number(request.kappa || 2),
      sigma_max: Number(request.sigma_max || 0.8),
      eta: Number(request.eta || 0.2),
      target_pid: request.target_pid || null,
      policy_mode: request.policy_mode || 'privileged',
      soulram_percent: request.soulram_percent || state.soulRamPercent || 20,
      samples,
      summary: computeAbSummary(samples),
    });
  }
  if (cmd === 'get_benchmark_history') {
    const sessions = state.kpiBench.sessions || [];
    return Promise.resolve({
      sessions,
      last_summary: state.kpiBench.lastSummary || (sessions[0]?.summary ?? null),
      advice: state.kpiBench.tuningAdvice || null,
      top_sessions: state.kpiBench.topSessions || [],
    });
  }
  if (cmd === 'clear_benchmark_history') return Promise.resolve(null);
  if (cmd === 'export_benchmark_to_file') return Promise.reject('Export benchmark disponible uniquement dans l’app native.');
  if (cmd === 'export_gains_to_file') return Promise.reject('Export fichier disponible uniquement dans l’app native.');
  if (cmd === 'audit_log_event') return Promise.resolve(null);
  if (cmd === 'get_audit_log_path') return Promise.resolve('Hors Tauri');
  if (cmd === 'get_evidence_data_paths') return Promise.resolve({
    telemetrySamplesJsonl: '(hors Tauri)',
    lifetimeGainsJson: '(hors Tauri)',
    energyPricingJson: '(hors Tauri)',
    benchmarkSessionsJsonl: '(hors Tauri)',
    auditLogJsonl: '(hors Tauri)',
  });
  if (cmd === 'ingest_telemetry_sample') return Promise.resolve(null);
  if (cmd === 'get_telemetry_summary') return Promise.resolve({ pricing: { currency: 'EUR', price_per_kwh: 0.22, co2_kg_per_kwh: 0.05 }, total: { mem_gb_hours_differential: 0, passive_clean_h: 0 }, hour: {}, day: {}, week: {}, month: {}, year: {}, live_power_w: null, data_real_power: false, power_source: 'cpu_differential', lifetime: { first_launch_ts: 0, total_dome_activations: 0, total_dome_hours: 0, total_cpu_hours_differential: 0, total_mem_gb_hours_differential: 0, total_energy_kwh: 0, total_co2_measured_kg: 0, total_energy_cost_measured: 0, total_dome_gain_integral: 0, avg_kpi_gain_pct: null, total_samples: 0, has_real_power: false, total_idle_hours: 0, total_media_hours: 0, soulram_active_hours: 0 } });
  if (cmd === 'get_lifetime_gains') return Promise.resolve({ first_launch_ts: 0, total_dome_activations: 0, total_dome_hours: 0, total_cpu_hours_differential: 0, total_mem_gb_hours_differential: 0, total_energy_kwh: 0, total_co2_measured_kg: 0, total_energy_cost_measured: 0, total_dome_gain_integral: 0, avg_kpi_gain_pct: null, total_samples: 0, has_real_power: false, total_idle_hours: 0, total_media_hours: 0, soulram_active_hours: 0 });
  if (cmd === 'get_energy_pricing') return Promise.resolve({ currency: 'EUR', price_per_kwh: 0.22, co2_kg_per_kwh: 0.05 });
  if (cmd === 'set_energy_pricing') return Promise.resolve(null);
  if (cmd === 'get_external_power_config') return Promise.resolve({ enabled: false, power_file: '', max_age_ms: 15000 });
  if (cmd === 'set_external_power_config') return Promise.resolve(null);
  if (cmd === 'get_external_power_status') return Promise.resolve({
    configPath: '(hors Tauri)',
    powerFilePath: '~/.config/soulkernel/meross_power.json',
    enabled: false,
    maxAgeMs: 15000,
    configExists: false,
    powerFileExists: false,
    lastWatts: null,
    lastTsMs: null,
    isFresh: false,
    sourceTag: 'meross_wall',
    autostartBridge: false,
    bridgeIntervalS: 8,
    merossRegion: 'eu',
    merossDeviceType: 'mss315',
    merossHttpProxy: '',
    mfaPresent: false,
    credsCachePath: '(hors Tauri)',
    credsCacheExists: false,
    pythonBin: '',
    defaultPythonHint: 'python3',
    credentialsPresent: false,
    bridgeLogPath: '(hors Tauri)',
  });
  if (cmd === 'get_external_bridge_status') return Promise.resolve({
    running: false,
    pid: null,
    lastError: null,
    lastStartTsMs: null,
    scriptPath: '(hors Tauri)',
    bridgeLogPath: '(hors Tauri)',
    resolvedPythonBin: 'python3',
    pythonSource: 'system',
  });
  if (cmd === 'start_external_bridge') return Promise.resolve({
    running: false,
    pid: null,
    lastError: 'Bridge indisponible hors Tauri',
    lastStartTsMs: null,
    scriptPath: '(hors Tauri)',
    bridgeLogPath: '(hors Tauri)',
    resolvedPythonBin: 'python3',
    pythonSource: 'system',
  });
  if (cmd === 'stop_external_bridge') return Promise.resolve({
    running: false,
    pid: null,
    lastError: null,
    lastStartTsMs: null,
    scriptPath: '(hors Tauri)',
    bridgeLogPath: '(hors Tauri)',
    resolvedPythonBin: 'python3',
    pythonSource: 'system',
  });
  if (cmd === 'open_system_hud') return Promise.resolve(null);
  if (cmd === 'close_system_hud') return Promise.resolve(null);
  if (cmd === 'set_system_hud_data') return Promise.resolve(null);
  if (cmd === 'set_system_hud_interactive') return Promise.resolve(null);
  if (cmd === 'set_system_hud_preset') return Promise.resolve(null);
  if (cmd === 'set_system_hud_presentation') return Promise.resolve(null);
  if (cmd === 'set_system_hud_display') return Promise.resolve(null);
  return Promise.reject('unknown command');
}


// ─── Init ─────────────────────────────────────────────────────────────────────
export async function bootSoulKernelApp(): Promise<void> {
if (HUD_ONLY) {
  await initHudOnlyMode();
  refreshSoulKernelLucide();
  return;
}
registerTauriBridge();
await loadWorkloadCatalog();
wireLegacyDomListeners();
wireLegacyDomMore();
loadRuntimeSettings();
setViewMode(state.viewMode);
syncHudPanelFromState();
wireHudLayoutControls();
applyHudPresentation();
setHudInteractive(!!state.hudInteractive);
setHudVisible(!!state.hudVisible);
if (hasTauri) { try { await refreshHudDisplays(); } catch (_) {} }
try { await invoke('set_policy_mode', { mode: state.policyMode }); } catch (_) {}
loadDomeHistory();
log('SoulKernel demarre - ' + (hasTauri ? 'Tauri runtime (metriques reelles)' : 'Hors Tauri - lancez cargo tauri dev pour les metriques'), hasTauri ? 'ok' : 'warn');
loadPlatformInfo();
updateSystemStatus(null);
poll();
refreshProcesses({ userInitiated: false });
await syncSoulRamStatus();
await loadPolicyStatus();
await applyStartupIntentIfAny();
await loadBenchmarkHistory(true);
await refreshTelemetrySummary(true);
try { await loadExternalPowerConfig(); } catch (_) {}
try { await refreshExternalPowerStatus(); } catch (_) {}
try { await refreshExternalBridgeStatus(); } catch (_) {}
updateSoulRamUi();
const ad = document.getElementById('adaptiveEnabled'); if (ad) ad.checked = !!state.adaptiveEnabled;
const aad = document.getElementById('adaptiveAutoDome'); if (aad) aad.checked = !!state.adaptiveAutoDome;
updateAdaptiveStatusText();
renderAbSummary(state.kpiBench.lastSummary);
renderBenchmarkLearning(state.kpiBench.tuningAdvice);
saveRuntimeSettings();
  scheduleNextPoll();
  startProcessRefreshLoop();
  refreshSoulKernelLucide();
}
