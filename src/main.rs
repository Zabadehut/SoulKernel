//! SoulKernel - Performance Dome orchestrator
//! Tauri entry point - wires frontend to hardware via invoke()

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod audit;
mod benchmark;
mod formula;
mod hud;
mod memory_policy;
mod metrics;
mod orchestrator;
mod platform;
mod telemetry;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{Emitter, Manager, State, WindowEvent};
use tauri_plugin_global_shortcut::{Code, Modifiers, ShortcutState};
use tokio::sync::mpsc;

use audit::{audit_write, default_audit_path, now_ms_local, AuditState, SharedAudit};
use hud::{
    apply_hud_window_mode, cleanup_hud_before_exit, reset_hud_health_for_show, HudHealthState,
    HudOverlayData,
    HudRuntimeState, SharedHud, SharedHudData, SharedHudHealth, SharedHudTx,
};

// ─── State ────────────────────────────────────────────────────────────────────

pub struct SoulKernelState {
    pub dome_active: bool,
    pub policy_mode: platform::PolicyMode,
    pub snapshot_before_dome: Option<metrics::ResourceState>,
    pub current_workload: String,
    pub target_pid: Option<u32>,
    pub soulram_active: bool,
    pub soulram_percent: u8,
}

type SharedState = Arc<Mutex<SoulKernelState>>;
type SharedTelemetry = Arc<Mutex<telemetry::TelemetryState>>;
type SharedBenchmark = Arc<Mutex<benchmark::BenchmarkState>>;

#[derive(serde::Serialize)]
pub struct SoulRamStatusResponse {
    pub active: bool,
    pub percent: u8,
    pub backend: String,
}

