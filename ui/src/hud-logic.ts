// @ts-nocheck
function refreshSoulKernelLucide() {
  try {
    if (typeof lucide !== 'undefined' && lucide.createIcons) {
      lucide.createIcons({ attrs: { 'stroke-width': 1.75 } });
    }
  } catch (_) {}
}
refreshSoulKernelLucide();
const t = window.__TAURI__;
const invoke = t?.core?.invoke ? t.core.invoke : null;
const root = document.getElementById('hudRoot');
const mode = document.getElementById('hudMode');
const drag = document.getElementById('hudDrag');
let interactive = false;
let lastDataSig = '';
let lastCfgSig = '';
let lastReadyPing = 0;
let _hudFitLast = { w: 0, h: 0 };
let _hudFitTimer = null;
let _hudPollInterval = null;
/** @type {null | (() => void)} */
let _unlistenHudCfg = null;
/** @type {'screen'|'content'|'manual'} */
let sizeMode = 'screen';

function syncHudLayoutChrome() {
  const html = document.documentElement;
  if (sizeMode === 'content') {
    html.classList.add('hud-compact');
    html.classList.remove('hud-stretch');
  } else {
    html.classList.add('hud-stretch');
    html.classList.remove('hud-compact');
  }
}

function applyMetricVisibility(cfg) {
  const vm = cfg && cfg.visible_metrics;
  if (!vm || !Array.isArray(vm)) return;
  const set = new Set(vm);
  document.querySelectorAll('.hud-tile[data-metric]').forEach((el) => {
    const k = el.getAttribute('data-metric');
    el.style.display = set.has(k) ? '' : 'none';
  });
}

function scheduleHudWindowFit() {
  if (!invoke || !root) return;
  if (sizeMode !== 'content') return;
  clearTimeout(_hudFitTimer);
  _hudFitTimer = setTimeout(async () => {
    try {
      const pad = 10;
      const r = root.getBoundingClientRect();
      const w = Math.ceil(Math.max(r.width, root.scrollWidth) + pad);
      const h = Math.ceil(
        Math.max(r.height, root.scrollHeight, root.offsetHeight) + pad
      );
      if (Math.abs(w - _hudFitLast.w) < 2 && Math.abs(h - _hudFitLast.h) < 2) return;
      _hudFitLast = { w, h };
      await invoke('set_hud_window_size', { width: w, height: h });
    } catch (_) {}
  }, 90);
}

function runHudFitBurst() {
  if (sizeMode !== 'content') return;
  scheduleHudWindowFit();
  setTimeout(scheduleHudWindowFit, 40);
  setTimeout(scheduleHudWindowFit, 180);
  setTimeout(scheduleHudWindowFit, 450);
}

function set(id, v) {
  const el = document.getElementById(id);
  if (el) el.textContent = v ?? 'N/A';
}
function setPreset(p) {
  root.classList.remove('mini', 'compact', 'detailed');
  root.classList.add(p === 'mini' ? 'mini' : (p === 'detailed' ? 'detailed' : 'compact'));
  refreshSoulKernelLucide();
  if (sizeMode === 'content') {
    _hudFitLast = { w: 0, h: 0 };
    scheduleHudWindowFit();
  }
}
function setInteractive(on) {
  const next = !!on;
  if (interactive === next) return;
  interactive = next;
  document.body.classList.toggle('edit', interactive);
  mode.textContent = interactive ? 'ÉDIT' : 'VERROU';
  if (sizeMode === 'content') {
    _hudFitLast = { w: 0, h: 0 };
    scheduleHudWindowFit();
  }
}
function setHudOpacity(o) {
  if (!root) return;
  const v = Math.max(0.3, Math.min(1, Number(o)));
  if (!Number.isFinite(v)) return;
  root.style.setProperty('--hud-opacity', String(v));
}

function renderPayload(p) {
  set('dome', p.dome);
  set('sigma', p.sigma);
  set('pi', p.pi);
  set('cpu', p.cpu);
  set('ram', p.ram);
  set('target', p.target);
  set('power', p.power);
  set('energy', p.energy);
}

async function refreshHud() {
  if (!invoke) return;
  const now = Date.now();
  if (now - lastReadyPing > 1000) {
    lastReadyPing = now;
    try { await invoke('set_system_hud_ready', { ts_ms: now }); } catch (_) {}
  }
  try {
    const data = await invoke('get_system_hud_data');
    if (data) {
      const sig = JSON.stringify(data);
      if (sig !== lastDataSig) {
        lastDataSig = sig;
        renderPayload(data);
        scheduleHudWindowFit();
      }
    }
  } catch (_) {}

  try {
    const cfg = await invoke('get_system_hud_config');
    if (cfg) {
      const sig = JSON.stringify({
        preset: cfg.preset,
        interactive: cfg.interactive,
        opacity: cfg.opacity,
        size_mode: cfg.size_mode,
        screen_width_pct: cfg.screen_width_pct,
        screen_height_pct: cfg.screen_height_pct,
        manual_width: cfg.manual_width,
        manual_height: cfg.manual_height,
        visible_metrics: cfg.visible_metrics,
      });
      if (sig !== lastCfgSig) {
        lastCfgSig = sig;
        if (cfg.size_mode === 'screen' || cfg.size_mode === 'content' || cfg.size_mode === 'manual') {
          sizeMode = cfg.size_mode;
        }
        syncHudLayoutChrome();
        setPreset(cfg.preset);
        setInteractive(!!cfg.interactive);
        if (typeof cfg.opacity === 'number') setHudOpacity(cfg.opacity);
        applyMetricVisibility(cfg);
      }
    }
  } catch (_) {}
}

if (drag && t?.window?.getCurrentWindow) {
  drag.addEventListener('mousedown', async (ev) => {
    if (!interactive || ev.button !== 0) return;
    try { await t.window.getCurrentWindow().startDragging(); } catch (_) {}
  });
}

(async () => {
  if (t?.event?.listen) {
    try {
      _unlistenHudCfg = await t.event.listen('soulkernel://hud-config', (ev) => {
        const cfg = ev.payload || {};
        if (cfg.size_mode === 'screen' || cfg.size_mode === 'content' || cfg.size_mode === 'manual') {
          sizeMode = cfg.size_mode;
        }
        syncHudLayoutChrome();
        if (cfg.preset) setPreset(cfg.preset);
        if (typeof cfg.interactive === 'boolean') setInteractive(cfg.interactive);
        if (typeof cfg.opacity === 'number') setHudOpacity(cfg.opacity);
        applyMetricVisibility(cfg);
        runHudFitBurst();
      });
    } catch (_) {}
  }
})();

syncHudLayoutChrome();
setPreset('compact');
setInteractive(false);
setHudOpacity(0.82);
_hudPollInterval = setInterval(refreshHud, 250);
refreshHud();
if (root && typeof ResizeObserver !== 'undefined') {
  new ResizeObserver(() => {
    if (sizeMode === 'content') scheduleHudWindowFit();
  }).observe(root);
}

function hudPageCleanup() {
  if (_hudPollInterval != null) {
    clearInterval(_hudPollInterval);
    _hudPollInterval = null;
  }
  try {
    if (typeof _unlistenHudCfg === 'function') _unlistenHudCfg();
  } catch (_) {}
}
window.addEventListener('beforeunload', hudPageCleanup);
if (document.readyState === 'complete') {
  runHudFitBurst();
} else {
  window.addEventListener('load', runHudFitBurst);
}
requestAnimationFrame(() => {
  requestAnimationFrame(() => runHudFitBurst());
});
