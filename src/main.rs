//! SoulKernel - Performance Dome orchestrator
//! Tauri entry point - wires frontend to hardware via invoke()

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod formula;
mod metrics;
mod orchestrator;
mod platform;
mod telemetry;

use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager, State, WindowEvent};
use tauri::PhysicalPosition;
use tokio::sync::mpsc;
use tauri_plugin_global_shortcut::{Code, Modifiers, ShortcutState};

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
type SharedAudit = Arc<Mutex<AuditState>>;
type SharedTelemetry = Arc<Mutex<telemetry::TelemetryState>>;
type SharedHud = Arc<Mutex<HudRuntimeState>>;
type SharedHudTx = Arc<Mutex<Option<mpsc::UnboundedSender<HudOverlayData>>>>;
type SharedHudData = Arc<Mutex<Option<HudOverlayData>>>;
type SharedHudHealth = Arc<Mutex<HudHealthState>>;

#[derive(Default)]
pub struct AuditState {
    pub path: Option<std::path::PathBuf>,
}

pub struct HudRuntimeState {
    pub visible: bool,
    pub interactive: bool,
    pub preset: String,
    pub opacity: f64,
    pub display_index: Option<usize>,
}
pub struct HudHealthState {
    pub last_ready_ms: u64,
    pub last_reload_ms: u64,
    pub reload_count: u32,
    pub ready_count: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DisplayInfo {
    pub index: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub scale_factor: f64,
    pub is_primary: bool,
}

#[derive(serde::Serialize)]
struct AuditEntry {
    ts_ms: u64,
    category: String,
    action: String,
    level: Option<String>,
    data: Option<serde_json::Value>,
}

static AUDIT_FILE: OnceLock<Mutex<std::fs::File>> = OnceLock::new();

fn now_ms_local() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
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
}

#[derive(serde::Serialize)]
pub struct KpiProbeResult {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub duration_ms: u64,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout_tail: String,
    pub stderr_tail: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HudOverlayData {
    pub dome: String,
    pub sigma: String,
    pub pi: String,
    pub cpu: String,
    pub ram: String,
    pub target: String,
    pub power: String,
    pub energy: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HudConfigEvent {
    pub preset: String,
    pub interactive: bool,
    pub opacity: f64,
}

fn preset_to_size(preset: &str) -> (f64, f64) {
    match preset {
        "mini" => (280.0, 148.0),
        "detailed" => (460.0, 280.0),
        _ => (360.0, 210.0),
    }
}

fn list_displays_internal(app: &tauri::AppHandle) -> Result<Vec<DisplayInfo>, String> {
    let primary_name = app.primary_monitor().map_err(|e| e.to_string())?.and_then(|m| m.name().cloned());
    let mons = app.available_monitors().map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(mons.len());
    for (idx, m) in mons.iter().enumerate() {
        let name = m.name().cloned().unwrap_or_else(|| format!("Display {}", idx + 1));
        out.push(DisplayInfo {
            index: idx,
            name: name.clone(),
            width: m.size().width,
            height: m.size().height,
            x: m.position().x,
            y: m.position().y,
            scale_factor: m.scale_factor(),
            is_primary: primary_name.as_ref().map(|n| n.as_str() == name.as_str()).unwrap_or(false),
        });
    }
    Ok(out)
}

fn pick_display(app: &tauri::AppHandle, index: Option<usize>) -> Result<Option<DisplayInfo>, String> {
    let list = list_displays_internal(app)?;
    if list.is_empty() { return Ok(None); }
    if let Some(i) = index {
        if let Some(d) = list.iter().find(|d| d.index == i) { return Ok(Some(d.clone())); }
    }
    if let Some(d) = list.iter().find(|d| d.is_primary) { return Ok(Some(d.clone())); }
    Ok(list.into_iter().next())
}

fn ensure_hud_window(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow, String> {
    if let Some(w) = app.get_webview_window("hud") {
        return Ok(w);
    }
    let url = tauri::WebviewUrl::App("hud.html".into());
    tauri::WebviewWindowBuilder::new(app, "hud", url)
        .title("SoulKernel HUD")        .initialization_script(
            r#"
            (() => {
              try {
                const s = document.createElement('style');
                s.textContent = 'html,body{background:#0b1320 !important;color:#9dbad6;font:12px monospace;}';
                document.documentElement.appendChild(s);
                const p = document.createElement('div');
                p.id = '__sk_hud_boot';
                p.textContent = 'SoulKernel HUD booting...';
                p.style.cssText = 'position:fixed;left:8px;top:8px;z-index:2147483647;opacity:.8';
                document.documentElement.appendChild(p);
                setTimeout(() => { try { p.remove(); } catch (_) {} }, 3000);
              } catch (_) {}
            })();
            "#,
        )
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false)
        .transparent(false)
        .inner_size(360.0, 210.0)
        .position(14.0, 58.0)
        .build()
        .map_err(|e| e.to_string())
}

fn apply_hud_window_mode(
    app: &tauri::AppHandle,
    interactive: bool,
    preset: &str,
    opacity: f64,
    display_index: Option<usize>,
) -> Result<(), String> {
    let w = ensure_hud_window(app)?;
    let (width, height) = preset_to_size(preset);
    let clamped_opacity = opacity.clamp(0.3, 1.0);
    if let Ok(Some(display)) = pick_display(app, display_index) {
        let x = display.x + 14;
        let y = display.y + 58;
        let _ = w.set_position(tauri::Position::Physical(PhysicalPosition::new(x, y)));
    }
    let _ = w.set_always_on_top(true);
    let _ = w.set_ignore_cursor_events(!interactive);
    let _ = w.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)));
    let _ = w.set_opacity(clamped_opacity);
    let _ = w.emit(
        "soulkernel://hud-config",
        HudConfigEvent {
            preset: preset.to_string(),
            interactive,
            opacity: clamped_opacity,
        },
    );
    Ok(())
}