#[derive(serde::Serialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_usage: f64,
    pub parent_pid: Option<u32>,
    /// RSS approximative (KiB).
    pub memory_kb: u64,
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
fn list_processes() -> Result<Vec<ProcessInfo>, String> {
    let mut sys = sysinfo::System::new();
    sys.refresh_processes();
    thread::sleep(Duration::from_millis(220));
    sys.refresh_processes();
    let mut list: Vec<ProcessInfo> = sys
        .processes()
        .iter()
        .map(|(pid, p)| ProcessInfo {
            pid: pid.as_u32(),
            name: p.name().to_string(),
            cpu_usage: p.cpu_usage() as f64,
            parent_pid: p.parent().map(|pp| pp.as_u32()),
            memory_kb: p.memory() / 1024,
        })
        .collect();
    list.sort_by(|a, b| {
        b.cpu_usage
            .partial_cmp(&a.cpu_usage)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    list.truncate(100);
    Ok(list)
}

#[tauri::command]
async fn get_metrics() -> Result<metrics::ResourceState, String> {
    metrics::collect().map_err(|e| e.to_string())
}

#[tauri::command]
fn compute_formula(
    state: metrics::ResourceState,
    profile: formula::WorkloadProfile,
    kappa: f64,
) -> formula::FormulaResult {
    formula::compute(&state, &profile, kappa)
}

#[tauri::command]
async fn activate_dome(
    workload: String,
    kappa: f64,
    sigma_max: f64,
    eta: f64,
    target_pid: Option<u32>,
    policy_mode: Option<String>,
    shared: State<'_, SharedState>,
) -> Result<orchestrator::DomeResult, String> {
    activate_dome_inner(
        workload,
        kappa,
        sigma_max,
        eta,
        target_pid,
        policy_mode,
        &shared,
    )
    .await
}

async fn activate_dome_inner(
    workload: String,
    kappa: f64,
    sigma_max: f64,
    eta: f64,
    target_pid: Option<u32>,
    policy_mode: Option<String>,
    shared: &SharedState,
) -> Result<orchestrator::DomeResult, String> {
    let profile = formula::WorkloadProfile::from_name(&workload)
        .ok_or_else(|| format!("Unknown workload: {}", workload))?;

    let metrics = metrics::collect().map_err(|e| e.to_string())?;
    let formula_res = formula::compute(&metrics, &profile, kappa);

    if metrics.sigma >= sigma_max {
        return Ok(orchestrator::DomeResult {
            activated: false,
            pi: formula_res.pi,
            dome_gain: formula_res.dome_gain,
            b_idle: formula_res.b_idle,
            actions: vec![],
            actions_ok: 0,
            actions_total: 0,
            message: format!(
                "DOME BLOQUE - Sigma({:.2}) >= SigmaMax({:.2})",
                metrics.sigma, sigma_max
            ),
        });
    }

    let policy = policy_mode
        .as_deref()
        .map(platform::PolicyMode::from_name)
        .unwrap_or_else(|| shared.lock().unwrap().policy_mode);

    {
        let mut s = shared.lock().unwrap();
        s.snapshot_before_dome = Some(metrics.clone());
        s.dome_active = true;
        s.current_workload = workload.clone();
        s.target_pid = target_pid;
        s.policy_mode = policy;
    }

    let result = orchestrator::activate(&profile, eta, &metrics, policy, target_pid)
        .await
        .map_err(|e| e.to_string())?;

    Ok(result)
}

#[tauri::command]
async fn rollback_dome(shared: State<'_, SharedState>) -> Result<Vec<String>, String> {
    rollback_dome_inner(&shared).await
}

async fn rollback_dome_inner(shared: &SharedState) -> Result<Vec<String>, String> {
    let (snapshot, target_pid) = {
        let mut s = shared.lock().unwrap();
        s.dome_active = false;
        let snap = s.snapshot_before_dome.take();
        let pid = s.target_pid.take();
        (snap, pid)
    };

    orchestrator::rollback(snapshot, target_pid)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn platform_info() -> platform::PlatformInfo {
    platform::info()
}

#[tauri::command]
fn get_snapshot_before_dome(shared: State<'_, SharedState>) -> Option<metrics::ResourceState> {
    shared.lock().unwrap().snapshot_before_dome.clone()
}

#[tauri::command]
async fn set_soulram(
    enabled: bool,
    percent: Option<u8>,
    shared: State<'_, SharedState>,
) -> Result<Vec<String>, String> {
    if enabled {
        let p = percent.unwrap_or(20).clamp(5, 60);
        let actions = platform::enable_soulram(p).await;
        let activated = platform::soulram_enablement_effective(&actions);
        {
            let mut s = shared.lock().unwrap();
            s.soulram_active = activated;
            s.soulram_percent = p;
        }
        Ok(actions
            .into_iter()
            .map(|(a, ok)| {
                if ok {
                    format!("[ok] {}", a)
                } else {
                    format!("[ko] {}", a)
                }
            })
            .collect())
    } else {
        let actions = platform::disable_soulram().await;
        {
            let mut s = shared.lock().unwrap();
            s.soulram_active = false;
        }
        Ok(actions
            .into_iter()
            .map(|(a, ok)| {
                if ok {
                    format!("[ok] {}", a)
                } else {
                    format!("[ko] {}", a)
                }
            })
            .collect())
    }
}

#[tauri::command]
fn get_soulram_status(shared: State<'_, SharedState>) -> SoulRamStatusResponse {
    let s = shared.lock().unwrap();
    SoulRamStatusResponse {
        active: s.soulram_active,
        percent: s.soulram_percent,
        backend: platform::soulram_backend_name(),
    }
}

#[tauri::command]
fn set_policy_mode(mode: String, shared: State<'_, SharedState>) -> String {
    let m = platform::PolicyMode::from_name(&mode);
    shared.lock().unwrap().policy_mode = m;
    m.as_name().to_string()
}

#[tauri::command]
fn get_policy_status(shared: State<'_, SharedState>) -> platform::PolicyStatus {
    let mode = shared.lock().unwrap().policy_mode;
    platform::policy_status(mode)
}

#[tauri::command]
fn set_taskbar_gauge(window: tauri::Window, value: f64) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use tauri::window::{ProgressBarState, ProgressBarStatus};
        let clamped = value.clamp(0.0, 1.0);
        let status = if clamped >= 0.85 {
            ProgressBarStatus::Error
        } else if clamped >= 0.65 {
            ProgressBarStatus::Paused
        } else {
            ProgressBarStatus::Normal
        };
        let progress = (clamped * 100.0).round() as u64;
        window
            .set_progress_bar(ProgressBarState {
                status: Some(status),
                progress: Some(progress),
            })
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (window, value);
    Ok(())
}

#[tauri::command]
async fn run_kpi_probe(
    command: String,
    args: Vec<String>,
    cwd: Option<String>,
) -> Result<benchmark::KpiProbeResult, String> {
    benchmark::execute_probe(command, args, cwd).await
}

#[tauri::command]
async fn run_ab_benchmark(
    request: benchmark::BenchmarkRequest,
    shared: State<'_, SharedState>,
    benchmark_state: State<'_, SharedBenchmark>,
) -> Result<benchmark::BenchmarkSession, String> {
    let runs_per_state = request.runs_per_state.clamp(3, 30);
    let settle_ms = request.settle_ms.clamp(250, 10_000);
    let initial = {
        let s = shared.lock().map_err(|e| e.to_string())?;
        (
            s.dome_active,
            s.current_workload.clone(),
            s.target_pid,
            s.policy_mode.as_name().to_string(),
        )
    };

    let mut samples = Vec::with_capacity(runs_per_state * 2);
    let started_at = chrono::Utc::now().to_rfc3339();

    let bench_result = async {
        for idx in 0..(runs_per_state * 2) {
            let phase = if idx % 2 == 0 {
                benchmark::BenchmarkPhase::Off
            } else {
                benchmark::BenchmarkPhase::On
            };

            match phase {
                benchmark::BenchmarkPhase::Off => {
                    rollback_dome_inner(&shared).await?;
                }
                benchmark::BenchmarkPhase::On => {
                    let activated = activate_dome_inner(
                        request.workload.clone(),
                        request.kappa,
                        request.sigma_max,
                        request.eta,
                        request.target_pid,
                        request.policy_mode.clone(),
                        &shared,
                    )
                    .await?;
                    if !activated.activated {
                        return Err(format!("benchmark ON phase blocked: {}", activated.message));
                    }
                }
            }

            tokio::time::sleep(Duration::from_millis(settle_ms)).await;

            let before = metrics::collect().ok();
            let probe = benchmark::execute_probe(
                request.command.clone(),
                request.args.clone(),
                request.cwd.clone(),
            )
            .await?;
            let after = metrics::collect().ok();
            let dome_active = shared.lock().map_err(|e| e.to_string())?.dome_active;

            samples.push(benchmark::BenchmarkSample {
                idx: idx + 1,
                phase,
                ts: chrono::Utc::now().to_rfc3339(),
                duration_ms: probe.duration_ms,
                success: probe.success,
                exit_code: probe.exit_code,
                dome_active,
                workload: request.workload.clone(),
                kappa: request.kappa,
                sigma_max: request.sigma_max,
                eta: request.eta,
                sigma_before: before.as_ref().map(|m| m.sigma),
                sigma_after: after.as_ref().map(|m| m.sigma),
                cpu_before_pct: before.as_ref().map(|m| m.raw.cpu_pct),
                cpu_after_pct: after.as_ref().map(|m| m.raw.cpu_pct),
                mem_before_gb: before.as_ref().map(|m| m.raw.mem_used_mb as f64 / 1024.0),
                mem_after_gb: after.as_ref().map(|m| m.raw.mem_used_mb as f64 / 1024.0),
                gpu_before_pct: before.as_ref().and_then(|m| m.raw.gpu_pct),
                gpu_after_pct: after.as_ref().and_then(|m| m.raw.gpu_pct),
                stdout_tail: probe.stdout_tail,
                stderr_tail: probe.stderr_tail,
            });
        }

        Ok::<_, String>(benchmark::BenchmarkSession {
            started_at,
            finished_at: chrono::Utc::now().to_rfc3339(),
            command: request.command.clone(),
            args: request.args.clone(),
            cwd: request.cwd.clone(),
            runs_per_state,
            settle_ms,
            workload: request.workload.clone(),
            kappa: request.kappa,
            sigma_max: request.sigma_max,
            eta: request.eta,
            target_pid: request.target_pid,
            policy_mode: request.policy_mode.clone(),
            summary: benchmark::compute_summary(&samples),
            samples,
        })
    }
    .await;

    match initial.0 {
        true => {
            let _ = activate_dome_inner(
                initial.1,
                request.kappa,
                request.sigma_max,
                request.eta,
                initial.2,
                Some(initial.3),
                &shared,
            )
            .await;
        }
        false => {
            let _ = rollback_dome_inner(&shared).await;
        }
    }

    if let Ok(session) = &bench_result {
        let mut history = benchmark_state.lock().map_err(|e| e.to_string())?;
        history.record_session(session.clone())?;
    }

    bench_result
}

#[tauri::command]
fn export_gains_to_file(content: String) -> Result<String, String> {
    let ts = now_ms_local() / 1000;
    let path = rfd::FileDialog::new()
        .add_filter("JSON", &["json"])
        .set_file_name(&format!("soulkernel_gains_{}.json", ts))
        .save_file()
        .ok_or_else(|| "Annule ou aucun chemin choisi".to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
fn export_benchmark_to_file(content: String) -> Result<String, String> {
    let ts = now_ms_local() / 1000;
    let path = rfd::FileDialog::new()
        .add_filter("JSON", &["json"])
        .set_file_name(&format!("soulkernel_benchmark_{}.json", ts))
        .save_file()
        .ok_or_else(|| "Annule ou aucun chemin choisi".to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

// ─── Telemetry ────────────────────────────────────────────────────────────────

fn default_telemetry_path() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return std::path::PathBuf::from(appdata)
            .join("SoulKernel")
            .join("telemetry")
            .join("energy_samples.jsonl");
    }

    std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("soulkernel_energy_samples.jsonl")
}

fn default_telemetry_pricing_path() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return std::path::PathBuf::from(appdata)
            .join("SoulKernel")
            .join("telemetry")
            .join("pricing.json");
    }

    std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("soulkernel_pricing.json")
}

fn default_telemetry_lifetime_path() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return std::path::PathBuf::from(appdata)
            .join("SoulKernel")
            .join("telemetry")
            .join("lifetime_gains.json");
    }

    std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("soulkernel_lifetime_gains.json")
}

