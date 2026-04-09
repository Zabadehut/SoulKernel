import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import zlib from 'node:zlib';
import crypto from 'node:crypto';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PORT = Number(process.env.PORT || 8787);
const ACTIVE_FRESHNESS_MS = 30_000;
const DEFAULT_TIMELINE_LIMIT = 720;
const MAX_SAMPLES_PER_MACHINE = 2000;

// ── Supervisor state ────────────────────────────────────────────────────────

/** Si défini, POST /api/register exige ce token en Bearer. */
const ENROLL_TOKEN = process.env.ENROLL_TOKEN || null;

/** machine_id → { api_key, registered_at_ms, hostname, platform, app, version } */
const supervisorMachines = new Map();

/** machine_id → sample[] (anneau, MAX_SAMPLES_PER_MACHINE entrées max) */
const supervisorSamples = new Map();

function defaultMachinesPath() {
  if (process.env.SUPERVISOR_MACHINES_PATH) return process.env.SUPERVISOR_MACHINES_PATH;
  if (process.platform === 'win32' && process.env.APPDATA)
    return path.join(process.env.APPDATA, 'SoulKernel', 'supervisor', 'machines.json');
  const base = process.env.XDG_DATA_HOME || path.join(os.homedir(), '.local', 'share');
  return path.join(base, 'SoulKernel', 'supervisor', 'machines.json');
}

const MACHINES_PATH = defaultMachinesPath();

function loadMachines() {
  try {
    if (!fs.existsSync(MACHINES_PATH)) return;
    const data = JSON.parse(fs.readFileSync(MACHINES_PATH, 'utf8'));
    for (const [k, v] of Object.entries(data)) supervisorMachines.set(k, v);
    console.log(`  ${supervisorMachines.size} machine(s) chargée(s) depuis ${MACHINES_PATH}`);
  } catch (e) {
    console.warn(`  Impossible de charger les machines: ${e.message}`);
  }
}

function saveMachines() {
  try {
    fs.mkdirSync(path.dirname(MACHINES_PATH), { recursive: true });
    fs.writeFileSync(
      MACHINES_PATH,
      JSON.stringify(Object.fromEntries(supervisorMachines), null, 2),
      'utf8',
    );
  } catch {}
}

function totalSampleCount() {
  let n = 0;
  for (const arr of supervisorSamples.values()) n += arr.length;
  return n;
}

function pushSample(machine_id, sample) {
  if (!supervisorSamples.has(machine_id)) supervisorSamples.set(machine_id, []);
  const arr = supervisorSamples.get(machine_id);
  arr.push(sample);
  if (arr.length > MAX_SAMPLES_PER_MACHINE)
    arr.splice(0, arr.length - MAX_SAMPLES_PER_MACHINE);
}

// ── HTTP helpers ────────────────────────────────────────────────────────────

function defaultObservabilityPath() {
  if (process.env.SOULKERNEL_OBSERVABILITY_PATH) return process.env.SOULKERNEL_OBSERVABILITY_PATH;
  if (process.platform === 'win32' && process.env.APPDATA) {
    return path.join(process.env.APPDATA, 'SoulKernel', 'telemetry', 'observability_samples.jsonl');
  }
  if (process.env.XDG_DATA_HOME) {
    return path.join(process.env.XDG_DATA_HOME, 'SoulKernel', 'telemetry', 'observability_samples.jsonl');
  }
  return path.join(os.homedir(), '.local', 'share', 'SoulKernel', 'telemetry', 'observability_samples.jsonl');
}

const OBSERVABILITY_PATH = defaultObservabilityPath();

function json(res, status, payload) {
  const body = JSON.stringify(payload);
  res.writeHead(status, {
    'Content-Type': 'application/json; charset=utf-8',
    'Cache-Control': 'no-store',
  });
  res.end(body);
}

function text(res, status, body, contentType = 'text/plain; charset=utf-8') {
  res.writeHead(status, {
    'Content-Type': contentType,
    'Cache-Control': 'no-store',
  });
  res.end(body);
}

function file(res, filePath, contentType) {
  fs.readFile(filePath, (err, buffer) => {
    if (err) {
      json(res, 500, { error: err.message });
      return;
    }
    res.writeHead(200, { 'Content-Type': contentType, 'Cache-Control': 'no-store' });
    res.end(buffer);
  });
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
    req.on('error', reject);
  });
}

function getBearerToken(req) {
  const auth = req.headers['authorization'] || '';
  return auth.startsWith('Bearer ') ? auth.slice(7).trim() : null;
}

