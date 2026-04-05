use crate::state::LiteViewModel;
use chrono::{Local, TimeZone, Utc};
use flate2::write::GzEncoder;
use flate2::Compression;
use rfd::FileDialog;
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::PathBuf;

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct ExternalConfigExport<'a> {
    enabled: bool,
    power_file: Option<&'a str>,
    max_age_ms: Option<u64>,
    meross_email: Option<&'a str>,
    meross_region: Option<&'a str>,
    meross_device_type: Option<&'a str>,
    meross_http_proxy: Option<&'a str>,
    mfa_present: bool,
    python_bin: Option<&'a str>,
    bridge_interval_s: Option<f64>,
    autostart_bridge: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct LiteRawHostMetricsExport<'a> {
    exported_at: String,
    exported_at_ms: u64,
    available: bool,
    platform: &'a str,
    observed_metric_count: usize,
    observed_metric_keys: Vec<&'static str>,
    device_inventory: &'a soulkernel_core::inventory::DeviceInventoryReport,
    normalized: NormalizedMetricsExport<'a>,
    raw: RawMetricsExport<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct NormalizedMetricsExport<'a> {
    cpu: f64,
    mem: f64,
    compression: Option<f64>,
    io_bandwidth: Option<f64>,
    gpu: Option<f64>,
    sigma: f64,
    epsilon: &'a [f64; 5],
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct RawMetricsExport<'a> {
    cpu_pct: f64,
    cpu_clock_mhz: Option<f64>,
    cpu_max_clock_mhz: Option<f64>,
    cpu_freq_ratio: Option<f64>,
    cpu_temp_c: Option<f64>,
    mem_used_mb: u64,
    mem_total_mb: u64,
    ram_clock_mhz: Option<f64>,
    swap_used_mb: u64,
    swap_total_mb: u64,
    zram_used_mb: Option<u64>,
    io_read_mb_s: Option<f64>,
    io_write_mb_s: Option<f64>,
    gpu_pct: Option<f64>,
    gpu_core_clock_mhz: Option<f64>,
    gpu_mem_clock_mhz: Option<f64>,
    gpu_temp_c: Option<f64>,
    gpu_power_watts: Option<f64>,
    gpu_power_source: Option<&'a str>,
    gpu_power_confidence: Option<&'a str>,
    gpu_mem_used_mb: Option<u64>,
    gpu_mem_total_mb: Option<u64>,
    gpu_devices: &'a [soulkernel_core::metrics::GpuDeviceMetrics],
    power_watts: Option<f64>,
    power_watts_source: Option<&'a str>,
    host_power_watts: Option<f64>,
    host_power_watts_source: Option<&'a str>,
    wall_power_watts: Option<f64>,
    wall_power_watts_source: Option<&'a str>,
    psi_cpu: Option<f64>,
    psi_mem: Option<f64>,
    load_avg_1m_norm: Option<f64>,
    runnable_tasks: Option<u64>,
    on_battery: Option<bool>,
    battery_percent: Option<f64>,
    page_faults_per_sec: Option<f64>,
    webview_host_cpu_sum: Option<f64>,
    webview_host_mem_mb: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct ExternalPowerExport<'a> {
    source_tag: &'a str,
    last_watts_label: String,
    freshness: &'static str,
    file_presence: &'static str,
    bridge_state: &'static str,
    runtime: &'static str,
    python_bin: &'a str,
    config_path: &'a str,
    power_file_path: &'a str,
    cache_path: &'a str,
    bridge_log_path: &'a str,
    last_ts_ms: Option<u64>,
    last_ts_label: String,
    last_error: &'a str,
    bridge_detail: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct StrictEvidenceExport<'a> {
    mode: &'static str,
    exported_at: String,
    exported_at_ms: u64,
    machine_activity: &'static str,
    assertions: StrictEvidenceAssertions<'a>,
    allowed_claims: Vec<&'static str>,
    forbidden_claims: Vec<&'static str>,
    external_power: ExternalPowerExport<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct StrictEvidenceAssertions<'a> {
    measured_os_metrics: MeasuredOsMetrics,
    raw_host_metrics: LiteRawHostMetricsExport<'a>,
    measured_energy: MeasuredEnergy<'a>,
    measured_differentials: MeasuredDifferentials<'a>,
    benchmark_ab: Option<BenchmarkEvidence>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct MeasuredOsMetrics {
    cpu_pct: f64,
    mem_used_mb: u64,
    mem_total_mb: u64,
    io_read_mb_s: Option<f64>,
    io_write_mb_s: Option<f64>,
    gpu_pct: Option<f64>,
    sigma: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct MeasuredEnergy<'a> {
    power_source: &'a str,
    live_power_w: Option<f64>,
    total_energy_kwh: f64,
    total_cost: f64,
    total_co2_kg: f64,
    period_windows: PeriodWindows,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct PeriodWindows {
    hour_kwh: f64,
    day_kwh: f64,
    week_kwh: f64,
    month_kwh: f64,
    year_kwh: f64,
}

// GainsSummary est défini dans soulkernel-core::telemetry — type unique partagé
// entre soulkernel-lite et le backend Tauri.

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct MeasuredDifferentials<'a> {
    cpu_hours_differential: f64,
    mem_gb_hours_differential: f64,
    lifetime_cpu_hours_differential: f64,
    lifetime_mem_gb_hours_differential: f64,
    idle_ratio: f64,
    media_ratio: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    _phantom: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct BenchmarkEvidence {
    samples_off_ok: usize,
    samples_on_ok: usize,
    gain_median_pct: Option<f64>,
    gain_p95_pct: Option<f64>,
    gain_power_median_pct: Option<f64>,
    gain_cpu_median_pct: Option<f64>,
    gain_mem_median_pct: Option<f64>,
    gain_sigma_median_pct: Option<f64>,
    measured_efficiency_off: Option<soulkernel_core::formula::MeasuredEfficiency>,
    measured_efficiency_on: Option<soulkernel_core::formula::MeasuredEfficiency>,
    gain_utility_per_watt_pct: Option<f64>,
    gain_kwh_per_utility_pct: Option<f64>,
    gain_watts_per_utility_rate_pct: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct ProcessImpactExport {
    exported_at: String,
    exported_at_ms: u64,
    process_count: usize,
    selected_target: SelectedTargetExport,
    summary: ProcessImpactSummaryExport,
    grouped_processes: Vec<ProcessImpactGroupExport>,
    top_process_rows: Vec<ProcessImpactRowExport>,
    top_contributors: Vec<ProcessImpactGroupExport>,
    /// SoulKernel lui-même — exclu du pool d'attribution pour ne pas biaiser les autres.
    monitoring_overhead: MonitoringOverheadExport,
    attribution_notice: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct MonitoringOverheadExport {
    cpu_pct: f64,
    memory_mib: f64,
    note: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct SelectedTargetExport {
    target_pid: Option<u32>,
    target_label: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct ProcessImpactSummaryExport {
    process_count: usize,
    top_count: usize,
    observed_cpu_count: usize,
    observed_gpu_count: usize,
    observed_memory_count: usize,
    observed_io_count: usize,
    machine_power_w: Option<f64>,
    attribution_method: &'static str,
    report_revision: String,
    ui_revision: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct ProcessImpactGroupExport {
    key: String,
    process_count: usize,
    cpu_usage_pct: f64,
    memory_kb: u64,
    estimated_power_w: Option<f64>,
    impact_score_pct_estimated: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct ProcessImpactRowExport {
    pid: u32,
    name: String,
    cpu_label: String,
    ram_label: String,
    ram_share_label: String,
    io_label: String,
    io_split_label: String,
    power_label: String,
    impact_label: String,
    duration_label: String,
    status_label: String,
    role: &'static str,
    attribution_method: &'static str,
    is_self_process: bool,
    is_embedded_webview: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct LiteReport<'a> {
    exported_at: String,
    exported_at_ms: u64,
    workload: &'a str,
    dome_active: bool,
    soulram_active: bool,
    policy_mode: &'a str,
    kappa: f64,
    sigma_max: f64,
    eta: f64,
    target_pid: Option<u32>,
    auto_target: bool,
    manual_target_pid: Option<u32>,
    platform_info: &'a soulkernel_core::platform::PlatformInfo,
    soulram_backend: &'a soulkernel_core::platform::SoulRamBackendInfo,
    metrics: &'a soulkernel_core::metrics::ResourceState,
    formula: &'a soulkernel_core::formula::FormulaResult,
    telemetry: &'a soulkernel_core::telemetry::TelemetrySummary,
    processes: &'a soulkernel_core::processes::ProcessObservedReport,
    device_inventory: &'a soulkernel_core::inventory::DeviceInventoryReport,
    external_config: ExternalConfigExport<'a>,
    external_status: &'a soulkernel_core::external_power::ExternalPowerStatus,
    external_bridge_running: bool,
    external_bridge_detail: &'a str,
    benchmark_last_session: &'a Option<soulkernel_core::benchmark::BenchmarkSession>,
    benchmark_history: &'a Option<soulkernel_core::benchmark::BenchmarkHistoryResponse>,
    audit_path: &'a str,
    last_actions: &'a [String],
}

/// Mirrors exactly what the "Matériel interne / externe → Écart" panel displays.
/// All derived fields are computed from raw measurements — no formulas.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct PowerComparisonExport {
    host_power_w: Option<f64>,
    wall_power_w: Option<f64>,
    /// Host power as a fraction of wall power (0–100 %).  None if either source is missing.
    host_of_wall_pct: Option<f64>,
    /// Watts measured at the wall but not attributed to the host sensor.  None if either source is missing.
    unattributed_w: Option<f64>,
    /// "bonne" | "à rafraîchir" | "mur seul" | "hôte seul" | "faible"
    confidence: &'static str,
    /// Whether the host sensor (RAPL / PDH / battery discharge) produced a reading.
    host_sensor_available: bool,
    /// Whether an external wall-plug sensor produced a reading.
    wall_sensor_available: bool,
}

/// KPI énergétique instantané : W / CPU_utile, avec pénalité faults et apprentissage.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct KpiExport {
    /// KPI(t) = P(t) / CPU_utile — None si pas de source de puissance.
    kpi_basic_w_per_pct: Option<f64>,
    /// KPI*(t) = KPI × (1 + λ × Faults/10000) — pénalisé par le thrashing.
    kpi_penalized_w_per_pct: Option<f64>,
    /// J(t) normalisé [0,1] : α×KPI + β×Faults + γ×RAM_inactive.
    objective_j: Option<f64>,
    /// Label : "EFFICACE" | "MODÉRÉ" | "INEFFICACE" | "—"
    label: &'static str,
    cpu_total_pct: f64,
    cpu_useful_pct: f64,
    cpu_overhead_pct: f64,
    cpu_system_pct: f64,
    cpu_self_pct: f64,
    self_overload: bool,
    /// Δ KPI* par rapport à la mesure précédente (positif = dégradation).
    trend: Option<f64>,
    /// Ratio d'actions ayant amélioré le KPI (mémoire d'apprentissage).
    reward_ratio: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct LiteJsonExport<'a> {
    product: &'static str,
    report_type: &'static str,
    period: Option<&'static str>,
    period_label: Option<&'static str>,
    /// Synthèse des gains — premier champ pour une lecture immédiate.
    gains_summary: soulkernel_core::telemetry::GainsSummary,
    report: LiteReport<'a>,
    power_comparison: PowerComparisonExport,
    kpi: KpiExport,
    raw_host_metrics: LiteRawHostMetricsExport<'a>,
    external_power: ExternalPowerExport<'a>,
    strict_evidence: StrictEvidenceExport<'a>,
    process_impact_report: ProcessImpactExport,
}

pub fn default_observability_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata)
            .join("SoulKernel")
            .join("telemetry")
            .join("observability_samples.jsonl");
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(xdg)
                .join("SoulKernel")
                .join("telemetry")
                .join("observability_samples.jsonl");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("SoulKernel")
                .join("telemetry")
                .join("observability_samples.jsonl");
        }
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("soulkernel_observability_samples.jsonl")
}

const OBSERVABILITY_ROTATE_BYTES: u64 = 16 * 1024 * 1024;
const OBSERVABILITY_ARCHIVE_KEEP: usize = 8;

pub fn observability_rotation_bytes() -> u64 {
    OBSERVABILITY_ROTATE_BYTES
}

fn rotate_observability_if_needed(path: &PathBuf) -> Result<(), String> {
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(_) => return Ok(()),
    };
    if meta.len() < OBSERVABILITY_ROTATE_BYTES {
        return Ok(());
    }

    let ts_ms = soulkernel_core::telemetry::now_ms();
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("observability_samples");
    let archive_path = path.with_file_name(format!("{stem}-{ts_ms}.jsonl.gz"));

    let mut src = File::open(path).map_err(|e| e.to_string())?;
    let archive_file = File::create(&archive_path).map_err(|e| e.to_string())?;
    let mut encoder = GzEncoder::new(archive_file, Compression::default());
    let mut buf = Vec::new();
    src.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    encoder.write_all(&buf).map_err(|e| e.to_string())?;
    encoder.finish().map_err(|e| e.to_string())?;

    std::fs::write(path, b"").map_err(|e| e.to_string())?;
    cleanup_observability_archives(path)?;
    Ok(())
}

fn cleanup_observability_archives(path: &PathBuf) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("observability_samples");
    let mut archives = std::fs::read_dir(parent)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|name| name.starts_with(stem) && name.ends_with(".jsonl.gz"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    archives.sort();
    let excess = archives.len().saturating_sub(OBSERVABILITY_ARCHIVE_KEEP);
    for archive in archives.into_iter().take(excess) {
        let _ = std::fs::remove_file(archive);
    }
    Ok(())
}

fn format_watts(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:.2} W"))
        .unwrap_or_else(|| "—".to_string())
}

