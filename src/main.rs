//! SoulKernel - Performance Dome orchestrator
//! Tauri entry point - wires frontend to hardware via invoke()

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod audit;
mod benchmark;
mod external_power;
mod formula;
mod hud;
mod memory_policy;
mod metrics;
mod orchestrator;
mod platform;
mod telemetry;
mod workload_catalog;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::{
    fs::OpenOptions,
    process::{Child, Command, Stdio},
};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_global_shortcut::{Code, Modifiers, ShortcutState};
use tokio::sync::mpsc;

use audit::{audit_write, default_audit_path, now_ms_local, AuditState, SharedAudit};
use hud::{
    apply_hud_window_mode, cleanup_hud_before_exit, reset_hud_health_for_show, HudHealthState,
    HudOverlayData, HudRuntimeState, SharedHud, SharedHudData, SharedHudHealth, SharedHudTx,
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
type SharedExternalBridge = Arc<Mutex<ExternalBridgeState>>;

pub struct ExternalBridgeState {
    pub child: Option<Child>,
    pub last_error: Option<String>,
    pub last_start_ts_ms: Option<u64>,
}

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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalBridgeStatusResponse {
    pub running: bool,
    pub pid: Option<u32>,
    pub last_error: Option<String>,
    pub last_start_ts_ms: Option<u64>,
    pub script_path: String,
    pub bridge_log_path: String,
}

fn resolve_meross_bridge_script(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let resource_candidate = app
        .path()
        .resource_dir()
        .ok()
        .map(|p| p.join("scripts").join("meross_mss315_bridge.py"));
    if let Some(path) = resource_candidate.filter(|p| p.exists()) {
        return Ok(path);
    }
    let cwd_candidate = std::env::current_dir()
        .ok()
        .map(|p| p.join("scripts").join("meross_mss315_bridge.py"));
    if let Some(path) = cwd_candidate.filter(|p| p.exists()) {
        return Ok(path);
    }
    Err("meross bridge script not found".to_string())
}

fn external_bridge_log_path() -> Result<std::path::PathBuf, String> {
    external_power::soulkernel_config_dir()
        .map(|p| p.join("meross_bridge.log"))
        .ok_or_else(|| "config dir unavailable".to_string())
}

fn effective_python_candidates(cfg: &external_power::MerossFileConfig) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(bin) = cfg
        .python_bin
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        out.push(bin.to_string());
    }
    #[cfg(target_os = "windows")]
    {
        out.push("py".to_string());
        out.push("python".to_string());
        out.push("python3".to_string());
    }
    #[cfg(not(target_os = "windows"))]
    {
        out.push("python3".to_string());
        out.push("python".to_string());
    }
    out.dedup();
    out
}

fn pick_python_bin(cfg: &external_power::MerossFileConfig) -> Result<String, String> {
    for candidate in effective_python_candidates(cfg) {
        if Command::new(&candidate)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Ok(candidate);
        }
    }
    Err("python introuvable (essayés: python3/python/py)".to_string())
}

fn refresh_bridge_process_state(bridge: &SharedExternalBridge) -> (bool, Option<u32>) {
    let mut g = match bridge.lock() {
        Ok(v) => v,
        Err(_) => return (false, None),
    };
    if let Some(child) = g.child.as_mut() {
        match child.try_wait() {
            Ok(Some(status)) => {
                g.last_error = Some(format!("bridge arrêté ({status})"));
                g.child = None;
                (false, None)
            }
            Ok(None) => (true, Some(child.id())),
            Err(e) => {
                g.last_error = Some(format!("bridge status error: {e}"));
                g.child = None;
                (false, None)
            }
        }
    } else {
        (false, None)
    }
}