// ── Local observability (lecture fichier JSONL) ─────────────────────────────

function sampleTs(sample) {
  return Number(
    sample?.report?.exported_at_ms
      ?? sample?.raw_host_metrics?.exported_at_ms
      ?? sample?.strict_evidence?.exported_at_ms
      ?? 0
  );
}

function getRaw(sample) {
  return sample?.report?.metrics?.raw || sample?.raw_host_metrics?.raw || {};
}

function getKpi(sample) {
  return sample?.kpi || sample?.report?.kpi || {};
}

function getTelemetry(sample) {
  return sample?.report?.telemetry || sample?.telemetry_summary || {};
}

function getExternal(sample) {
  return sample?.external_power || sample?.report?.external_power || {};
}

function getGains(sample) {
  return sample?.gains_summary || {};
}

function parseJsonl(content, source) {
  return content
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      try {
        return JSON.parse(line);
      } catch (error) {
        throw new Error(`${source}: ${error.message}`);
      }
    });
}

function readJsonlFile(filePath) {
  if (!fs.existsSync(filePath)) return [];
  const content = fs.readFileSync(filePath, 'utf8');
  return parseJsonl(content, filePath);
}

function readGzipJsonlFile(filePath) {
  if (!fs.existsSync(filePath)) return [];
  const compressed = fs.readFileSync(filePath);
  const content = zlib.gunzipSync(compressed).toString('utf8');
  return parseJsonl(content, filePath);
}

function observabilityArchives(activePath) {
  const dir = path.dirname(activePath);
  const stem = path.basename(activePath, '.jsonl');
  if (!fs.existsSync(dir)) return [];
  return fs
    .readdirSync(dir)
    .filter((name) => name.startsWith(`${stem}-`) && name.endsWith('.jsonl.gz'))
    .sort()
    .map((name) => path.join(dir, name));
}

function loadObservability({ includeArchives = true } = {}) {
  const activePath = OBSERVABILITY_PATH;
  const archives = includeArchives ? observabilityArchives(activePath) : [];
  const activeSamples = readJsonlFile(activePath);
  const archiveSamples = archives.flatMap((archive) => readGzipJsonlFile(archive));
  const byTs = new Map();
  for (const sample of [...archiveSamples, ...activeSamples]) {
    byTs.set(sampleTs(sample), sample);
  }
  const samples = [...byTs.entries()]
    .sort((a, b) => a[0] - b[0])
    .map(([, sample]) => sample);
  const latest = samples.at(-1) || null;
  const stat = fs.existsSync(activePath) ? fs.statSync(activePath) : null;
  const latestTs = latest ? sampleTs(latest) : null;
  return {
    activePath,
    archives,
    archiveCount: archives.length,
    activeExists: !!stat,
    activeSizeBytes: stat?.size || 0,
    activeModifiedMs: stat ? stat.mtimeMs : null,
    latestTs,
    isFresh: latestTs != null ? Date.now() - latestTs <= ACTIVE_FRESHNESS_MS : false,
    samples,
    latest,
  };
}

function projectSample(sample) {
  const raw = getRaw(sample);
  const kpi = getKpi(sample);
  const gains = getGains(sample);
  const pc = sample?.power_comparison || {};
  const norm = sample?.raw_host_metrics?.normalized || {};
  const formula = sample?.report?.formula || {};
  return {
    ts_ms: sampleTs(sample),
    exported_at: sample?.report?.exported_at || null,
    // Power
    watts: raw.wall_power_watts ?? raw.power_watts ?? getTelemetry(sample)?.live_power_w ?? null,
    host_power_w: pc.host_power_w ?? raw.host_power_watts ?? null,
    wall_power_w: pc.wall_power_w ?? raw.wall_power_watts ?? null,
    power_confidence: pc.confidence ?? null,
    // Resources
    cpu_pct: raw.cpu_pct ?? null,
    ram_pct: raw.mem_total_mb ? (raw.mem_used_mb / raw.mem_total_mb) * 100 : null,
    ram_used_mb: raw.mem_used_mb ?? null,
    ram_total_mb: raw.mem_total_mb ?? null,
    gpu_pct: raw.gpu_pct ?? null,
    gpu_power_watts: raw.gpu_power_watts ?? null,
    faults_per_sec: raw.page_faults_per_sec ?? null,
    sigma: sample?.report?.metrics?.sigma ?? null,
    compression: norm.compression ?? null,
    // KPI
    kpi_basic: kpi.kpi_basic_w_per_pct ?? null,
    kpi_penalized: kpi.kpi_penalized_w_per_pct ?? null,
    kpi_label: kpi.label ?? null,
    kpi_trend: kpi.trend ?? null,
    kpi_reward_ratio: kpi.reward_ratio ?? null,
    cpu_useful_pct: kpi.cpu_useful_pct ?? null,
    cpu_overhead_pct: kpi.cpu_overhead_pct ?? null,
    // Formula
    pi: formula.advanced_guard != null ? formula.pi ?? null : null,
    advanced_guard: formula.advanced_guard ?? null,
    // Dome / SoulRAM
    dome_active: sample?.report?.dome_active ?? false,
    soulram_active: sample?.report?.soulram_active ?? false,
    target_pid: sample?.report?.target_pid ?? null,
    workload: sample?.report?.workload ?? null,
    // Gains session (dome ON vs OFF correlation)
    dome_on_avg_w: gains.session_dome_on_avg_power_w ?? null,
    dome_off_avg_w: gains.session_dome_off_avg_power_w ?? null,
    energy_saved_kwh: gains.session_energy_saved_kwh ?? null,
    // Context
    machine_activity: sample?.strict_evidence?.machine_activity ?? null,
  };
}