fn format_timestamp_label(ts_ms: Option<u64>) -> String {
    ts_ms
        .map(format_iso_timestamp)
        .unwrap_or_else(|| "—".to_string())
}

fn format_iso_timestamp(ts_ms: u64) -> String {
    Local
        .timestamp_millis_opt(ts_ms as i64)
        .single()
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| Utc::now().to_rfc3339())
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

fn format_runtime_compact(run_time_s: u64) -> String {
    let h = run_time_s / 3600;
    let m = (run_time_s % 3600) / 60;
    let s = run_time_s % 60;
    if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}

fn process_role(sample: &soulkernel_core::processes::ProcessSample) -> &'static str {
    if sample.is_self_process {
        "self"
    } else if sample.is_embedded_webview {
        "webview"
    } else {
        "other"
    }
}

fn hash_f64(hasher: &mut DefaultHasher, value: f64) {
    if value.is_finite() {
        value.to_bits().hash(hasher);
    } else {
        0u64.hash(hasher);
    }
}

fn build_raw_host_metrics_export<'a>(vm: &'a LiteViewModel) -> LiteRawHostMetricsExport<'a> {
    let raw = &vm.metrics.raw;
    let observed_metric_keys = [
        ("cpu_pct", Some(())),
        ("cpu_clock_mhz", raw.cpu_clock_mhz.map(|_| ())),
        ("cpu_max_clock_mhz", raw.cpu_max_clock_mhz.map(|_| ())),
        ("cpu_freq_ratio", raw.cpu_freq_ratio.map(|_| ())),
        ("cpu_temp_c", raw.cpu_temp_c.map(|_| ())),
        ("mem_used_mb", Some(())),
        ("mem_total_mb", Some(())),
        ("ram_clock_mhz", raw.ram_clock_mhz.map(|_| ())),
        ("swap_used_mb", Some(())),
        ("swap_total_mb", Some(())),
        ("zram_used_mb", raw.zram_used_mb.map(|_| ())),
        ("io_read_mb_s", raw.io_read_mb_s.map(|_| ())),
        ("io_write_mb_s", raw.io_write_mb_s.map(|_| ())),
        ("gpu_pct", raw.gpu_pct.map(|_| ())),
        ("gpu_core_clock_mhz", raw.gpu_core_clock_mhz.map(|_| ())),
        ("gpu_mem_clock_mhz", raw.gpu_mem_clock_mhz.map(|_| ())),
        ("gpu_temp_c", raw.gpu_temp_c.map(|_| ())),
        ("gpu_power_watts", raw.gpu_power_watts.map(|_| ())),
        (
            "gpu_power_source",
            raw.gpu_power_source.as_ref().map(|_| ()),
        ),
        (
            "gpu_power_confidence",
            raw.gpu_power_confidence.as_ref().map(|_| ()),
        ),
        ("gpu_mem_used_mb", raw.gpu_mem_used_mb.map(|_| ())),
        ("gpu_mem_total_mb", raw.gpu_mem_total_mb.map(|_| ())),
        ("gpu_devices", (!raw.gpu_devices.is_empty()).then_some(())),
        ("power_watts", raw.power_watts.map(|_| ())),
        (
            "power_watts_source",
            raw.power_watts_source.as_ref().map(|_| ()),
        ),
        ("host_power_watts", raw.host_power_watts.map(|_| ())),
        (
            "host_power_watts_source",
            raw.host_power_watts_source.as_ref().map(|_| ()),
        ),
        ("wall_power_watts", raw.wall_power_watts.map(|_| ())),
        (
            "wall_power_watts_source",
            raw.wall_power_watts_source.as_ref().map(|_| ()),
        ),
        ("psi_cpu", raw.psi_cpu.map(|_| ())),
        ("psi_mem", raw.psi_mem.map(|_| ())),
        ("load_avg_1m_norm", raw.load_avg_1m_norm.map(|_| ())),
        ("runnable_tasks", raw.runnable_tasks.map(|_| ())),
        ("on_battery", raw.on_battery.map(|_| ())),
        ("battery_percent", raw.battery_percent.map(|_| ())),
        ("page_faults_per_sec", raw.page_faults_per_sec.map(|_| ())),
        ("webview_host_cpu_sum", raw.webview_host_cpu_sum.map(|_| ())),
        ("webview_host_mem_mb", raw.webview_host_mem_mb.map(|_| ())),
    ]
    .into_iter()
    .filter_map(|(key, present)| present.map(|_| key))
    .collect::<Vec<_>>();

    LiteRawHostMetricsExport {
        exported_at: format_iso_timestamp(vm.now_ms),
        exported_at_ms: vm.now_ms,
        available: true,
        platform: &raw.platform,
        observed_metric_count: observed_metric_keys.len(),
        observed_metric_keys,
        device_inventory: &vm.device_inventory,
        normalized: NormalizedMetricsExport {
            cpu: vm.metrics.cpu,
            mem: vm.metrics.mem,
            compression: vm.metrics.compression,
            io_bandwidth: vm.metrics.io_bandwidth,
            gpu: vm.metrics.gpu,
            sigma: vm.metrics.sigma,
            epsilon: &vm.metrics.epsilon,
        },
        raw: RawMetricsExport {
            cpu_pct: raw.cpu_pct,
            cpu_clock_mhz: raw.cpu_clock_mhz,
            cpu_max_clock_mhz: raw.cpu_max_clock_mhz,
            cpu_freq_ratio: raw.cpu_freq_ratio,
            cpu_temp_c: raw.cpu_temp_c,
            mem_used_mb: raw.mem_used_mb,
            mem_total_mb: raw.mem_total_mb,
            ram_clock_mhz: raw.ram_clock_mhz,
            swap_used_mb: raw.swap_used_mb,
            swap_total_mb: raw.swap_total_mb,
            zram_used_mb: raw.zram_used_mb,
            io_read_mb_s: raw.io_read_mb_s,
            io_write_mb_s: raw.io_write_mb_s,
            gpu_pct: raw.gpu_pct,
            gpu_core_clock_mhz: raw.gpu_core_clock_mhz,
            gpu_mem_clock_mhz: raw.gpu_mem_clock_mhz,
            gpu_temp_c: raw.gpu_temp_c,
            gpu_power_watts: raw.gpu_power_watts,
            gpu_power_source: raw.gpu_power_source.as_deref(),
            gpu_power_confidence: raw.gpu_power_confidence.as_deref(),
            gpu_mem_used_mb: raw.gpu_mem_used_mb,
            gpu_mem_total_mb: raw.gpu_mem_total_mb,
            gpu_devices: &raw.gpu_devices,
            power_watts: raw.power_watts,
            power_watts_source: raw.power_watts_source.as_deref(),
            host_power_watts: raw.host_power_watts,
            host_power_watts_source: raw.host_power_watts_source.as_deref(),
            wall_power_watts: raw.wall_power_watts,
            wall_power_watts_source: raw.wall_power_watts_source.as_deref(),
            psi_cpu: raw.psi_cpu,
            psi_mem: raw.psi_mem,
            load_avg_1m_norm: raw.load_avg_1m_norm,
            runnable_tasks: raw.runnable_tasks,
            on_battery: raw.on_battery,
            battery_percent: raw.battery_percent,
            page_faults_per_sec: raw.page_faults_per_sec,
            webview_host_cpu_sum: raw.webview_host_cpu_sum,
            webview_host_mem_mb: raw.webview_host_mem_mb,
        },
    }
}

