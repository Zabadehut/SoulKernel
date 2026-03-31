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
    collections::hash_map::DefaultHasher,
    fs::OpenOptions,
    hash::{Hash, Hasher},
    path::Path,
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

#[derive(Clone, serde::Serialize)]
pub struct DeviceInventoryItem {
    pub kind: String,
    pub name: String,
    pub detail: Option<String>,
    pub status: Option<String>,
    pub evidence: String,
}

#[derive(Clone, serde::Serialize)]
pub struct DeviceInventoryReport {
    pub platform: String,
    pub displays: Vec<DeviceInventoryItem>,
    pub gpus: Vec<DeviceInventoryItem>,
    pub storage: Vec<DeviceInventoryItem>,
    pub network: Vec<DeviceInventoryItem>,
    pub power: Vec<DeviceInventoryItem>,
    pub connected_endpoints: Vec<DeviceInventoryItem>,
    pub platform_features: Vec<String>,
}

#[cfg(target_os = "linux")]
fn collect_connected_endpoints() -> Vec<DeviceInventoryItem> {
    let mut items = Vec::new();

    if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.contains('-') {
                continue;
            }
            let status = std::fs::read_to_string(entry.path().join("status"))
                .ok()
                .map(|s| s.trim().to_string());
            let modes = std::fs::read_to_string(entry.path().join("modes"))
                .ok()
                .map(|s| s.lines().take(2).collect::<Vec<_>>().join(", "));
            items.push(DeviceInventoryItem {
                kind: "display_output".to_string(),
                name,
                detail: modes.filter(|s| !s.is_empty()),
                status,
                evidence: "platform_detected".to_string(),
            });
        }
    }

    if let Ok(entries) = std::fs::read_dir("/sys/bus/usb/devices") {
        for entry in entries.flatten() {
            let path = entry.path();
            let product = std::fs::read_to_string(path.join("product"))
                .ok()
                .map(|s| s.trim().to_string());
            if product.as_deref().unwrap_or("").is_empty() {
                continue;
            }
            let manufacturer = std::fs::read_to_string(path.join("manufacturer"))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let speed = std::fs::read_to_string(path.join("speed"))
                .ok()
                .map(|s| format!("{} Mb/s", s.trim()))
                .filter(|s| !s.starts_with("0"));
            let vendor_id = std::fs::read_to_string(path.join("idVendor"))
                .ok()
                .map(|s| s.trim().to_string());
            let product_id = std::fs::read_to_string(path.join("idProduct"))
                .ok()
                .map(|s| s.trim().to_string());
            let mut detail_parts = Vec::new();
            if let Some(v) = manufacturer {
                detail_parts.push(v);
            }
            if let Some(v) = speed {
                detail_parts.push(v);
            }
            if let (Some(v), Some(p)) = (vendor_id, product_id) {
                detail_parts.push(format!("{v}:{p}"));
            }
            items.push(DeviceInventoryItem {
                kind: "usb_device".to_string(),
                name: product.unwrap_or_else(|| "USB device".to_string()),
                detail: (!detail_parts.is_empty()).then(|| detail_parts.join(" · ")),
                status: Some("connected".to_string()),
                evidence: "platform_detected".to_string(),
            });
        }
    }

    if let Ok(cards) = std::fs::read_to_string("/proc/asound/cards") {
        for line in cards.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || !trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                continue;
            }
            items.push(DeviceInventoryItem {
                kind: "audio_endpoint".to_string(),
                name: trimmed.to_string(),
                detail: Some("ALSA card".to_string()),
                status: Some("detected".to_string()),
                evidence: "platform_detected".to_string(),
            });
        }
    }

    items
}

#[cfg(target_os = "windows")]
fn collect_connected_endpoints() -> Vec<DeviceInventoryItem> {
    let mut items = Vec::new();
    let out = std::process::Command::new("wmic")
        .args([
            "path",
            "Win32_PnPEntity",
            "where",
            "PNPClass='USB' or PNPClass='Monitor' or PNPClass='MEDIA'",
            "get",
            "Name,PNPClass,Status",
            "/format:csv",
        ])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();

    for line in out.lines().skip(1) {
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 4 {
            continue;
        }
        let class = cols[2].trim();
        let name = cols[1].trim();
        let status = cols[3].trim();
        if name.is_empty() || class.is_empty() {
            continue;
        }
        let kind = match class {
            "USB" => "usb_device",
            "Monitor" => "display_output",
            "MEDIA" => "audio_endpoint",
            _ => "endpoint",
        };
        items.push(DeviceInventoryItem {
            kind: kind.to_string(),
            name: name.to_string(),
            detail: Some(format!("class {class}")),
            status: Some(status.to_string()),
            evidence: "platform_detected".to_string(),
        });
    }

    items
}