fn default_benchmark_history_path() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return std::path::PathBuf::from(appdata)
            .join("SoulKernel")
            .join("benchmark")
            .join("ab_sessions.jsonl");
    }

    std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("soulkernel_ab_sessions.jsonl")
}

#[tauri::command]
fn ingest_telemetry_sample(
    sample: telemetry::TelemetryIngestRequest,
    telemetry_state: State<'_, SharedTelemetry>,
) -> Result<(), String> {
    let mut t = telemetry_state.lock().map_err(|e| e.to_string())?;
    t.ingest(sample)
}

#[tauri::command]
fn get_telemetry_summary(
    telemetry_state: State<'_, SharedTelemetry>,
) -> Result<telemetry::TelemetrySummary, String> {
    let t = telemetry_state.lock().map_err(|e| e.to_string())?;
    Ok(t.summary(telemetry::now_ms()))
}

#[tauri::command]
fn get_energy_pricing(
    telemetry_state: State<'_, SharedTelemetry>,
) -> Result<telemetry::EnergyPricing, String> {
    let t = telemetry_state.lock().map_err(|e| e.to_string())?;
    Ok(t.pricing())
}

#[tauri::command]
fn set_energy_pricing(
    pricing: telemetry::EnergyPricing,
    telemetry_state: State<'_, SharedTelemetry>,
) -> Result<(), String> {
    let mut t = telemetry_state.lock().map_err(|e| e.to_string())?;
    t.set_pricing(pricing)
}