fn build_external_power_export<'a>(vm: &'a LiteViewModel) -> ExternalPowerExport<'a> {
    let runtime = if vm.external_status.python_bin.contains("runtime/python")
        || vm
            .external_status
            .python_bin
            .contains("python\\windows\\python.exe")
        || vm
            .external_status
            .python_bin
            .contains("python/macos/bin/python3")
        || vm
            .external_status
            .python_bin
            .contains("python/linux/bin/python3")
    {
        "Runtime embarqué"
    } else {
        "Runtime système"
    };
    let last_error = if vm.external_bridge_running {
        "—"
    } else {
        vm.external_bridge_detail.as_str()
    };
    ExternalPowerExport {
        source_tag: &vm.external_status.source_tag,
        last_watts_label: format_watts(vm.external_status.last_watts),
        freshness: if vm.external_status.is_fresh {
            "frais"
        } else {
            "a_rafraichir"
        },
        file_presence: if vm.external_status.power_file_exists {
            "présent"
        } else {
            "absent"
        },
        bridge_state: if vm.external_bridge_running {
            "ON"
        } else {
            "OFF"
        },
        runtime,
        python_bin: &vm.external_status.python_bin,
        config_path: &vm.external_status.config_path,
        power_file_path: &vm.external_status.power_file_path,
        cache_path: &vm.external_status.creds_cache_path,
        bridge_log_path: &vm.external_status.bridge_log_path,
        last_ts_ms: vm.external_status.last_ts_ms,
        last_ts_label: format_timestamp_label(vm.external_status.last_ts_ms),
        last_error,
        bridge_detail: &vm.external_bridge_detail,
    }
}