fn external_bridge_status(
    app: &AppHandle,
    bridge: &SharedExternalBridge,
) -> ExternalBridgeStatusResponse {
    let (running, pid) = refresh_bridge_process_state(bridge);
    let (last_error, last_start_ts_ms) = bridge
        .lock()
        .map(|g| (g.last_error.clone(), g.last_start_ts_ms))
        .unwrap_or((Some("bridge state poisoned".to_string()), None));
    ExternalBridgeStatusResponse {
        running,
        pid,
        last_error,
        last_start_ts_ms,
        script_path: resolve_meross_bridge_script(app)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        bridge_log_path: external_bridge_log_path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

fn stop_external_bridge_inner(bridge: &SharedExternalBridge) -> Result<(), String> {
    let mut g = bridge.lock().map_err(|e| e.to_string())?;
    if let Some(mut child) = g.child.take() {
        child.kill().map_err(|e| e.to_string())?;
        let _ = child.wait();
    }
    Ok(())
}

fn set_bridge_error(bridge: &SharedExternalBridge, message: String) {
    if let Ok(mut g) = bridge.lock() {
        g.last_error = Some(message);
    }
}

fn start_external_bridge_inner(
    app: &AppHandle,
    bridge: &SharedExternalBridge,
) -> Result<(), String> {
    let cfg = external_power::get_meross_config_or_default();
    if !cfg.enabled {
        return Err("active d'abord la source puissance externe".to_string());
    }
    let email = cfg
        .meross_email
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "MEROSS email manquant".to_string())?;
    let password = cfg
        .meross_password
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "MEROSS password manquant".to_string())?;
    let region = cfg
        .meross_region
        .as_deref()
        .unwrap_or("eu")
        .trim()
        .to_string();
    let device_type = cfg
        .meross_device_type
        .as_deref()
        .unwrap_or("mss315")
        .trim()
        .to_string();
    let interval = cfg.bridge_interval_s.unwrap_or(8.0).clamp(2.0, 300.0);
    let python_bin = pick_python_bin(&cfg)?;
    let script_path = resolve_meross_bridge_script(app)?;
    let out_path = cfg
        .power_file
        .as_ref()
        .map(std::path::PathBuf::from)
        .or_else(external_power::default_power_file)
        .ok_or_else(|| "power file path unavailable".to_string())?;
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let log_path = external_bridge_log_path()?;
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| e.to_string())?;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| e.to_string())?;

    let mut g = bridge.lock().map_err(|e| e.to_string())?;
    if let Some(child) = g.child.as_mut() {
        if child.try_wait().map_err(|e| e.to_string())?.is_none() {
            return Ok(());
        }
        g.child = None;
    }

    let mut cmd = Command::new(&python_bin);
    cmd.arg(script_path)
        .arg("--out")
        .arg(out_path)
        .arg("--interval")
        .arg(format!("{interval:.1}"))
        .env("MEROSS_EMAIL", email)
        .env("MEROSS_PASSWORD", password)
        .env("MEROSS_REGION", &region)
        .env("MEROSS_DEVICE_TYPE", &device_type)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .stdin(Stdio::null());
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    g.last_error = None;
    g.last_start_ts_ms = Some(now_ms_local());
    g.child = Some(child);
    Ok(())
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
fn list_workload_scenes() -> Vec<workload_catalog::WorkloadSceneDto> {
    workload_catalog::list_scenes_for_ui()
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
    if command.trim().eq_ignore_ascii_case("system") {
        let dur = args
            .get(0)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(4000);
        return benchmark::execute_system_probe(dur).await;
    }
    benchmark::execute_probe(command, args, cwd).await
}

fn emit_benchmark_progress(app: &AppHandle, payload: serde_json::Value) {
    let _ = app.emit("soulkernel://benchmark-progress", payload);
}