#[cfg(target_os = "macos")]
fn collect_connected_endpoints() -> Vec<DeviceInventoryItem> {
    let mut items = Vec::new();
    let sections = [
        ("SPUSBDataType", "usb_device"),
        ("SPAudioDataType", "audio_endpoint"),
        ("SPThunderboltDataType", "external_bus"),
    ];
    for (section, kind) in sections {
        let out = std::process::Command::new("system_profiler")
            .arg(section)
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        for line in out.lines() {
            let raw = line.trim_end();
            let trimmed = raw.trim();
            if trimmed.is_empty() || !trimmed.ends_with(':') {
                continue;
            }
            if trimmed.contains("Data Type") || trimmed.contains("Bus:") {
                continue;
            }
            let name = trimmed.trim_end_matches(':').trim();
            if name.is_empty() {
                continue;
            }
            items.push(DeviceInventoryItem {
                kind: kind.to_string(),
                name: name.to_string(),
                detail: Some(section.to_string()),
                status: Some("detected".to_string()),
                evidence: "platform_detected".to_string(),
            });
        }
    }
    items
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn collect_connected_endpoints() -> Vec<DeviceInventoryItem> {
    Vec::new()
}

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
    pub platform: String,
    pub equivalent_goal: String,
    pub roadmap: Vec<String>,
}

#[derive(Clone, serde::Serialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_usage: f64,
    pub gpu_usage_pct: Option<f64>,
    pub parent_pid: Option<u32>,
    /// RSS approximative (KiB).
    pub memory_kb: u64,
    pub memory_share_pct: Option<f64>,
    pub disk_read_bytes: Option<u64>,
    pub disk_written_bytes: Option<u64>,
    pub run_time_s: Option<u64>,
    pub status: Option<String>,
    pub exe: Option<String>,
    pub cmd: Vec<String>,
    pub is_self_process: bool,
    pub is_embedded_webview: bool,
    pub impact_score_pct_estimated: Option<f64>,
    pub estimated_power_share_pct: Option<f64>,
    pub estimated_power_w: Option<f64>,
    pub attribution_method: Option<String>,
}

#[derive(serde::Serialize)]
pub struct ProcessImpactSummary {
    pub process_count: usize,
    pub top_count: usize,
    pub observed_cpu_count: usize,
    pub observed_gpu_count: usize,
    pub observed_memory_count: usize,
    pub observed_io_count: usize,
    pub machine_power_w: Option<f64>,
    pub attribution_method: String,
    pub report_revision: String,
    pub ui_revision: String,
}

#[derive(serde::Serialize)]
pub struct ProcessImpactReport {
    pub processes: Vec<ProcessInfo>,
    pub top_processes: Vec<ProcessInfo>,
    pub top_process_rows: Vec<ProcessImpactUiRow>,
    pub grouped_processes: Vec<ProcessImpactGroup>,
    pub overhead_audit: ProcessOverheadAudit,
    pub summary: ProcessImpactSummary,
}

#[derive(serde::Serialize)]
pub struct ProcessImpactUiRow {
    pub pid: u32,
    pub name: String,
    pub exe: Option<String>,
    pub cmd_preview: Option<String>,
    pub cpu_label: String,
    pub gpu_label: String,
    pub ram_label: String,
    pub ram_share_label: String,
    pub io_label: String,
    pub io_split_label: String,
    pub power_label: String,
    pub impact_label: String,
    pub duration_label: String,
    pub status_label: String,
    pub role: String,
    pub attribution_method: String,
    pub is_self_process: bool,
    pub is_embedded_webview: bool,
}

#[derive(serde::Serialize)]
pub struct ProcessImpactGroup {
    pub key: String,
    pub process_count: usize,
    pub cpu_usage_pct: f64,
    pub gpu_usage_pct: f64,
    pub memory_kb: u64,
    pub estimated_power_w: Option<f64>,
    pub impact_score_pct_estimated: Option<f64>,
}

#[derive(serde::Serialize)]
pub struct ProcessOverheadAudit {
    pub soulkernel_process_count: usize,
    pub soulkernel_cpu_usage_pct: f64,
    pub soulkernel_gpu_usage_pct: f64,
    pub soulkernel_memory_kb: u64,
    pub soulkernel_estimated_power_w: Option<f64>,
    pub webview_process_count: usize,
    pub webview_cpu_usage_pct: f64,
    pub webview_gpu_usage_pct: f64,
    pub webview_memory_kb: u64,
    pub webview_estimated_power_w: Option<f64>,
    pub combined_cpu_usage_pct: f64,
    pub combined_gpu_usage_pct: f64,
    pub combined_memory_kb: u64,
    pub combined_estimated_power_w: Option<f64>,
    pub webview_runtime_buckets: Vec<WebviewRuntimeBucketAudit>,
}