fn infer_machine_activity(vm: &LiteViewModel) -> &'static str {
    let cpu = vm.metrics.raw.cpu_pct;
    let gpu_pct = vm.metrics.raw.gpu_pct.unwrap_or(0.0);
    let io_total = vm.metrics.raw.io_read_mb_s.unwrap_or(0.0)
        + vm.metrics.raw.io_write_mb_s.unwrap_or(0.0);
    let webview_mem_mb = vm.metrics.raw.webview_host_mem_mb.unwrap_or(0) as f64;
    let gpu_adjusted = if webview_mem_mb >= 48.0 {
        (gpu_pct - 18.0).max(0.0)
    } else {
        gpu_pct
    };
    if cpu < 12.0 && gpu_adjusted > 34.0 {
        "media"
    } else if cpu < 8.0 && io_total < 0.5 && gpu_pct < 8.0 {
        "idle"
    } else {
        "active"
    }
}

fn build_strict_evidence_export<'a>(
    vm: &'a LiteViewModel,
    raw_host_metrics: LiteRawHostMetricsExport<'a>,
    external_power: ExternalPowerExport<'a>,
) -> StrictEvidenceExport<'a> {
    let benchmark_ab = vm
        .benchmark_last_session
        .as_ref()
        .map(|session| BenchmarkEvidence {
            samples_off_ok: session.summary.samples_off_ok,
            samples_on_ok: session.summary.samples_on_ok,
            gain_median_pct: session.summary.gain_median_pct,
            gain_p95_pct: session.summary.gain_p95_pct,
            gain_power_median_pct: session.summary.gain_power_median_pct,
            gain_cpu_median_pct: session.summary.gain_cpu_median_pct,
            gain_mem_median_pct: session.summary.gain_mem_median_pct,
            gain_sigma_median_pct: session.summary.gain_sigma_median_pct,
            measured_efficiency_off: session.summary.measured_efficiency_off.clone(),
            measured_efficiency_on: session.summary.measured_efficiency_on.clone(),
            gain_utility_per_watt_pct: session.summary.gain_utility_per_watt_pct,
            gain_kwh_per_utility_pct: session.summary.gain_kwh_per_utility_pct,
            gain_watts_per_utility_rate_pct: session.summary.gain_watts_per_utility_rate_pct,
        });

    StrictEvidenceExport {
        mode: "strict_evidence",
        exported_at: format_iso_timestamp(vm.now_ms),
        exported_at_ms: vm.now_ms,
        machine_activity: infer_machine_activity(vm),
        assertions: StrictEvidenceAssertions {
            measured_os_metrics: MeasuredOsMetrics {
                cpu_pct: vm.metrics.raw.cpu_pct,
                mem_used_mb: vm.metrics.raw.mem_used_mb,
                mem_total_mb: vm.metrics.raw.mem_total_mb,
                io_read_mb_s: vm.metrics.raw.io_read_mb_s,
                io_write_mb_s: vm.metrics.raw.io_write_mb_s,
                gpu_pct: vm.metrics.raw.gpu_pct,
                sigma: vm.metrics.sigma,
            },
            raw_host_metrics,
            measured_energy: MeasuredEnergy {
                power_source: &vm.telemetry.power_source,
                live_power_w: vm.telemetry.live_power_w,
                total_energy_kwh: vm.telemetry.total.energy_kwh,
                total_cost: vm.telemetry.total.cost,
                total_co2_kg: vm.telemetry.total.co2_kg,
                period_windows: PeriodWindows {
                    hour_kwh: vm.telemetry.hour.energy_kwh,
                    day_kwh: vm.telemetry.day.energy_kwh,
                    week_kwh: vm.telemetry.week.energy_kwh,
                    month_kwh: vm.telemetry.month.energy_kwh,
                    year_kwh: vm.telemetry.year.energy_kwh,
                },
            },
            measured_differentials: MeasuredDifferentials {
                cpu_hours_differential: vm.telemetry.total.cpu_hours_differential,
                mem_gb_hours_differential: vm.telemetry.total.mem_gb_hours_differential,
                lifetime_cpu_hours_differential: vm.telemetry.lifetime.total_cpu_hours_differential,
                lifetime_mem_gb_hours_differential: vm.telemetry.lifetime.total_mem_gb_hours_differential,
                idle_ratio: vm.telemetry.total.idle_ratio,
                media_ratio: vm.telemetry.total.media_ratio,
                _phantom: None,
            },
            benchmark_ab,
        },
        allowed_claims: vec![
            "Mesures OS natives: CPU, RAM, GPU, I/O, sigma selon disponibilité plateforme.",
            "Consommation énergétique mesurée: kWh, coût et CO2 calculés depuis une puissance réelle.",
            "Aucune affirmation stricte de gain sans benchmark A/B exploitable.",
        ],
        forbidden_claims: vec![
            "Ne pas présenter π(t) ou ∫𝒟 comme une mesure physique.",
            "Ne pas présenter CPU·h ou RAM·GB·h différentielles comme des économies matérielles absolues.",
            "Ne pas présenter coût ou CO2 mesurés comme des gains évités sans baseline énergétique OFF vs ON.",
        ],
        external_power,
    }
}