#[tauri::command]
fn get_lifetime_gains(
    telemetry_state: State<'_, SharedTelemetry>,
) -> Result<telemetry::LifetimeGains, String> {
    let t = telemetry_state.lock().map_err(|e| e.to_string())?;
    Ok(t.lifetime())
}

#[derive(serde::Deserialize)]
struct BenchmarkHistoryQuery {
    command: Option<String>,
    args: Option<Vec<String>>,
    cwd: Option<String>,
    workload: Option<String>,
}

#[tauri::command]
fn get_benchmark_history(
    query: Option<BenchmarkHistoryQuery>,
    benchmark_state: State<'_, SharedBenchmark>,
) -> Result<benchmark::BenchmarkHistoryResponse, String> {
    let history = benchmark_state.lock().map_err(|e| e.to_string())?;
    let query = query.unwrap_or(BenchmarkHistoryQuery {
        command: None,
        args: None,
        cwd: None,
        workload: None,
    });
    Ok(history.history(
        query.command.as_deref(),
        query.args.as_deref(),
        query.cwd.as_deref(),
        query.workload.as_deref(),
    ))
}

#[tauri::command]
fn clear_benchmark_history(benchmark_state: State<'_, SharedBenchmark>) -> Result<(), String> {
    let mut history = benchmark_state.lock().map_err(|e| e.to_string())?;
    history.clear()
}