#[derive(serde::Serialize)]
pub struct WebviewRuntimeBucketAudit {
    pub key: String,
    pub label: String,
    pub process_count: usize,
    pub cpu_usage_pct: f64,
    pub gpu_usage_pct: f64,
    pub memory_kb: u64,
    pub estimated_power_w: Option<f64>,
}

fn is_embedded_webview_name(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("msedgewebview")
        || n.contains("webview2")
        || n.contains("webkitnetworkprocess")
        || n.contains("webkit.webcontent")
        || n.contains("webkitwebprocess")
        || (n.contains("webkit") && n.contains("gpu"))
}

fn format_bytes_iec(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let value = bytes as f64;
    if bytes == 0 {
        "—".to_string()
    } else if value >= GIB {
        format!("{:.2} GiB", value / GIB)
    } else if value >= MIB {
        format!("{:.1} MiB", value / MIB)
    } else if value >= KIB {
        format!("{:.1} KiB", value / KIB)
    } else {
        format!("{} B", bytes)
    }
}

fn format_runtime_compact(run_time_s: Option<u64>) -> String {
    let Some(secs) = run_time_s else {
        return "—".to_string();
    };
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}

fn process_role(info: &ProcessInfo) -> &'static str {
    if info.is_self_process {
        "self"
    } else if info.is_embedded_webview {
        "webview"
    } else {
        "other"
    }
}

fn process_group_key(info: &ProcessInfo) -> String {
    if let Some(exe) = info.exe.as_deref() {
        if let Some(stem) = Path::new(exe).file_stem().and_then(|s| s.to_str()) {
            let trimmed = stem.trim();
            if !trimmed.is_empty() {
                return trimmed.to_lowercase();
            }
        }
    }
    info.name.trim().to_lowercase()
}

fn classify_webview_runtime_bucket(info: &ProcessInfo) -> (&'static str, String) {
    let name = info.name.to_lowercase();
    let cmd = info.cmd.join(" ").to_lowercase();
    if name.contains("crashpad") || cmd.contains("crashpad") {
        return ("crashpad", "Crashpad".to_string());
    }
    if name.contains("gpu") || cmd.contains("--type=gpu-process") {
        return ("gpu", "GPU process".to_string());
    }
    if name.contains("manager") || cmd.contains("--type=browser") {
        return ("manager", "Manager".to_string());
    }
    if cmd.contains("--type=utility") || name.contains("utility") {
        if cmd.contains("network.mojom.networkservice")
            || cmd.contains("--utility-sub-type=network")
            || name.contains("network service")
        {
            return ("utility_network", "Utility: Network".to_string());
        }
        if cmd.contains("storage.mojom.storageservice")
            || cmd.contains("--utility-sub-type=storage")
            || name.contains("storage service")
        {
            return ("utility_storage", "Utility: Storage".to_string());
        }
        if cmd.contains("audio")
            || cmd.contains("--utility-sub-type=audio")
            || name.contains("audio service")
        {
            return ("utility_audio", "Utility: Audio".to_string());
        }
        return ("utility_other", "Utility: Other".to_string());
    }
    if cmd.contains("--type=renderer") || name.contains("webview2") || name.contains("webcontent") {
        return ("renderer", "Renderer".to_string());
    }
    ("other", "Other WebView".to_string())
}

fn build_process_ui_row(info: &ProcessInfo) -> ProcessImpactUiRow {
    let cmd_preview = if info.cmd.is_empty() {
        None
    } else {
        Some(info.cmd.join(" "))
    };
    let ram_mib = info.memory_kb as f64 / 1024.0;
    let io_read = info.disk_read_bytes.unwrap_or(0);
    let io_write = info.disk_written_bytes.unwrap_or(0);
    ProcessImpactUiRow {
        pid: info.pid,
        name: info.name.clone(),
        exe: info.exe.clone(),
        cmd_preview,
        cpu_label: if info.cpu_usage.is_finite() {
            format!("{:.1} %", info.cpu_usage)
        } else {
            "—".to_string()
        },
        gpu_label: info
            .gpu_usage_pct
            .map(|v| format!("{v:.1} %"))
            .unwrap_or_else(|| "—".to_string()),
        ram_label: if info.memory_kb > 0 {
            format!("{:.0} MiB", ram_mib)
        } else {
            "—".to_string()
        },
        ram_share_label: info
            .memory_share_pct
            .map(|v| format!("{v:.1}%"))
            .unwrap_or_else(|| "—".to_string()),
        io_label: format_bytes_iec(io_read.saturating_add(io_write)),
        io_split_label: format!(
            "R {} / W {}",
            format_bytes_iec(io_read),
            format_bytes_iec(io_write)
        ),
        power_label: info
            .estimated_power_w
            .map(|v| format!("{v:.2} W"))
            .unwrap_or_else(|| "—".to_string()),
        impact_label: info
            .impact_score_pct_estimated
            .map(|v| format!("{v:.2} %"))
            .unwrap_or_else(|| "—".to_string()),
        duration_label: format_runtime_compact(info.run_time_s),
        status_label: info.status.clone().unwrap_or_else(|| "—".to_string()),
        role: process_role(info).to_string(),
        attribution_method: info
            .attribution_method
            .clone()
            .unwrap_or_else(|| "—".to_string()),
        is_self_process: info.is_self_process,
        is_embedded_webview: info.is_embedded_webview,
    }
}