fn cleanup_hud_before_exit(app: &tauri::AppHandle) {
    if let Some(audit) = app.try_state::<SharedAudit>() {
        let _ = audit_write(&audit, "hud", "exit-cleanup", Some("info"), None);
    }
    if let Some(hud_state) = app.try_state::<SharedHud>() {
        if let Ok(mut hs) = hud_state.lock() {
            hs.visible = false;
        }
    }
    if let Some(w) = app.get_webview_window("hud") {
        let _ = w.hide();
        let _ = w.close();
    }
}
#[tauri::command]
fn open_system_hud(
    app: tauri::AppHandle,
    hud: State<'_, SharedHud>,
    hud_health: State<'_, SharedHudHealth>,
    audit: State<'_, SharedAudit>,
) -> Result<(), String> {
    {
        let mut hs = hud.lock().map_err(|e| e.to_string())?;
        hs.visible = true;
    }
    {
        let mut health = hud_health.lock().map_err(|e| e.to_string())?;
        let now = now_ms_local();
        health.last_ready_ms = now;
        health.last_reload_ms = 0;
        health.reload_count = 0;
    }
    let hs = hud.lock().map_err(|e| e.to_string())?;
    apply_hud_window_mode(&app, hs.interactive, &hs.preset, hs.opacity, hs.display_index)?;
    if let Some(w) = app.get_webview_window("hud") {
        let _ = w.show();
    }
    let _ = audit_write(
        &audit,
        "hud",
        "open",
        Some("info"),
        Some(serde_json::json!({
            "preset": hs.preset,
            "interactive": hs.interactive,
            "opacity": hs.opacity,
            "display_index": hs.display_index
        })),
    );
    Ok(())
}

#[tauri::command]
fn close_system_hud(
    app: tauri::AppHandle,
    hud: State<'_, SharedHud>,
    audit: State<'_, SharedAudit>,
) -> Result<(), String> {
    {
        let mut hs = hud.lock().map_err(|e| e.to_string())?;
        hs.visible = false;
    }
    if let Some(w) = app.get_webview_window("hud") {
        w.hide().map_err(|e| e.to_string())?;
    }
    let _ = audit_write(&audit, "hud", "close", Some("info"), None);
    Ok(())
}