/// Chemins des fichiers persistants (télémétrie, lifetime, benchmark, audit) pour preuve / audit externe.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceDataPaths {
    pub telemetry_samples_jsonl: String,
    pub lifetime_gains_json: String,
    pub energy_pricing_json: String,
    pub benchmark_sessions_jsonl: String,
    pub audit_log_jsonl: String,
}

#[tauri::command]
fn get_evidence_data_paths(audit: State<'_, SharedAudit>) -> Result<EvidenceDataPaths, String> {
    let audit_log_jsonl = {
        let mut g = audit.lock().map_err(|e| e.to_string())?;
        if g.path.is_none() {
            g.path = Some(default_audit_path());
        }
        g.path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .ok_or_else(|| "audit path unavailable".to_string())?
    };
    Ok(EvidenceDataPaths {
        telemetry_samples_jsonl: default_telemetry_path().to_string_lossy().into_owned(),
        lifetime_gains_json: default_telemetry_lifetime_path().to_string_lossy().into_owned(),
        energy_pricing_json: default_telemetry_pricing_path().to_string_lossy().into_owned(),
        benchmark_sessions_jsonl: default_benchmark_history_path().to_string_lossy().into_owned(),
        audit_log_jsonl,
    })
}

/// Création / affichage / fermeture / rechargement des fenêtres WebView (HUD).
/// **Doit** s’exécuter sur le thread UI de l’app : WebView2 (Windows) et wry attendent le main thread
/// pour les opérations COM / HWND (cf. raccourcis déjà marshalisés via `run_on_main_thread`).
fn soulkernel_hud_watchdog_tick(app: &tauri::AppHandle) {
    // Marge large : WebView2 / wry peut mettre >5 s avant le premier `set_system_hud_ready`.
    const HUD_STALE_MS: u64 = 12_000;
    const HUD_RELOAD_BASE_COOLDOWN_MS: u64 = 4000;
    const HUD_RELOAD_MAX_COOLDOWN_MS: u64 = 30000;
    const HUD_RELOAD_MAX: u32 = 6;

    let hud_state = app.state::<SharedHud>();
    let hs = match hud_state.lock() {
        Ok(v) => v,
        Err(_) => return,
    };
    if !hs.visible {
        return;
    }
    if app.get_webview_window("hud").is_none() {
        let _ = apply_hud_window_mode(app, &hs);
        if let Some(w) = app.get_webview_window("hud") {
            let _ = w.hide();
        }
        {
            let hh = app.state::<SharedHudHealth>();
            reset_hud_health_for_show(&*hh);
        }
        let audit = app.state::<SharedAudit>();
        let _ = audit_write(
            &*audit,
            "hud",
            "window-recreated",
            Some("warn"),
            Some(serde_json::json!({
                "preset": hs.preset,
                "interactive": hs.interactive,
                "display_index": hs.display_index
            })),
        );
        return;
    }

    let hud_health = app.state::<SharedHudHealth>();
    let mut health = match hud_health.lock() {
        Ok(v) => v,
        Err(_) => return,
    };
    let now = now_ms_local();
    let stale = now.saturating_sub(health.last_ready_ms) > HUD_STALE_MS;
    let exp = health.reload_count.min(3);
    let cooldown_ms = (HUD_RELOAD_BASE_COOLDOWN_MS.saturating_mul(1u64 << exp))
        .min(HUD_RELOAD_MAX_COOLDOWN_MS);
    let since_reload = now.saturating_sub(health.last_reload_ms);
    let cooldown_ok = since_reload > cooldown_ms;
    if stale && cooldown_ok && health.reload_count < HUD_RELOAD_MAX {
        if let Some(w) = app.get_webview_window("hud") {
            let _ = w.hide();
            let _ = w.close();
        }
        let _ = apply_hud_window_mode(app, &hs);
        if let Some(w) = app.get_webview_window("hud") {
            let _ = w.show();
        }
        let age_before_ready_ms = now.saturating_sub(health.last_ready_ms);
        health.last_reload_ms = now;
        health.reload_count = health.reload_count.saturating_add(1);
        // Sans ce « grace », last_ready reste vieux → stale au tick suivant → boucle de reload / crash WebView.
        health.last_ready_ms = now;
        let audit = app.state::<SharedAudit>();
        let _ = audit_write(
            &*audit,
            "hud",
            "window-hard-recreate",
            Some("warn"),
            Some(serde_json::json!({
                "reload_count": health.reload_count,
                "last_ready_age_ms": age_before_ready_ms,
                "cooldown_ms": cooldown_ms
            })),
        );
    } else if stale && !cooldown_ok {
        let audit = app.state::<SharedAudit>();
        let _ = audit_write(
            &*audit,
            "hud",
            "watchdog-skip-backoff",
            Some("info"),
            Some(serde_json::json!({
                "reload_count": health.reload_count,
                "since_reload_ms": since_reload,
                "cooldown_ms": cooldown_ms
            })),
        );
    }
}