fn hash_f64(hasher: &mut DefaultHasher, value: f64) {
    if value.is_finite() {
        value.to_bits().hash(hasher);
    } else {
        0u64.hash(hasher);
    }
}

fn build_process_report_revision(processes: &[ProcessInfo]) -> String {
    let mut hasher = DefaultHasher::new();
    processes.len().hash(&mut hasher);
    for p in processes.iter().take(64) {
        p.pid.hash(&mut hasher);
        p.name.hash(&mut hasher);
        hash_f64(&mut hasher, p.cpu_usage);
        p.memory_kb.hash(&mut hasher);
        p.disk_read_bytes.unwrap_or(0).hash(&mut hasher);
        p.disk_written_bytes.unwrap_or(0).hash(&mut hasher);
        hash_f64(
            &mut hasher,
            p.impact_score_pct_estimated.unwrap_or_default(),
        );
        hash_f64(&mut hasher, p.estimated_power_w.unwrap_or_default());
    }
    format!("{:016x}", hasher.finish())
}

fn build_process_ui_revision(rows: &[ProcessImpactUiRow]) -> String {
    let mut hasher = DefaultHasher::new();
    rows.len().hash(&mut hasher);
    for row in rows {
        row.pid.hash(&mut hasher);
        row.cpu_label.hash(&mut hasher);
        row.ram_label.hash(&mut hasher);
        row.io_label.hash(&mut hasher);
        row.power_label.hash(&mut hasher);
        row.impact_label.hash(&mut hasher);
        row.status_label.hash(&mut hasher);
        row.role.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
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
    pub resolved_python_bin: String,
    pub python_source: String,
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

fn bundled_python_relative_paths() -> &'static [&'static str] {
    #[cfg(target_os = "windows")]
    {
        &[
            "python/windows/python.exe",
            "runtime/python/windows/python.exe",
        ]
    }
    #[cfg(target_os = "macos")]
    {
        &[
            "python/macos/bin/python3",
            "runtime/python/macos/bin/python3",
        ]
    }
    #[cfg(target_os = "linux")]
    {
        &[
            "python/linux/bin/python3",
            "runtime/python/linux/bin/python3",
        ]
    }
}

fn allow_bundled_python_in_dev() -> bool {
    std::env::var("SOULKERNEL_USE_BUNDLED_PYTHON_IN_DEV")
        .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
}

fn resolve_bundled_python(app: &AppHandle) -> Option<std::path::PathBuf> {
    if cfg!(debug_assertions) && !allow_bundled_python_in_dev() {
        return None;
    }
    let mut bases = Vec::new();
    if let Some(resource_dir) = app.path().resource_dir().ok() {
        bases.push(resource_dir);
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.push(cwd);
    }

    for base in bases {
        for rel in bundled_python_relative_paths() {
            let candidate = base.join(rel);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn external_bridge_log_path() -> Result<std::path::PathBuf, String> {
    external_power::soulkernel_config_dir()
        .map(|p| p.join("meross_bridge.log"))
        .ok_or_else(|| "config dir unavailable".to_string())
}

fn bridge_log_last_non_empty_line() -> Option<String> {
    let path = external_bridge_log_path().ok()?;
    let raw = std::fs::read_to_string(path).ok()?;
    raw.lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| {
            const MAX_CHARS: usize = 320;
            if line.chars().count() > MAX_CHARS {
                let tail: String = line
                    .chars()
                    .rev()
                    .take(MAX_CHARS)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
                format!("...{tail}")
            } else {
                line.to_string()
            }
        })
}

fn effective_python_candidates(
    app: &AppHandle,
    cfg: &external_power::MerossFileConfig,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(path) = resolve_bundled_python(app) {
        out.push(path.to_string_lossy().into_owned());
    }
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

fn pick_python_bin(
    app: &AppHandle,
    cfg: &external_power::MerossFileConfig,
) -> Result<String, String> {
    for candidate in effective_python_candidates(app, cfg) {
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
    Err(
        "python introuvable (essayés: runtime embarqué, python configuré, python3/python/py)"
            .to_string(),
    )
}

fn detect_python_source(app: &AppHandle, resolved_python_bin: &str) -> String {
    let resolved = resolved_python_bin.trim();
    if resolved.is_empty() {
        return "none".to_string();
    }
    if let Some(bundled) = resolve_bundled_python(app) {
        if bundled.to_string_lossy() == resolved {
            return "embedded".to_string();
        }
    }
    "system".to_string()
}

fn refresh_bridge_process_state(bridge: &SharedExternalBridge) -> (bool, Option<u32>) {
    let mut g = match bridge.lock() {
        Ok(v) => v,
        Err(_) => return (false, None),
    };
    if let Some(child) = g.child.as_mut() {
        match child.try_wait() {
            Ok(Some(status)) => {
                let detail = bridge_log_last_non_empty_line();
                g.last_error = Some(match detail {
                    Some(detail) => format!("bridge arrêté ({status}) | {detail}"),
                    None => format!("bridge arrêté ({status})"),
                });
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
    let cfg = external_power::get_meross_config_or_default();
    let (last_error, last_start_ts_ms) = bridge
        .lock()
        .map(|g| (g.last_error.clone(), g.last_start_ts_ms))
        .unwrap_or((Some("bridge state poisoned".to_string()), None));
    let resolved_python_bin = pick_python_bin(app, &cfg).unwrap_or_default();
    let python_source = detect_python_source(app, &resolved_python_bin);
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
        resolved_python_bin,
        python_source,
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
    let http_proxy = cfg
        .meross_http_proxy
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let mfa_code = cfg
        .meross_mfa_code
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let interval = cfg.bridge_interval_s.unwrap_or(8.0).clamp(2.0, 300.0);
    let python_bin = pick_python_bin(app, &cfg)?;
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
    let creds_cache_path = external_power::default_creds_cache_file()
        .ok_or_else(|| "creds cache path unavailable".to_string())?;
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if let Some(parent) = creds_cache_path.parent() {
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
        .env("MEROSS_CREDS_CACHE", &creds_cache_path)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .stdin(Stdio::null());
    if let Some(proxy) = http_proxy {
        cmd.env("MEROSS_HTTP_PROXY", proxy);
    }
    if let Some(mfa_code) = mfa_code {
        cmd.env("MEROSS_MFA_CODE", mfa_code);
    }
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    g.last_error = None;
    g.last_start_ts_ms = Some(now_ms_local());
    g.child = Some(child);
    Ok(())
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
fn list_processes() -> Result<ProcessImpactReport, String> {
    let mut sys = sysinfo::System::new_all();
    sys.refresh_processes();
    thread::sleep(Duration::from_millis(220));
    sys.refresh_processes();
    sys.refresh_memory();

    let machine_metrics = metrics::collect().ok();
    let machine_power_w = machine_metrics.as_ref().and_then(|m| m.raw.power_watts);
    #[cfg(target_os = "windows")]
    let process_gpu_map = crate::platform::windows::process_gpu_utilisation_by_pid();
    #[cfg(not(target_os = "windows"))]
    let process_gpu_map = std::collections::HashMap::<u32, f64>::new();
    let total_mem_kb = (sys.total_memory() / 1024).max(1);
    let self_pid = sysinfo::get_current_pid().ok().map(|p| p.as_u32());
    let cpu_sum = sys
        .processes()
        .values()
        .map(|p| (p.cpu_usage() as f64).max(0.0))
        .sum::<f64>();
    let io_sum = sys
        .processes()
        .values()
        .map(|p| {
            let du = p.disk_usage();
            du.read_bytes.saturating_add(du.written_bytes) as f64
        })
        .sum::<f64>();
    let gpu_sum = process_gpu_map
        .values()
        .copied()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .sum::<f64>();

    let mut rows: Vec<(ProcessInfo, f64)> = sys
        .processes()
        .iter()
        .map(|(pid, p)| {
            let cpu_usage = p.cpu_usage() as f64;
            let gpu_usage_pct = process_gpu_map.get(&pid.as_u32()).copied();
            let memory_kb = p.memory() / 1024;
            let memory_share_pct = Some((memory_kb as f64 / total_mem_kb as f64) * 100.0);
            let du = p.disk_usage();
            let disk_bytes = du.read_bytes.saturating_add(du.written_bytes) as f64;
            let cpu_share_pct = if cpu_sum > 0.0 {
                (cpu_usage.max(0.0) / cpu_sum) * 100.0
            } else {
                0.0
            };
            let io_share_pct = if io_sum > 0.0 {
                (disk_bytes / io_sum) * 100.0
            } else {
                0.0
            };
            let gpu_share_pct = if gpu_sum > 0.0 {
                (gpu_usage_pct.unwrap_or(0.0).max(0.0) / gpu_sum) * 100.0
            } else {
                0.0
            };
            let impact_raw = if gpu_sum > 0.0 {
                (0.55 * cpu_share_pct
                    + 0.15 * memory_share_pct.unwrap_or(0.0)
                    + 0.10 * io_share_pct
                    + 0.20 * gpu_share_pct)
                    .max(0.0)
            } else {
                (0.70 * cpu_share_pct
                    + 0.20 * memory_share_pct.unwrap_or(0.0)
                    + 0.10 * io_share_pct)
                    .max(0.0)
            };
            let name = p.name().to_string();
            (
                ProcessInfo {
                    pid: pid.as_u32(),
                    name: name.clone(),
                    cpu_usage,
                    gpu_usage_pct,
                    parent_pid: p.parent().map(|pp| pp.as_u32()),
                    memory_kb,
                    memory_share_pct,
                    disk_read_bytes: Some(du.read_bytes),
                    disk_written_bytes: Some(du.written_bytes),
                    run_time_s: Some(p.run_time()),
                    status: Some(format!("{:?}", p.status()).to_lowercase()),
                    exe: p.exe().map(|v| v.to_string_lossy().into_owned()),
                    cmd: p.cmd().iter().map(|s| s.to_string()).collect(),
                    is_self_process: self_pid == Some(pid.as_u32()),
                    is_embedded_webview: is_embedded_webview_name(&name),
                    impact_score_pct_estimated: None,
                    estimated_power_share_pct: None,
                    estimated_power_w: None,
                    attribution_method: None,
                },
                impact_raw,
            )
        })
        .collect();

    let impact_sum = rows.iter().map(|(_, raw)| raw.max(0.0)).sum::<f64>();
    let has_power = machine_power_w.is_some();
    let mut list: Vec<ProcessInfo> = rows
        .drain(..)
        .map(|(mut info, impact_raw)| {
            let impact_pct = if impact_sum > 0.0 {
                Some((impact_raw / impact_sum) * 100.0)
            } else {
                None
            };
            info.impact_score_pct_estimated = impact_pct;
            info.estimated_power_share_pct = impact_pct;
            info.estimated_power_w = impact_pct
                .zip(machine_power_w)
                .map(|(pct, w)| (pct / 100.0) * w);
            info.attribution_method = Some(if gpu_sum > 0.0 && has_power {
                "estimated_weighted_cpu_gpu_mem_io_over_measured_machine_power".to_string()
            } else if gpu_sum > 0.0 {
                "estimated_weighted_cpu_gpu_mem_io".to_string()
            } else if has_power {
                "estimated_weighted_cpu_mem_io_over_measured_machine_power".to_string()
            } else {
                "estimated_weighted_cpu_mem_io".to_string()
            });
            info
        })
        .collect();

    list.sort_by(|a, b| {
        b.impact_score_pct_estimated
            .unwrap_or(b.cpu_usage)
            .partial_cmp(&a.impact_score_pct_estimated.unwrap_or(a.cpu_usage))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let top_limit = 12usize;
    let top_processes = list.iter().take(top_limit).cloned().collect::<Vec<_>>();
    let top_process_rows = top_processes
        .iter()
        .map(build_process_ui_row)
        .collect::<Vec<_>>();
    let report_revision = build_process_report_revision(&list);
    let ui_revision = build_process_ui_revision(&top_process_rows);
    let mut grouped = std::collections::BTreeMap::<String, ProcessImpactGroup>::new();
    for info in &list {
        let key = process_group_key(info);
        let entry = grouped.entry(key.clone()).or_insert(ProcessImpactGroup {
            key,
            process_count: 0,
            cpu_usage_pct: 0.0,
            gpu_usage_pct: 0.0,
            memory_kb: 0,
            estimated_power_w: Some(0.0),
            impact_score_pct_estimated: Some(0.0),
        });
        entry.process_count += 1;
        entry.cpu_usage_pct += info.cpu_usage.max(0.0);
        entry.gpu_usage_pct += info.gpu_usage_pct.unwrap_or(0.0).max(0.0);
        entry.memory_kb = entry.memory_kb.saturating_add(info.memory_kb);
        entry.estimated_power_w = match (entry.estimated_power_w, info.estimated_power_w) {
            (Some(a), Some(b)) => Some(a + b),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        entry.impact_score_pct_estimated = match (
            entry.impact_score_pct_estimated,
            info.impact_score_pct_estimated,
        ) {
            (Some(a), Some(b)) => Some(a + b),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
    }
    let mut grouped_processes = grouped.into_values().collect::<Vec<_>>();
    grouped_processes.sort_by(|a, b| {
        b.impact_score_pct_estimated
            .unwrap_or(b.cpu_usage_pct)
            .partial_cmp(&a.impact_score_pct_estimated.unwrap_or(a.cpu_usage_pct))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    grouped_processes.truncate(12);
    let soulkernel_processes = list
        .iter()
        .filter(|p| p.is_self_process)
        .collect::<Vec<_>>();
    let webview_processes = list
        .iter()
        .filter(|p| p.is_embedded_webview)
        .collect::<Vec<_>>();
    let mut webview_runtime_buckets_map =
        std::collections::BTreeMap::<String, WebviewRuntimeBucketAudit>::new();
    for info in &webview_processes {
        let (key, label) = classify_webview_runtime_bucket(info);
        let entry = webview_runtime_buckets_map
            .entry(key.to_string())
            .or_insert(WebviewRuntimeBucketAudit {
                key: key.to_string(),
                label,
                process_count: 0,
                cpu_usage_pct: 0.0,
                gpu_usage_pct: 0.0,
                memory_kb: 0,
                estimated_power_w: Some(0.0),
            });
        entry.process_count += 1;
        entry.cpu_usage_pct += info.cpu_usage.max(0.0);
        entry.gpu_usage_pct += info.gpu_usage_pct.unwrap_or(0.0).max(0.0);
        entry.memory_kb = entry.memory_kb.saturating_add(info.memory_kb);
        entry.estimated_power_w = match (entry.estimated_power_w, info.estimated_power_w) {
            (Some(a), Some(b)) => Some(a + b),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
    }
    let mut webview_runtime_buckets = webview_runtime_buckets_map
        .into_values()
        .collect::<Vec<_>>();
    webview_runtime_buckets.sort_by(|a, b| {
        b.cpu_usage_pct
            .partial_cmp(&a.cpu_usage_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let sum_power = |items: &[&ProcessInfo]| -> Option<f64> {
        let mut any = false;
        let mut total = 0.0f64;
        for item in items {
            if let Some(v) = item.estimated_power_w {
                any = true;
                total += v;
            }
        }
        if any {
            Some(total)
        } else {
            None
        }
    };
    let overhead_audit = ProcessOverheadAudit {
        soulkernel_process_count: soulkernel_processes.len(),
        soulkernel_cpu_usage_pct: soulkernel_processes
            .iter()
            .map(|p| p.cpu_usage.max(0.0))
            .sum(),
        soulkernel_gpu_usage_pct: soulkernel_processes
            .iter()
            .map(|p| p.gpu_usage_pct.unwrap_or(0.0).max(0.0))
            .sum(),
        soulkernel_memory_kb: soulkernel_processes.iter().map(|p| p.memory_kb).sum(),
        soulkernel_estimated_power_w: sum_power(&soulkernel_processes),
        webview_process_count: webview_processes.len(),
        webview_cpu_usage_pct: webview_processes.iter().map(|p| p.cpu_usage.max(0.0)).sum(),
        webview_gpu_usage_pct: webview_processes
            .iter()
            .map(|p| p.gpu_usage_pct.unwrap_or(0.0).max(0.0))
            .sum(),
        webview_memory_kb: webview_processes.iter().map(|p| p.memory_kb).sum(),
        webview_estimated_power_w: sum_power(&webview_processes),
        combined_cpu_usage_pct: soulkernel_processes
            .iter()
            .chain(webview_processes.iter())
            .map(|p| p.cpu_usage.max(0.0))
            .sum(),
        combined_gpu_usage_pct: soulkernel_processes
            .iter()
            .chain(webview_processes.iter())
            .map(|p| p.gpu_usage_pct.unwrap_or(0.0).max(0.0))
            .sum(),
        combined_memory_kb: soulkernel_processes
            .iter()
            .chain(webview_processes.iter())
            .map(|p| p.memory_kb)
            .sum(),
        combined_estimated_power_w: match (
            sum_power(&soulkernel_processes),
            sum_power(&webview_processes),
        ) {
            (Some(a), Some(b)) => Some(a + b),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        },
        webview_runtime_buckets,
    };
    let summary = ProcessImpactSummary {
        process_count: list.len(),
        top_count: top_processes.len(),
        observed_cpu_count: top_processes
            .iter()
            .filter(|p| p.cpu_usage.is_finite())
            .count(),
        observed_gpu_count: top_processes
            .iter()
            .filter(|p| p.gpu_usage_pct.is_some())
            .count(),
        observed_memory_count: top_processes.iter().filter(|p| p.memory_kb > 0).count(),
        observed_io_count: top_processes
            .iter()
            .filter(|p| p.disk_read_bytes.unwrap_or(0) > 0 || p.disk_written_bytes.unwrap_or(0) > 0)
            .count(),
        machine_power_w,
        attribution_method: if gpu_sum > 0.0 && has_power {
            "estimated_weighted_cpu_gpu_mem_io_over_measured_machine_power".to_string()
        } else if gpu_sum > 0.0 {
            "estimated_weighted_cpu_gpu_mem_io".to_string()
        } else if has_power {
            "estimated_weighted_cpu_mem_io_over_measured_machine_power".to_string()
        } else {
            "estimated_weighted_cpu_mem_io".to_string()
        },
        report_revision,
        ui_revision,
    };

    Ok(ProcessImpactReport {
        processes: list,
        top_processes,
        top_process_rows,
        grouped_processes,
        overhead_audit,
        summary,
    })
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
fn get_device_inventory(app: AppHandle) -> Result<DeviceInventoryReport, String> {
    let platform = platform::info();
    let raw = metrics::collect().ok().map(|m| m.raw);

    let app2 = app.clone();
    let displays = hud::dispatch_on_main_thread(&app, move || hud::list_displays_internal(&app2))
        .unwrap_or_else(|_| Ok(Vec::new()))
        .unwrap_or_default()
        .into_iter()
        .map(|display| DeviceInventoryItem {
            kind: "display".to_string(),
            name: display.name,
            detail: Some(format!(
                "{}x{} · scale {:.2} · pos {}:{}",
                display.width, display.height, display.scale_factor, display.x, display.y
            )),
            status: Some(if display.is_primary {
                "primary".to_string()
            } else {
                "active".to_string()
            }),
            evidence: "platform_detected".to_string(),
        })
        .collect::<Vec<_>>();

    let gpus = raw
        .as_ref()
        .map(|raw| {
            if !raw.gpu_devices.is_empty() {
                raw.gpu_devices
                    .iter()
                    .map(|gpu| DeviceInventoryItem {
                        kind: "gpu".to_string(),
                        name: gpu
                            .name
                            .clone()
                            .unwrap_or_else(|| format!("GPU {}", gpu.index)),
                        detail: Some(format!(
                            "{} · util {} · power {}",
                            gpu.vendor.clone().unwrap_or_else(|| "vendor —".to_string()),
                            gpu.utilization_pct
                                .map(|v| format!("{v:.1} %"))
                                .unwrap_or_else(|| "—".to_string()),
                            gpu.power_watts
                                .map(|v| format!("{v:.1} W"))
                                .unwrap_or_else(|| "—".to_string())
                        )),
                        status: gpu.kind.clone(),
                        evidence: gpu
                            .confidence
                            .clone()
                            .unwrap_or_else(|| "observed_usage".to_string()),
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        })
        .unwrap_or_default();

    let disks = sysinfo::Disks::new_with_refreshed_list();
    let storage = disks
        .list()
        .iter()
        .map(|disk| DeviceInventoryItem {
            kind: "storage".to_string(),
            name: disk.name().to_string_lossy().to_string(),
            detail: Some(format!(
                "{} · {} / {} GiB",
                String::from_utf8_lossy(disk.file_system()),
                ((disk.total_space().saturating_sub(disk.available_space())) as f64
                    / 1024.0
                    / 1024.0
                    / 1024.0)
                    .round(),
                (disk.total_space() as f64 / 1024.0 / 1024.0 / 1024.0).round()
            )),
            status: Some(format!("{:?}", disk.kind()).to_lowercase()),
            evidence: "platform_detected".to_string(),
        })
        .collect::<Vec<_>>();

    let networks = sysinfo::Networks::new_with_refreshed_list();
    let network = networks
        .iter()
        .map(|(name, data)| DeviceInventoryItem {
            kind: "network".to_string(),
            name: name.to_string(),
            detail: Some(format!(
                "rx {} B · tx {} B",
                data.received(),
                data.transmitted()
            )),
            status: Some("detected".to_string()),
            evidence: "platform_detected".to_string(),
        })
        .collect::<Vec<_>>();

    let mut power = Vec::new();
    if let Some(raw) = raw.as_ref() {
        if let Some(source) = raw.power_watts_source.clone() {
            power.push(DeviceInventoryItem {
                kind: "power_source".to_string(),
                name: source,
                detail: raw
                    .power_watts
                    .map(|v| format!("{v:.2} W machine"))
                    .or_else(|| Some("W machine indisponibles".to_string())),
                status: Some("active".to_string()),
                evidence: "platform_measured".to_string(),
            });
        }
        if let Some(on_battery) = raw.on_battery {
            power.push(DeviceInventoryItem {
                kind: "power_mode".to_string(),
                name: if on_battery {
                    "battery".to_string()
                } else {
                    "ac".to_string()
                },
                detail: raw
                    .battery_percent
                    .map(|v| format!("{v:.0} %"))
                    .or_else(|| Some("niveau inconnu".to_string())),
                status: Some("detected".to_string()),
                evidence: "platform_detected".to_string(),
            });
        }
    }
    let connected_endpoints = collect_connected_endpoints();

    Ok(DeviceInventoryReport {
        platform: platform.os,
        displays,
        gpus,
        storage,
        network,
        power,
        connected_endpoints,
        platform_features: platform.features,
    })
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
    let backend = platform::soulram_backend_info();
    SoulRamStatusResponse {
        active: s.soulram_active,
        percent: s.soulram_percent,
        backend: backend.backend,
        platform: backend.platform,
        equivalent_goal: backend.equivalent_goal,
        roadmap: backend.roadmap,
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
            get_device_inventory,
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