function buildStatus(store) {
  const latest = store.latest;
  const raw = latest ? getRaw(latest) : {};
  const telemetry = latest ? getTelemetry(latest) : {};
  const external = latest ? getExternal(latest) : {};
  return {
    server_time_ms: Date.now(),
    observability_path: store.activePath,
    active_exists: store.activeExists,
    active_size_bytes: store.activeSizeBytes,
    active_modified_ms: store.activeModifiedMs,
    archives: store.archives,
    archive_count: store.archiveCount,
    sample_count: store.samples.length + totalSampleCount(),
    machine_count: supervisorMachines.size,
    latest_sample_ts_ms: store.latestTs,
    is_fresh: store.isFresh,
    power_source: telemetry.power_source || raw.power_watts_source || external.source_tag || null,
    latest_watts: raw.wall_power_watts ?? raw.power_watts ?? telemetry.live_power_w ?? null,
  };
}

// ── Supervisor endpoints (POST) ─────────────────────────────────────────────

async function handleRegister(req, res) {
  // Vérification du token d'enrôlement si configuré
  if (ENROLL_TOKEN) {
    const token = getBearerToken(req);
    if (token !== ENROLL_TOKEN) {
      json(res, 401, { error: 'Token d\'enrôlement invalide' });
      return;
    }
  }

  let body;
  try {
    body = JSON.parse(await readBody(req));
  } catch {
    json(res, 400, { error: 'JSON invalide' });
    return;
  }

  const hint = String(body.machine_id_hint || body.hostname || '').trim();
  const machine_id = (hint || `machine-${Date.now()}`)
    .replace(/[^a-zA-Z0-9_-]/g, '-')
    .slice(0, 64);
  const api_key = crypto.randomBytes(24).toString('hex');

  supervisorMachines.set(machine_id, {
    api_key,
    registered_at_ms: Date.now(),
    hostname: body.hostname || null,
    platform: body.platform || null,
    app: body.app || null,
    version: body.version || null,
  });
  saveMachines();

  const host = req.headers.host || `localhost:${PORT}`;
  const ingest_url = `http://${host}/api/ingest`;
  console.log(`  [register] machine="${machine_id}" host="${host}"`);
  json(res, 200, { machine_id, api_key, ingest_url });
}

async function handleIngest(req, res) {
  const token = getBearerToken(req);

  // Recherche de la machine par api_key
  let machine_id = null;
  for (const [id, m] of supervisorMachines) {
    if (m.api_key === token) { machine_id = id; break; }
  }
  if (!machine_id) {
    json(res, 401, { error: 'Clé API inconnue ou manquante' });
    return;
  }

  let body;
  try {
    body = JSON.parse(await readBody(req));
  } catch {
    json(res, 400, { error: 'JSON invalide' });
    return;
  }

  pushSample(machine_id, body);
  json(res, 202, { ok: true, machine_id });
}

// ── Routeur API ─────────────────────────────────────────────────────────────