// ─── Entry point ──────────────────────────────────────────────────────────────

fn main() {
    env_logger::init();

    #[cfg(target_os = "windows")]
    {
        match platform::ensure_admin_or_relaunch() {
            Ok(true) => {}
            Ok(false) => return,
            Err(e) => eprintln!("SoulKernel: admin relaunch failed: {}", e),
        }
    }

    let state = Arc::new(Mutex::new(SoulKernelState {
        dome_active: false,
        policy_mode: platform::PolicyMode::Privileged,
        snapshot_before_dome: None,
        current_workload: "es".to_string(),
        target_pid: None,
        soulram_active: false,
        soulram_percent: 20,
    }));

    let audit: SharedAudit = Arc::new(Mutex::new(AuditState {
        path: Some(default_audit_path()),
    }));

    let telemetry_state: SharedTelemetry = Arc::new(Mutex::new(telemetry::TelemetryState::new(
        default_telemetry_path(),
        default_telemetry_pricing_path(),
        default_telemetry_lifetime_path(),
    )));
    let benchmark_state: SharedBenchmark = Arc::new(Mutex::new(benchmark::BenchmarkState::new(
        default_benchmark_history_path(),
    )));

    let hud_state: SharedHud = Arc::new(Mutex::new(HudRuntimeState {
        visible: false,
        interactive: false,
        preset: "compact".to_string(),
        opacity: 0.82,
        display_index: None,
        size_mode: "screen".to_string(),
        screen_width_pct: 22.0,
        screen_height_pct: 28.0,
        manual_width: 420.0,
        manual_height: 260.0,
        visible_metrics: hud::default_visible_metrics(),
    }));

    let (hud_tx_ch, mut hud_rx_ch) = mpsc::unbounded_channel::<HudOverlayData>();
    let hud_tx_state: SharedHudTx = Arc::new(Mutex::new(Some(hud_tx_ch)));
    let hud_data_state: SharedHudData = Arc::new(Mutex::new(None));
    let hud_health_state: SharedHudHealth = Arc::new(Mutex::new(HudHealthState {
        last_ready_ms: now_ms_local(),
        last_reload_ms: 0,
        reload_count: 0,
        ready_count: 0,
    }));

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let app = app.clone();
            let _ = app.clone().run_on_main_thread(move || {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            });
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts(["alt+shift+h", "alt+shift+j"])
                .expect("valid shortcuts")
                .with_handler(|app, shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }
                    let toggle_hud = shortcut.matches(Modifiers::ALT | Modifiers::SHIFT, Code::KeyH);
                    let toggle_interactive =
                        shortcut.matches(Modifiers::ALT | Modifiers::SHIFT, Code::KeyJ);
                    if !toggle_hud && !toggle_interactive {
                        return;
                    }
                    let app = app.clone();
                    let _ = app.clone().run_on_main_thread(move || {
                        if toggle_hud {
                            let hud = app.state::<SharedHud>();
                            let mut hs = match hud.lock() {
                                Ok(v) => v,
                                Err(_) => return,
                            };
                            hs.visible = !hs.visible;
                            if hs.visible {
                                let hh = app.state::<SharedHudHealth>();
                                reset_hud_health_for_show(&*hh);
                                let _ = apply_hud_window_mode(&app, &hs);
                                if let Some(w) = app.get_webview_window("hud") {
                                    let _ = w.show();
                                }
                            } else if let Some(w) = app.get_webview_window("hud") {
                                let _ = w.hide();
                            }
                            let _ = app.emit("soulkernel://hud-state", hs.visible);
                        }
                        if toggle_interactive {
                            let hud = app.state::<SharedHud>();
                            let mut hs = match hud.lock() {
                                Ok(v) => v,
                                Err(_) => return,
                            };
                            hs.interactive = !hs.interactive;
                            let _ = apply_hud_window_mode(&app, &hs);
                            let _ = app.emit("soulkernel://hud-interactive", hs.interactive);
                        }
                    });
                })
                .build(),
        )
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .manage(audit)
        .manage(telemetry_state)
        .manage(benchmark_state)
        .manage(hud_state)
        .manage(hud_tx_state)
        .manage(hud_data_state)
        .manage(hud_health_state)
        .setup(move |app| {
            if let Some(w) = app.get_webview_window("hud") {
                let _ = w.hide();
                let _ = w.close();
                let audit = app.state::<SharedAudit>();
                let _ = audit_write(&*audit, "hud", "startup-orphan-cleanup", Some("warn"), None);
            }

            let app_handle = app.handle().clone();

            tauri::async_runtime::spawn(async move {
                let mut latest: Option<HudOverlayData> = None;
                let mut tick = tokio::time::interval(Duration::from_millis(110));
                loop {
                    tokio::select! {
                        recv = hud_rx_ch.recv() => {
                            if let Some(payload) = recv {
                                latest = Some(payload);
                            } else {
                                break;
                            }
                        }
                        _ = tick.tick() => {
                            if let Some(payload) = latest.take() {
                                let main = app_handle.clone();
                                let emit = app_handle.clone();
                                let _ = main.run_on_main_thread(move || {
                                    let _ = emit.emit_to("hud", "soulkernel://hud", payload);
                                });
                            }
                        }
                    }
                }
            });

            let app_handle2 = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut hb = tokio::time::interval(Duration::from_millis(1500));
                loop {
                    hb.tick().await;
                    let app = app_handle2.clone();
                    let _ = app.clone().run_on_main_thread(move || {
                        soulkernel_hud_watchdog_tick(&app);
                    });
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, WindowEvent::CloseRequested { .. }) {
                cleanup_hud_before_exit(&window.app_handle());
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_processes,
            get_metrics,
            compute_formula,
            activate_dome,
            rollback_dome,
            platform_info,
            get_snapshot_before_dome,
            export_gains_to_file,
            export_benchmark_to_file,
            set_soulram,
            get_soulram_status,
            set_policy_mode,
            get_policy_status,
            set_taskbar_gauge,
            run_kpi_probe,
            hud::list_displays,
            hud::open_system_hud,
            hud::close_system_hud,
            hud::set_system_hud_display,
            hud::set_system_hud_interactive,
            hud::set_system_hud_preset,
            hud::set_system_hud_presentation,
            hud::set_system_hud_data,
            hud::get_system_hud_data,
            hud::get_system_hud_config,
            hud::set_system_hud_ready,
            hud::set_hud_window_size,
            audit::audit_log_event,
            audit::get_audit_log_path,
            ingest_telemetry_sample,
            get_telemetry_summary,
            get_energy_pricing,
            set_energy_pricing,
            get_lifetime_gains,
            run_ab_benchmark,
            get_benchmark_history,
            clear_benchmark_history,
            get_evidence_data_paths,
        ])
        .run(tauri::generate_context!())
        .expect("SoulKernel: failed to start Tauri runtime");
}