#[tauri::command]
async fn run_ab_benchmark(
    request: benchmark::BenchmarkRequest,
    app: AppHandle,
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
            s.soulram_percent,
        )
    };

    let mut samples = Vec::with_capacity(runs_per_state * 2);
    let started_at = chrono::Utc::now().to_rfc3339();
    let bench_profile = formula::WorkloadProfile::from_name(&request.workload)
        .ok_or_else(|| format!("Unknown workload for benchmark: {}", request.workload))?;

    let total_steps = runs_per_state * 2;
    emit_benchmark_progress(
        &app,
        serde_json::json!({
            "current": 0,
            "total": total_steps,
            "phase": null,
            "step": "start",
            "progress_percent": 0.0,
            "message": format!("Démarrage A/B — {} échantillons ({}× OFF + {}× ON)", total_steps, runs_per_state, runs_per_state),
        }),
    );

    let bench_result = async {
        for idx in 0..total_steps {
            let phase = if idx % 2 == 0 {
                benchmark::BenchmarkPhase::Off
            } else {
                benchmark::BenchmarkPhase::On
            };

            let phase_str = if matches!(phase, benchmark::BenchmarkPhase::Off) {
                "off"
            } else {
                "on"
            };
            let phase_label = if matches!(phase, benchmark::BenchmarkPhase::Off) {
                "Dôme OFF"
            } else {
                "Dôme ON"
            };
            let pct_dome = ((idx as f64 + 0.22) / total_steps as f64 * 100.0).min(100.0);

            emit_benchmark_progress(
                &app,
                serde_json::json!({
                    "current": idx + 1,
                    "total": total_steps,
                    "phase": phase_str,
                    "step": "dome",
                    "progress_percent": pct_dome,
                    "message": format!("{} — échantillon {}/{} — réglage noyau ({})", phase_label, idx + 1, total_steps, if matches!(phase, benchmark::BenchmarkPhase::Off) { "rollback" } else { "activation dôme" }),
                }),
            );

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

            let pct_settle = ((idx as f64 + 0.48) / total_steps as f64 * 100.0).min(100.0);
            emit_benchmark_progress(
                &app,
                serde_json::json!({
                    "current": idx + 1,
                    "total": total_steps,
                    "phase": phase_str,
                    "step": "settle",
                    "progress_percent": pct_settle,
                    "message": format!("Stabilisation {} ms avant mesure…", settle_ms),
                }),
            );

            tokio::time::sleep(Duration::from_millis(settle_ms)).await;

            let pct_probe = ((idx as f64 + 0.72) / total_steps as f64 * 100.0).min(100.0);
            emit_benchmark_progress(
                &app,
                serde_json::json!({
                    "current": idx + 1,
                    "total": total_steps,
                    "phase": phase_str,
                    "step": "probe",
                    "progress_percent": pct_probe,
                    "message": format!(
                        "Sonde KPI : `{}` {}",
                        request.command,
                        request.args.join(" ")
                    ),
                }),
            );

            let before = metrics::collect().ok();
            let probe = if request.command.trim().eq_ignore_ascii_case("system") {
                let dur = request
                    .args
                    .get(0)
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(4000);
                benchmark::execute_system_probe(dur).await?
            } else {
                benchmark::execute_probe(
                    request.command.clone(),
                    request.args.clone(),
                    request.cwd.clone(),
                )
                .await?
            };
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
                io_before_mb_s: before.as_ref().and_then(|m| {
                    m.raw
                        .io_read_mb_s
                        .zip(m.raw.io_write_mb_s)
                        .map(|(r, w)| r + w)
                }),
                io_after_mb_s: after.as_ref().and_then(|m| {
                    m.raw
                        .io_read_mb_s
                        .zip(m.raw.io_write_mb_s)
                        .map(|(r, w)| r + w)
                }),
                power_before_watts: before.as_ref().and_then(|m| m.raw.power_watts),
                power_after_watts: after.as_ref().and_then(|m| m.raw.power_watts),
                cpu_temp_before_c: before.as_ref().and_then(|m| m.raw.cpu_temp_c),
                cpu_temp_after_c: after.as_ref().and_then(|m| m.raw.cpu_temp_c),
                gpu_temp_before_c: before.as_ref().and_then(|m| m.raw.gpu_temp_c),
                gpu_temp_after_c: after.as_ref().and_then(|m| m.raw.gpu_temp_c),
                sigma_effective_before: before
                    .as_ref()
                    .map(|m| formula::compute(m, &bench_profile, request.kappa).sigma_effective),
                sigma_effective_after: after
                    .as_ref()
                    .map(|m| formula::compute(m, &bench_profile, request.kappa).sigma_effective),
                stdout_tail: probe.stdout_tail.clone(),
                stderr_tail: probe.stderr_tail.clone(),
            });

            let pct_done = ((idx + 1) as f64 / total_steps as f64 * 100.0).min(100.0);
            let stdout_short: String = probe.stdout_tail.chars().take(320).collect();
            let stderr_short: String = probe.stderr_tail.chars().take(320).collect();
            emit_benchmark_progress(
                &app,
                serde_json::json!({
                    "current": idx + 1,
                    "total": total_steps,
                    "phase": phase_str,
                    "step": "done",
                    "progress_percent": pct_done,
                    "message": format!(
                        "Échantillon {}/{} terminé — {} ms — succès={} exit={:?}",
                        idx + 1,
                        total_steps,
                        probe.duration_ms,
                        probe.success,
                        probe.exit_code
                    ),
                    "probe_duration_ms": probe.duration_ms,
                    "probe_ok": probe.success,
                    "stdout_tail": stdout_short,
                    "stderr_tail": stderr_short,
                }),
            );
        }

        emit_benchmark_progress(
            &app,
            serde_json::json!({
                "current": total_steps,
                "total": total_steps,
                "phase": null,
                "step": "aggregate",
                "progress_percent": 100.0,
                "message": "Agrégation des métriques et verdict…",
                "finished": true,
                "ok": true,
            }),
        );

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
            soulram_percent: request.soulram_percent.or(Some(initial.4)),
            summary: benchmark::compute_summary(&samples),
            samples,
        })
    }
    .await;

    if let Err(ref e) = bench_result {
        emit_benchmark_progress(
            &app,
            serde_json::json!({
                "current": null,
                "total": null,
                "phase": null,
                "step": "error",
                "progress_percent": 0.0,
                "message": format!("Échec benchmark: {}", e),
                "finished": true,
                "ok": false,
            }),
        );
    }

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