async function routeApi(req, res, url) {
  const includeArchives = url.searchParams.get('archives') !== '0';
  const method = req.method.toUpperCase();

  // ── POST /api/register ───────────────────────────────────────────────────
  if (url.pathname === '/api/register' && method === 'POST') {
    await handleRegister(req, res);
    return true;
  }

  // ── POST /api/ingest ─────────────────────────────────────────────────────
  if (url.pathname === '/api/ingest' && method === 'POST') {
    await handleIngest(req, res);
    return true;
  }

  // ── GET routes (lecture locale) ──────────────────────────────────────────
  const store = loadObservability({ includeArchives });

  if (url.pathname === '/api/status') {
    json(res, 200, buildStatus(store));
    return true;
  }

  if (url.pathname === '/api/latest') {
    // Priorité : données ingérées distantes, fallback fichier local
    const machine_id = url.searchParams.get('machine_id');
    const remoteSamples = machine_id
      ? (supervisorSamples.get(machine_id) || [])
      : [...supervisorSamples.values()].flat().sort((a, b) => sampleTs(a) - sampleTs(b));
    const remoteLatest = remoteSamples.at(-1) || null;
    const latest = remoteLatest || store.latest;
    if (!latest) {
      json(res, 404, { error: 'Aucun échantillon d\'observabilité trouvé', ...buildStatus(store) });
      return true;
    }
    json(res, 200, {
      status: buildStatus(store),
      latest,
      latest_projection: projectSample(latest),
    });
    return true;
  }

  if (url.pathname === '/api/timeline') {
    const limit = Math.max(1, Math.min(Number(url.searchParams.get('limit') || DEFAULT_TIMELINE_LIMIT), 5000));
    const sinceMs = Number(url.searchParams.get('since_ms') || 0);
    const machine_id = url.searchParams.get('machine_id');

    // Fusionner fichier local + données distantes pour la machine demandée (ou toutes)
    const remoteSamples = machine_id
      ? (supervisorSamples.get(machine_id) || [])
      : [...supervisorSamples.values()].flat();
    const byTs = new Map();
    for (const s of [...store.samples, ...remoteSamples]) byTs.set(sampleTs(s), s);
    let samples = [...byTs.entries()]
      .sort((a, b) => a[0] - b[0])
      .map(([, s]) => s);
    if (sinceMs > 0) samples = samples.filter((s) => sampleTs(s) >= sinceMs);
    if (samples.length > limit) samples = samples.slice(-limit);
    json(res, 200, {
      status: buildStatus(store),
      count: samples.length,
      samples: samples.map(projectSample),
    });
    return true;
  }

  // ── GET /api/machines ────────────────────────────────────────────────────
  if (url.pathname === '/api/machines' && method === 'GET') {
    const machines = [...supervisorMachines.entries()].map(([id, m]) => ({
      machine_id: id,
      registered_at_ms: m.registered_at_ms,
      hostname: m.hostname,
      platform: m.platform,
      app: m.app,
      version: m.version,
      sample_count: supervisorSamples.get(id)?.length ?? 0,
      last_sample_ts_ms: supervisorSamples.get(id)?.at(-1)
        ? sampleTs(supervisorSamples.get(id).at(-1))
        : null,
    }));
    json(res, 200, { machine_count: machines.length, machines });
    return true;
  }

  return false;
}

// ── Serveur HTTP ─────────────────────────────────────────────────────────────

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url || '/', `http://${req.headers.host || 'localhost'}`);

  if (url.pathname.startsWith('/api/')) {
    try {
      if (await routeApi(req, res, url)) return;
    } catch (e) {
      json(res, 500, { error: e.message });
      return;
    }
    json(res, 404, { error: 'Not found' });
    return;
  }

  if (url.pathname === '/' || url.pathname === '/index.html') {
    file(res, path.join(__dirname, 'index.html'), 'text/html; charset=utf-8');
    return;
  }
  if (url.pathname === '/app.js') {
    file(res, path.join(__dirname, 'app.js'), 'application/javascript; charset=utf-8');
    return;
  }
  if (url.pathname === '/styles.css') {
    file(res, path.join(__dirname, 'styles.css'), 'text/css; charset=utf-8');
    return;
  }

  text(res, 404, 'Not found');
});

loadMachines();
server.listen(PORT, () => {
  console.log(`SoulKernel Supervisor + live dashboard → http://localhost:${PORT}`);
  console.log(`  Observability path : ${OBSERVABILITY_PATH}`);
  console.log(`  Machines path      : ${MACHINES_PATH}`);
  console.log(`  Enroll token       : ${ENROLL_TOKEN ? 'configuré' : 'ouvert (pas de token)'}`);
  console.log(`  Endpoints superviseur :`);
  console.log(`    POST /api/register  → enrôle une machine, retourne api_key`);
  console.log(`    POST /api/ingest    → reçoit les ticks (Bearer api_key)`);
  console.log(`    GET  /api/status    → machine_count + sample_count`);
  console.log(`    GET  /api/machines  → liste des machines enrôlées`);
});