fn build_process_report_revision(
    processes: &[soulkernel_core::processes::ProcessSample],
    impacts: &[(u32, f64, Option<f64>)],
) -> String {
    let mut hasher = DefaultHasher::new();
    processes.len().hash(&mut hasher);
    for p in processes {
        p.pid.hash(&mut hasher);
        p.name.hash(&mut hasher);
        hash_f64(&mut hasher, p.cpu_usage_pct);
        p.memory_kb.hash(&mut hasher);
        p.disk_read_bytes.hash(&mut hasher);
        p.disk_written_bytes.hash(&mut hasher);
    }
    for (pid, impact_pct, power_w) in impacts {
        pid.hash(&mut hasher);
        hash_f64(&mut hasher, *impact_pct);
        hash_f64(&mut hasher, power_w.unwrap_or_default());
    }
    format!("{:016x}", hasher.finish())
}

fn build_process_ui_revision(rows: &[ProcessImpactRowExport]) -> String {
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

fn build_process_impact_export(vm: &LiteViewModel) -> ProcessImpactExport {
    let processes = &vm.process_report.top_processes;
    let machine_power_w = vm
        .metrics
        .raw
        .power_watts
        .or(if vm.external_status.is_fresh {
            vm.external_status.last_watts
        } else {
            None
        });

    // Exclude SoulKernel itself and its embedded WebView from the attribution pool.
    // They are monitoring overhead — including them distorts every other process's share.
    let user_processes: Vec<_> = processes
        .iter()
        .filter(|p| !p.is_self_process && !p.is_embedded_webview)
        .collect();

    let cpu_sum = user_processes
        .iter()
        .map(|p| p.cpu_usage_pct.max(0.0))
        .sum::<f64>()
        .max(0.0001);
    let mem_sum = user_processes
        .iter()
        .map(|p| p.memory_kb as f64)
        .sum::<f64>()
        .max(0.0001);
    let io_sum = user_processes
        .iter()
        .map(|p| p.disk_read_bytes.saturating_add(p.disk_written_bytes) as f64)
        .sum::<f64>()
        .max(0.0001);

    let mut impacts = Vec::new();
    for proc_ in &user_processes {
        let cpu_share = (proc_.cpu_usage_pct.max(0.0) / cpu_sum) * 100.0;
        let mem_share = (proc_.memory_kb as f64 / mem_sum) * 100.0;
        let io_share = (proc_
            .disk_read_bytes
            .saturating_add(proc_.disk_written_bytes) as f64
            / io_sum)
            * 100.0;
        let impact_pct = (0.7 * cpu_share + 0.2 * mem_share + 0.1 * io_share).max(0.0);
        let power_w = machine_power_w.map(|w| (impact_pct / 100.0) * w);
        impacts.push((proc_.pid, impact_pct, power_w));
    }
    let impact_sum = impacts
        .iter()
        .map(|(_, pct, _)| *pct)
        .sum::<f64>()
        .max(0.0001);

    let top_process_rows = user_processes
        .iter()
        .map(|proc_| {
            let (_, impact_pct, power_w) = impacts
                .iter()
                .find(|(pid, _, _)| *pid == proc_.pid)
                .copied()
                .unwrap_or((proc_.pid, 0.0, None));
            let memory_share = if mem_sum > 0.0 {
                (proc_.memory_kb as f64 / mem_sum) * 100.0
            } else {
                0.0
            };
            ProcessImpactRowExport {
                pid: proc_.pid,
                name: proc_.name.clone(),
                cpu_label: format!("{:.1} %", proc_.cpu_usage_pct),
                ram_label: if proc_.memory_kb > 0 {
                    format!("{:.0} MiB", proc_.memory_kb as f64 / 1024.0)
                } else {
                    "—".to_string()
                },
                ram_share_label: format!("{memory_share:.1}%"),
                io_label: format_bytes_iec(
                    proc_
                        .disk_read_bytes
                        .saturating_add(proc_.disk_written_bytes),
                ),
                io_split_label: format!(
                    "R {} / W {}",
                    format_bytes_iec(proc_.disk_read_bytes),
                    format_bytes_iec(proc_.disk_written_bytes)
                ),
                power_label: format_watts(power_w),
                impact_label: format!("{:.2} %", (impact_pct / impact_sum) * 100.0),
                duration_label: format_runtime_compact(proc_.run_time_s),
                status_label: proc_.status.clone(),
                role: process_role(proc_),
                attribution_method: "estimated_weighted_cpu_mem_io_over_measured_machine_power",
                is_self_process: proc_.is_self_process,
                is_embedded_webview: proc_.is_embedded_webview,
            }
        })
        .collect::<Vec<_>>();

    let mut grouped = BTreeMap::<String, ProcessImpactGroupExport>::new();
    for proc_ in &user_processes {
        let key = proc_.name.trim().to_lowercase();
        let (_, impact_pct, power_w) = impacts
            .iter()
            .find(|(pid, _, _)| *pid == proc_.pid)
            .copied()
            .unwrap_or((proc_.pid, 0.0, None));
        let entry = grouped
            .entry(key.clone())
            .or_insert(ProcessImpactGroupExport {
                key,
                process_count: 0,
                cpu_usage_pct: 0.0,
                memory_kb: 0,
                estimated_power_w: Some(0.0),
                impact_score_pct_estimated: Some(0.0),
            });
        entry.process_count += 1;
        entry.cpu_usage_pct += proc_.cpu_usage_pct.max(0.0);
        entry.memory_kb = entry.memory_kb.saturating_add(proc_.memory_kb);
        entry.estimated_power_w = match (entry.estimated_power_w, power_w) {
            (Some(a), Some(b)) => Some(a + b),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        entry.impact_score_pct_estimated = Some(
            entry.impact_score_pct_estimated.unwrap_or(0.0) + (impact_pct / impact_sum) * 100.0,
        );
    }
    let mut grouped_processes = grouped.into_values().collect::<Vec<_>>();
    grouped_processes.sort_by(|a, b| {
        b.impact_score_pct_estimated
            .unwrap_or_default()
            .partial_cmp(&a.impact_score_pct_estimated.unwrap_or_default())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let top_contributors = grouped_processes
        .iter()
        .take(5)
        .cloned()
        .collect::<Vec<_>>();
    let report_revision = build_process_report_revision(processes, &impacts);
    let ui_revision = build_process_ui_revision(&top_process_rows);
    let selected_target = SelectedTargetExport {
        target_pid: vm.target_pid,
        target_label: vm.target_pid.and_then(|pid| {
            processes.iter().find(|p| p.pid == pid).map(|p| {
                format!(
                    "{} (PID {}) — {:.1}% CPU · {:.0} MiB",
                    p.name,
                    p.pid,
                    p.cpu_usage_pct,
                    p.memory_kb as f64 / 1024.0
                )
            })
        }),
    };

    ProcessImpactExport {
        exported_at: format_iso_timestamp(vm.now_ms),
        exported_at_ms: vm.now_ms,
        process_count: vm.process_report.summary.process_count,
        selected_target,
        summary: ProcessImpactSummaryExport {
            process_count: vm.process_report.summary.process_count,
            top_count: processes.len(),
            observed_cpu_count: processes.len(),
            observed_gpu_count: 0,
            observed_memory_count: processes.iter().filter(|p| p.memory_kb > 0).count(),
            observed_io_count: processes
                .iter()
                .filter(|p| p.disk_read_bytes > 0 || p.disk_written_bytes > 0)
                .count(),
            machine_power_w,
            attribution_method: "estimated_weighted_cpu_mem_io_over_measured_machine_power",
            report_revision,
            ui_revision,
        },
        grouped_processes,
        top_process_rows,
        top_contributors,
        monitoring_overhead: {
            let self_cpu = processes
                .iter()
                .filter(|p| p.is_self_process || p.is_embedded_webview)
                .map(|p| p.cpu_usage_pct)
                .sum::<f64>();
            let self_mem_mib = processes
                .iter()
                .filter(|p| p.is_self_process || p.is_embedded_webview)
                .map(|p| p.memory_kb as f64 / 1024.0)
                .sum::<f64>();
            MonitoringOverheadExport {
                cpu_pct: self_cpu,
                memory_mib: self_mem_mib,
                note: "SoulKernel exclu du pool d'attribution — overhead de monitoring, pas un processus utilisateur.",
            }
        },
        attribution_notice: "CPU, RAM et I/O par processus sont observés. impact_score_pct_estimated et estimated_power_w restent des attributions estimées, pas une mesure énergétique directe par processus.",
    }
}

fn build_external_config_export<'a>(vm: &'a LiteViewModel) -> ExternalConfigExport<'a> {
    ExternalConfigExport {
        enabled: vm.external_config.enabled,
        power_file: vm.external_config.power_file.as_deref(),
        max_age_ms: vm.external_config.max_age_ms,
        meross_email: vm.external_config.meross_email.as_deref(),
        meross_region: vm.external_config.meross_region.as_deref(),
        meross_device_type: vm.external_config.meross_device_type.as_deref(),
        meross_http_proxy: vm.external_config.meross_http_proxy.as_deref(),
        mfa_present: vm
            .external_config
            .meross_mfa_code
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false),
        python_bin: vm.external_config.python_bin.as_deref(),
        bridge_interval_s: vm.external_config.bridge_interval_s,
        autostart_bridge: vm.external_config.autostart_bridge,
    }
}

fn build_power_comparison_export(vm: &LiteViewModel) -> PowerComparisonExport {
    let host = vm.metrics.raw.host_power_watts;
    let wall = vm.metrics.raw.wall_power_watts.or_else(|| {
        if vm.external_status.is_fresh {
            vm.external_status.last_watts
        } else {
            None
        }
    });
    let host_of_wall_pct = match (host, wall) {
        (Some(h), Some(w)) if w > 0.0 => Some((h / w * 100.0).clamp(0.0, 100.0)),
        _ => None,
    };
    let unattributed_w = match (host, wall) {
        (Some(h), Some(w)) => Some((w - h).max(0.0)),
        _ => None,
    };
    let confidence = if wall.is_some() && host.is_some() {
        if vm.external_status.is_fresh {
            "bonne"
        } else {
            "à rafraîchir"
        }
    } else if wall.is_some() && host.is_none() {
        "mur seul"
    } else if host.is_some() && wall.is_none() {
        "hôte seul"
    } else {
        "faible"
    };
    PowerComparisonExport {
        host_power_w: host,
        wall_power_w: wall,
        host_of_wall_pct,
        unattributed_w,
        confidence,
        host_sensor_available: host.is_some(),
        wall_sensor_available: wall.is_some(),
    }
}

fn build_kpi_export(vm: &LiteViewModel) -> KpiExport {
    let k = &vm.kpi;
    KpiExport {
        kpi_basic_w_per_pct: k.kpi_basic,
        kpi_penalized_w_per_pct: k.kpi_penalized,
        objective_j: k.objective_j,
        label: k.label.as_str(),
        cpu_total_pct: k.cpu_total_pct,
        cpu_useful_pct: k.cpu_useful_pct,
        cpu_overhead_pct: k.cpu_overhead_pct,
        cpu_system_pct: k.cpu_system_pct,
        cpu_self_pct: k.cpu_self_pct,
        self_overload: k.self_overload,
        trend: k.trend,
        reward_ratio: vm.kpi_memory.reward_ratio(),
    }
}

fn build_gains_summary_export(vm: &LiteViewModel) -> soulkernel_core::telemetry::GainsSummary {
    // Délègue vers la logique partagée dans soulkernel-core.
    // soulkernel-lite fournit les données kpi_memory (non disponibles côté Tauri).
    vm.telemetry.to_gains_summary(
        vm.kpi_memory.reward_ratio() * 100.0,
        vm.kpi_memory.avg_kpi_gain(),
    )
}

fn build_payload<'a>(
    vm: &'a LiteViewModel,
    report_type: &'static str,
    period: Option<&'static str>,
    period_label: Option<&'static str>,
) -> LiteJsonExport<'a> {
    let raw_host_metrics = build_raw_host_metrics_export(vm);
    let external_power = build_external_power_export(vm);
    let strict_evidence = build_strict_evidence_export(
        vm,
        build_raw_host_metrics_export(vm),
        build_external_power_export(vm),
    );
    let process_impact_report = build_process_impact_export(vm);

    LiteJsonExport {
        product: "SoulKernel",
        report_type,
        period,
        period_label,
        gains_summary: build_gains_summary_export(vm),
        report: LiteReport {
            exported_at: format_iso_timestamp(vm.now_ms),
            exported_at_ms: vm.now_ms,
            workload: &vm.selected_workload,
            dome_active: vm.dome_active,
            soulram_active: vm.soulram_active,
            policy_mode: vm.policy_mode.as_name(),
            kappa: vm.kappa,
            sigma_max: vm.sigma_max,
            eta: vm.eta,
            target_pid: vm.target_pid,
            auto_target: vm.auto_target,
            manual_target_pid: vm.manual_target_pid,
            platform_info: &vm.platform_info,
            soulram_backend: &vm.soulram_backend,
            metrics: &vm.metrics,
            formula: &vm.formula,
            telemetry: &vm.telemetry,
            processes: &vm.process_report,
            device_inventory: &vm.device_inventory,
            external_config: build_external_config_export(vm),
            external_status: &vm.external_status,
            external_bridge_running: vm.external_bridge_running,
            external_bridge_detail: &vm.external_bridge_detail,
            benchmark_last_session: &vm.benchmark_last_session,
            benchmark_history: &vm.benchmark_history,
            audit_path: &vm.audit_path,
            last_actions: &vm.last_actions,
        },
        power_comparison: build_power_comparison_export(vm),
        kpi: build_kpi_export(vm),
        raw_host_metrics,
        external_power,
        strict_evidence,
        process_impact_report,
    }
}

pub fn export_snapshot(vm: &LiteViewModel) -> Result<String, String> {
    let path = FileDialog::new()
        .set_file_name("soulkernel-lite-export.json")
        .save_file()
        .ok_or_else(|| "export annulé".to_string())?;

    let payload = build_payload(vm, "session_report", Some("live"), Some("instant"));

    let bytes = serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

pub fn append_observability_sample(vm: &LiteViewModel) -> Result<String, String> {
    let path = default_observability_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    rotate_observability_if_needed(&path)?;
    let payload = build_payload(vm, "observability_sample", Some("timeseries"), Some("tick"));
    let line = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    writeln!(file, "{line}").map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}