#[tauri::command]
fn set_system_hud_interactive(
    app: tauri::AppHandle,
    interactive: bool,
    hud: State<'_, SharedHud>,
    audit: State<'_, SharedAudit>,
) -> Result<(), String> {
    {
        let mut hs = hud.lock().map_err(|e| e.to_string())?;
        hs.interactive = interactive;
    }
    let hs = hud.lock().map_err(|e| e.to_string())?;
    apply_hud_window_mode(&app, hs.interactive, &hs.preset, hs.opacity, hs.display_index)?;
    let _ = audit_write(
        &audit,
        "hud",
        "interactive",
        Some("info"),
        Some(serde_json::json!({ "interactive": hs.interactive })),
    );
    Ok(())
}

#[tauri::command]
fn set_system_hud_preset(
    app: tauri::AppHandle,
    preset: String,
    opacity: f64,
    hud: State<'_, SharedHud>,
    audit: State<'_, SharedAudit>,
) -> Result<(), String> {
    {
        let mut hs = hud.lock().map_err(|e| e.to_string())?;
        hs.preset = match preset.as_str() {
            "mini" | "compact" | "detailed" => preset,
            _ => "compact".to_string(),
        };
        hs.opacity = opacity.clamp(0.3, 1.0);
    }
    let hs = hud.lock().map_err(|e| e.to_string())?;
    apply_hud_window_mode(&app, hs.interactive, &hs.preset, hs.opacity, hs.display_index)?;
    let _ = audit_write(
        &audit,
        "hud",
        "preset",
        Some("info"),
        Some(serde_json::json!({
            "preset": hs.preset,
            "opacity": hs.opacity
        })),
    );
    Ok(())
}