#[tauri::command]
fn get_external_power_config() -> Result<external_power::MerossFileConfig, String> {
    Ok(external_power::get_meross_config_or_default())
}

#[tauri::command]
fn set_external_power_config(config: external_power::MerossFileConfig) -> Result<(), String> {
    external_power::save_meross_config(&config)
}

#[tauri::command]
fn get_external_power_status() -> Result<external_power::ExternalPowerStatus, String> {
    Ok(external_power::get_external_power_status())
}

#[tauri::command]
fn get_external_bridge_status(
    app: AppHandle,
    bridge: State<'_, SharedExternalBridge>,
) -> Result<ExternalBridgeStatusResponse, String> {
    Ok(external_bridge_status(&app, &bridge))
}

#[tauri::command]
fn start_external_bridge(
    app: AppHandle,
    bridge: State<'_, SharedExternalBridge>,
) -> Result<ExternalBridgeStatusResponse, String> {
    if let Err(e) = start_external_bridge_inner(&app, &bridge) {
        set_bridge_error(&bridge, e.clone());
        return Err(e);
    }
    Ok(external_bridge_status(&app, &bridge))
}

#[tauri::command]
fn stop_external_bridge(
    app: AppHandle,
    bridge: State<'_, SharedExternalBridge>,
) -> Result<ExternalBridgeStatusResponse, String> {
    stop_external_bridge_inner(&bridge)?;
    Ok(external_bridge_status(&app, &bridge))
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
        lifetime_gains_json: default_telemetry_lifetime_path()
            .to_string_lossy()
            .into_owned(),
        energy_pricing_json: default_telemetry_pricing_path()
            .to_string_lossy()
            .into_owned(),
        benchmark_sessions_jsonl: default_benchmark_history_path()
            .to_string_lossy()
            .into_owned(),
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
    let cooldown_ms =
        (HUD_RELOAD_BASE_COOLDOWN_MS.saturating_mul(1u64 << exp)).min(HUD_RELOAD_MAX_COOLDOWN_MS);
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
    let external_bridge_state: SharedExternalBridge = Arc::new(Mutex::new(ExternalBridgeState {
        child: None,
        last_error: None,
        last_start_ts_ms: None,
    }));

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
                    let toggle_hud =
                        shortcut.matches(Modifiers::ALT | Modifiers::SHIFT, Code::KeyH);
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
        .manage(external_bridge_state)
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

            {
                let app_handle = app.handle().clone();
                let bridge = app.state::<SharedExternalBridge>().inner().clone();
                tauri::async_runtime::spawn(async move {
                    let cfg = external_power::get_meross_config_or_default();
                    if cfg.enabled && cfg.autostart_bridge {
                        if let Err(e) = start_external_bridge_inner(&app_handle, &bridge) {
                            set_bridge_error(&bridge, e);
                        }
                    }
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, WindowEvent::CloseRequested { .. }) {
                let bridge = window
                    .app_handle()
                    .state::<SharedExternalBridge>()
                    .inner()
                    .clone();
                let _ = stop_external_bridge_inner(&bridge);
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
            list_workload_scenes,
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
            get_external_power_config,
            set_external_power_config,
            get_external_power_status,
            get_external_bridge_status,
            start_external_bridge,
            stop_external_bridge,
            run_ab_benchmark,
            get_benchmark_history,
            clear_benchmark_history,
            get_evidence_data_paths,
        ])
        .run(tauri::generate_context!())
        .expect("SoulKernel: failed to start Tauri runtime");
}