#[tauri::command]
fn set_system_hud_data(
    app: tauri::AppHandle,
    payload: HudOverlayData,
    hud_tx: State<'_, SharedHudTx>,
    hud_data: State<'_, SharedHudData>,
) -> Result<(), String> {
    {
        let mut latest = hud_data.lock().map_err(|e| e.to_string())?;
        *latest = Some(payload.clone());
    }
    if let Some(tx) = hud_tx.lock().map_err(|e| e.to_string())?.as_ref() {
        let _ = tx.send(payload);
        return Ok(());
    }
    if app.get_webview_window("hud").is_some() {
        app.emit_to("hud", "soulkernel://hud", payload)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn get_system_hud_data(hud_data: State<'_, SharedHudData>) -> Result<Option<HudOverlayData>, String> {
    let data = hud_data.lock().map_err(|e| e.to_string())?;
    Ok(data.clone())
}

#[tauri::command]
fn get_system_hud_config(hud: State<'_, SharedHud>) -> Result<HudConfigEvent, String> {
    let hs = hud.lock().map_err(|e| e.to_string())?;
    Ok(HudConfigEvent {
        preset: hs.preset.clone(),
        interactive: hs.interactive,
        opacity: hs.opacity,
    })
}


#[tauri::command]
fn set_system_hud_ready(
    app: tauri::AppHandle,
    ts_ms: Option<u64>,
    hud_health: State<'_, SharedHudHealth>,
    hud: State<'_, SharedHud>,
    audit: State<'_, SharedAudit>,
) -> Result<(), String> {
    let now = ts_ms.unwrap_or_else(now_ms_local);
    let mut health = hud_health.lock().map_err(|e| e.to_string())?;
    health.last_ready_ms = now;
    health.ready_count = health.ready_count.saturating_add(1);

    let hs = hud.lock().map_err(|e| e.to_string())?;
    if hs.visible {
        if let Some(w) = app.get_webview_window("hud") {
            let was_visible = w.is_visible().unwrap_or(false);
            let _ = w.show();
            if !was_visible {
                let _ = audit_write(
                    &audit,
                    "hud",
                    "shown-after-ready",
                    Some("info"),
                    Some(serde_json::json!({
                        "ready_count": health.ready_count
                    })),
                );
            }
        }
    }

    if health.reload_count > 0 {
        let _ = audit_write(
            &audit,
            "hud",
            "recovered",
            Some("info"),
            Some(serde_json::json!({ "reload_count": health.reload_count })),
        );
        health.reload_count = 0;
    }
    Ok(())
}
#[tauri::command]
fn list_displays(app: tauri::AppHandle) -> Result<Vec<DisplayInfo>, String> {
    list_displays_internal(&app)
}

#[tauri::command]
fn set_system_hud_display(
    app: tauri::AppHandle,
    display_index: Option<usize>,
    hud: State<'_, SharedHud>,
    audit: State<'_, SharedAudit>,
) -> Result<(), String> {
    {
        let mut hs = hud.lock().map_err(|e| e.to_string())?;
        hs.display_index = display_index;
    }
    let hs = hud.lock().map_err(|e| e.to_string())?;
    apply_hud_window_mode(
        &app,
        hs.interactive,
        &hs.preset,
        hs.opacity,
        hs.display_index,
    )?;
    let _ = audit_write(
        &audit,
        "hud",
        "display",
        Some("info"),
        Some(serde_json::json!({ "display_index": hs.display_index })),
    );
    Ok(())
}
#[tauri::command]
fn list_processes() -> Result<Vec<ProcessInfo>, String> {
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();
    let mut list: Vec<ProcessInfo> = sys
        .processes()
        .iter()
        .map(|(pid, p)| ProcessInfo {
            pid: pid.as_u32(),
            name: p.name().to_string(),
            cpu_usage: p.cpu_usage() as f64,
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
        let activated = actions.iter().any(|(_, ok)| *ok);
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

fn tail_text(buf: &[u8], max_chars: usize) -> String {
    let s = String::from_utf8_lossy(buf);
    let v: Vec<char> = s.chars().collect();
    let start = v.len().saturating_sub(max_chars);
    v[start..].iter().collect::<String>()
}

#[tauri::command]
async fn run_kpi_probe(
    command: String,
    args: Vec<String>,
    cwd: Option<String>,
) -> Result<KpiProbeResult, String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err("kpi command is empty".to_string());
    }

    let mut cmd = tokio::process::Command::new(trimmed);
    cmd.args(args.clone());
    if let Some(c) = cwd.as_ref() {
        let ctrim = c.trim();
        if !ctrim.is_empty() {
            cmd.current_dir(ctrim);
        }
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let start = Instant::now();
    let out = cmd.output().await.map_err(|e| e.to_string())?;
    let duration_ms = start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

    Ok(KpiProbeResult {
        command: trimmed.to_string(),
        args,
        cwd,
        duration_ms,
        success: out.status.success(),
        exit_code: out.status.code(),
        stdout_tail: tail_text(&out.stdout, 600),
        stderr_tail: tail_text(&out.stderr, 600),
    })
}

#[tauri::command]
fn export_gains_to_file(content: String) -> Result<String, String> {
    let path = rfd::FileDialog::new()
        .add_filter("JSON", &["json"])
        .set_file_name(&format!("soulkernel_gains_{}.json", {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        }))
        .save_file()
        .ok_or_else(|| "Annule ou aucun chemin choisi".to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}


fn default_audit_path() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return std::path::PathBuf::from(appdata)
            .join("SoulKernel")
            .join("audit")
            .join("soulkernel_audit.jsonl");
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            return std::path::PathBuf::from(xdg)
                .join("SoulKernel")
                .join("audit")
                .join("soulkernel_audit.jsonl");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("SoulKernel")
                .join("audit")
                .join("soulkernel_audit.jsonl");
        }
    }

    std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("soulkernel_audit.jsonl")
}



fn ensure_audit_file(audit: &State<'_, SharedAudit>) -> Result<&'static Mutex<std::fs::File>, String> {
    if AUDIT_FILE.get().is_none() {
        let mut guard = audit.lock().map_err(|e| e.to_string())?;
        if guard.path.is_none() {
            guard.path = Some(default_audit_path());
        }
        let path = guard
            .path
            .as_ref()
            .cloned()
            .ok_or_else(|| "audit path unavailable".to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let _ = AUDIT_FILE.set(Mutex::new(file));
    }
    AUDIT_FILE
        .get()
        .ok_or_else(|| "audit logger init failed".to_string())
}
fn audit_write(
    audit: &State<'_, SharedAudit>,
    category: &str,
    action: &str,
    level: Option<&str>,
    data: Option<serde_json::Value>,
) -> Result<(), String> {
    let file_mutex = ensure_audit_file(audit)?;
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0);
    let entry = AuditEntry {
        ts_ms,
        category: category.to_string(),
        action: action.to_string(),
        level: level.map(|s| s.to_string()),
        data,
    };
    let line = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
    let mut file = file_mutex.lock().map_err(|e| e.to_string())?;
    use std::io::Write;
    writeln!(file, "{}", line).map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
fn get_audit_log_path(audit: State<'_, SharedAudit>) -> Result<String, String> {
    {
        let mut g = audit.lock().map_err(|e| e.to_string())?;
        if g.path.is_none() {
            g.path = Some(default_audit_path());
        }
    }
    let _ = ensure_audit_file(&audit)?;
    let g = audit.lock().map_err(|e| e.to_string())?;
    g.path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| "audit path unavailable".to_string())
}

#[tauri::command]
fn audit_log_event(
    category: String,
    action: String,
    level: Option<String>,
    data: Option<serde_json::Value>,
    audit: State<'_, SharedAudit>,
) -> Result<(), String> {
    audit_write(&audit, &category, &action, level.as_deref(), data)
}

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

#[tauri::command]
fn ingest_telemetry_sample(
    sample: telemetry::TelemetryIngestRequest,
    telemetry_state: State<'_, SharedTelemetry>,
) -> Result<(), String> {
    let mut t = telemetry_state.lock().map_err(|e| e.to_string())?;
    t.ingest(sample)
}

#[tauri::command]
fn get_telemetry_summary(telemetry_state: State<'_, SharedTelemetry>) -> Result<telemetry::TelemetrySummary, String> {
    let t = telemetry_state.lock().map_err(|e| e.to_string())?;
    Ok(t.summary(telemetry::now_ms()))
}

#[tauri::command]
fn get_energy_pricing(telemetry_state: State<'_, SharedTelemetry>) -> Result<telemetry::EnergyPricing, String> {
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

    let audit = Arc::new(Mutex::new(AuditState {
        path: Some(default_audit_path()),
    }));

    let telemetry_state = Arc::new(Mutex::new(telemetry::TelemetryState::new(
        default_telemetry_path(),
        default_telemetry_pricing_path(),
    )));

    let hud_state = Arc::new(Mutex::new(HudRuntimeState {
        visible: false,
        interactive: false,
        preset: "compact".to_string(),
        opacity: 0.82,
        display_index: None,
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
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts(["alt+shift+h", "alt+shift+j"])
                .expect("valid shortcuts")
                .with_handler(|app, shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }
                    let hud = app.state::<SharedHud>();
                    if shortcut.matches(Modifiers::ALT | Modifiers::SHIFT, Code::KeyH) {
                        let mut hs = match hud.lock() {
                            Ok(v) => v,
                            Err(_) => return,
                        };
                        hs.visible = !hs.visible;
                        if hs.visible {
                            let _ = apply_hud_window_mode(app, hs.interactive, &hs.preset, hs.opacity, hs.display_index);
                            if let Some(w) = app.get_webview_window("hud") {
                                let _ = w.show();
                            }
                        } else if let Some(w) = app.get_webview_window("hud") {
                            let _ = w.hide();
                        }
                        let _ = app.emit("soulkernel://hud-state", hs.visible);
                    }
                    if shortcut.matches(Modifiers::ALT | Modifiers::SHIFT, Code::KeyJ) {
                        let mut hs = match hud.lock() {
                            Ok(v) => v,
                            Err(_) => return,
                        };
                        hs.interactive = !hs.interactive;
                        let _ = apply_hud_window_mode(app, hs.interactive, &hs.preset, hs.opacity, hs.display_index);
                        let _ = app.emit("soulkernel://hud-interactive", hs.interactive);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_shell::init())
        .manage(state)
        .manage(audit)
        .manage(telemetry_state)
        .manage(hud_state)
         .manage(hud_tx_state)
        .manage(hud_data_state)
        .manage(hud_health_state)
        .setup(move |app| {
            if let Some(w) = app.get_webview_window("hud") {
                let _ = w.hide();
                let _ = w.close();
                let audit = app.state::<SharedAudit>();
                let _ = audit_write(
                    &audit,
                    "hud",
                    "startup-orphan-cleanup",
                    Some("warn"),
                    None,
                );
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
                                let _ = app_handle.emit_to("hud", "soulkernel://hud", payload);
                            }
                        }
                    }
                }
            });

            let app_handle2 = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut hb = tokio::time::interval(Duration::from_millis(1500));
                const HUD_STALE_MS: u64 = 5000;
                const HUD_RELOAD_BASE_COOLDOWN_MS: u64 = 4000;
                const HUD_RELOAD_MAX_COOLDOWN_MS: u64 = 30000;
                const HUD_RELOAD_MAX: u32 = 6;
                loop {
                    hb.tick().await;
                    let hud_state = app_handle2.state::<SharedHud>();
                    let hs = match hud_state.lock() {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    if !hs.visible {
                        continue;
                    }
                    if app_handle2.get_webview_window("hud").is_none() {
                        let _ = apply_hud_window_mode(&app_handle2, hs.interactive, &hs.preset, hs.opacity, hs.display_index);
                        if let Some(w) = app_handle2.get_webview_window("hud") {
                            let _ = w.show();
                        }
                        let audit = app_handle2.state::<SharedAudit>();
                        let _ = audit_write(
                            &audit,
                            "hud",
                            "window-recreated",
                            Some("warn"),
                            Some(serde_json::json!({
                                "preset": hs.preset,
                                "interactive": hs.interactive,
                                "display_index": hs.display_index
                            })),
                        );
                        continue;
                    }

                    let hud_health = app_handle2.state::<SharedHudHealth>();
                    let mut health = match hud_health.lock() {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let now = now_ms_local();
                    let stale = now.saturating_sub(health.last_ready_ms) > HUD_STALE_MS;
                    let exp = health.reload_count.min(3);
                    let cooldown_ms = (HUD_RELOAD_BASE_COOLDOWN_MS.saturating_mul(1u64 << exp))
                        .min(HUD_RELOAD_MAX_COOLDOWN_MS);
                    let since_reload = now.saturating_sub(health.last_reload_ms);
                    let cooldown_ok = since_reload > cooldown_ms;
                    if stale && cooldown_ok && health.reload_count < HUD_RELOAD_MAX {
                        if let Some(w) = app_handle2.get_webview_window("hud") {
                            let _ = w.hide();
                            let _ = w.close();
                        }
                        let _ = apply_hud_window_mode(&app_handle2, hs.interactive, &hs.preset, hs.opacity, hs.display_index);
                        if let Some(w) = app_handle2.get_webview_window("hud") {
                            let _ = w.show();
                        }
                        health.last_reload_ms = now;
                        health.reload_count = health.reload_count.saturating_add(1);
                        let audit = app_handle2.state::<SharedAudit>();
                        let _ = audit_write(
                            &audit,
                            "hud",
                            "window-hard-recreate",
                            Some("warn"),
                            Some(serde_json::json!({
                                "reload_count": health.reload_count,
                                "last_ready_age_ms": now.saturating_sub(health.last_ready_ms),
                                "cooldown_ms": cooldown_ms
                            })),
                        );
                    } else if stale && !cooldown_ok {
                        let audit = app_handle2.state::<SharedAudit>();
                        let _ = audit_write(
                            &audit,
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
            set_soulram,
            get_soulram_status,
            set_policy_mode,
            get_policy_status,
            set_taskbar_gauge,
            run_kpi_probe,
            list_displays,
            open_system_hud,
            close_system_hud,
            set_system_hud_display,
            set_system_hud_interactive,
            set_system_hud_preset,
            set_system_hud_data,
            get_system_hud_data,
            get_system_hud_config,
            set_system_hud_ready,
            audit_log_event,
            get_audit_log_path,
            ingest_telemetry_sample,
            get_telemetry_summary,
            get_energy_pricing,
            set_energy_pricing,
        ])
        .run(tauri::generate_context!())
        .expect("SoulKernel: failed to start Tauri runtime");
}


































