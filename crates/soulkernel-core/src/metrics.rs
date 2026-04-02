//! metrics.rs - Cross-platform hardware metrics -> ResourceState r(t)
//!
//! Maps raw OS data to the vector:
//!   r(t) = [C(t), M(t), Lambda(t), B_io(t), G(t)]  in [0,1]^5
//!
//! No simulation: either real native data or Option/Err.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use sysinfo::{get_current_pid, Pid, System};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GpuDeviceMetrics {
    pub index: u32,
    pub name: Option<String>,
    pub vendor: Option<String>,
    pub kind: Option<String>,
    pub utilization_pct: Option<f64>,
    pub power_watts: Option<f64>,
    pub memory_used_mb: Option<u64>,
    pub memory_total_mb: Option<u64>,
    pub core_clock_mhz: Option<f64>,
    pub mem_clock_mhz: Option<f64>,
    pub temperature_c: Option<f64>,
    pub source: Option<String>,
    pub confidence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceState {
    /// C(t) - CPU utilisation [0,1]
    pub cpu: f64,
    /// M(t) - effective memory availability [0,1]
    pub mem: f64,
    /// Lambda(t) - compression ratio/store [0,1]. None = unavailable.
    pub compression: Option<f64>,
    /// B_io(t) - normalised I/O bandwidth [0,1]. None = unavailable.
    pub io_bandwidth: Option<f64>,
    /// G(t) - GPU utilisation [0,1]. None = unavailable.
    pub gpu: Option<f64>,
    /// Sigma(t) - global stress (PSI on Linux, mem pressure fallback elsewhere)
    pub sigma: f64,
    /// epsilon contention vector per dimension [0,1]^5
    pub epsilon: [f64; 5],
    /// Raw values for display - real values only, Option = N/A
    pub raw: RawMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMetrics {
    pub cpu_pct: f64,
    /// Average CPU clock in MHz when available.
    pub cpu_clock_mhz: Option<f64>,
    /// Maximum CPU clock in MHz when available.
    pub cpu_max_clock_mhz: Option<f64>,
    /// Average CPU frequency ratio versus max clock [0,1].
    pub cpu_freq_ratio: Option<f64>,
    /// CPU package temperature in Celsius when available.
    pub cpu_temp_c: Option<f64>,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    /// Effective RAM speed in MHz when available.
    pub ram_clock_mhz: Option<f64>,
    pub swap_used_mb: u64,
    pub swap_total_mb: u64,
    /// Linux zRAM only. None = no zram / not Linux.
    pub zram_used_mb: Option<u64>,
    /// Real throughput; None = unavailable.
    pub io_read_mb_s: Option<f64>,
    pub io_write_mb_s: Option<f64>,
    /// None = GPU metric unavailable.
    pub gpu_pct: Option<f64>,
    /// GPU core clock in MHz when available.
    pub gpu_core_clock_mhz: Option<f64>,
    /// GPU memory clock in MHz when available.
    pub gpu_mem_clock_mhz: Option<f64>,
    /// GPU temperature in Celsius when available.
    pub gpu_temp_c: Option<f64>,
    /// GPU board/package power in Watts when available.
    pub gpu_power_watts: Option<f64>,
    /// Source du watt GPU agrégé (`nvml`, `sysfs`, `reconciled_estimate`, etc.).
    #[serde(default)]
    pub gpu_power_source: Option<String>,
    /// Niveau de confiance du watt GPU (`direct_measured`, `derived_measured`, `reconciled_estimated`, `activity_only`).
    #[serde(default)]
    pub gpu_power_confidence: Option<String>,
    /// VRAM used in MiB when available.
    pub gpu_mem_used_mb: Option<u64>,
    /// VRAM total in MiB when available.
    pub gpu_mem_total_mb: Option<u64>,
    /// Détail multi-GPU observé sur la machine quand disponible.
    #[serde(default)]
    pub gpu_devices: Vec<GpuDeviceMetrics>,
    /// System power draw from host power meter (Watts). None = unavailable.
    pub power_watts: Option<f64>,
    /// Origine de `power_watts` quand connue : ex. `meross_wall`, `rapl`, `windows_meter`.
    #[serde(default)]
    pub power_watts_source: Option<String>,
    /// Puissance mesurée côté hôte / unité centrale quand disponible.
    #[serde(default)]
    pub host_power_watts: Option<f64>,
    /// Source de la mesure hôte (`rapl`, `windows_meter`, etc.).
    #[serde(default)]
    pub host_power_watts_source: Option<String>,
    /// Puissance murale / externe quand disponible.
    #[serde(default)]
    pub wall_power_watts: Option<f64>,
    /// Source de la mesure externe (`meross_wall`, etc.).
    #[serde(default)]
    pub wall_power_watts_source: Option<String>,
    /// Linux PSI only. None = unavailable.
    pub psi_cpu: Option<f64>,
    pub psi_mem: Option<f64>,
    /// Normalized load average over 1 minute: load1 / logical_cpu_count.
    pub load_avg_1m_norm: Option<f64>,
    /// Runnable tasks in `/proc/loadavg` when available.
    pub runnable_tasks: Option<u64>,
    /// Battery mode hint when available (primarily Windows).
    pub on_battery: Option<bool>,
    /// Battery charge percentage when available.
    pub battery_percent: Option<f64>,
    /// Windows PDH `\Memory\Page Faults/sec` quand disponible.
    pub page_faults_per_sec: Option<f64>,
    pub platform: String,
    /// Somme des `cpu_usage` sysinfo des processus WebView/WebKit descendants de SoulKernel
    /// (ex. `msedgewebview2`, WebKit sur Linux/macOS). Aide à interpréter la charge « UI ».
    #[serde(default)]
    pub webview_host_cpu_sum: Option<f64>,
    /// RSS agrégée des mêmes processus (MiB).
    #[serde(default)]
    pub webview_host_mem_mb: Option<u64>,
}

/// Processus hébergés par le runtime WebView (hors onglets navigateur classiques).
fn is_embedded_webview_name(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("msedgewebview")
        || n.contains("webview2")
        || n.contains("webkitnetworkprocess")
        || n.contains("webkit.webcontent")
        || n.contains("webkitwebprocess")
        || (n.contains("webkit") && n.contains("gpu"))
}

/// Agrège CPU (somme des % sysinfo par cœur) et mémoire des descendants WebView du PID courant.
fn webview_host_aggregate(sys: &System) -> (Option<f64>, Option<u64>) {
    let root = match get_current_pid() {
        Ok(p) => p,
        Err(_) => return (None, None),
    };
    let mut by_parent: HashMap<Pid, Vec<Pid>> = HashMap::new();
    for (pid, proc) in sys.processes() {
        if let Some(par) = proc.parent() {
            by_parent.entry(par).or_default().push(*pid);
        }
    }
    let mut q = VecDeque::new();
    q.push_back(root);
    let mut seen = HashSet::new();
    let mut cpu_sum = 0.0_f64;
    let mut mem_sum = 0_u64;
    let mut webview_nodes = 0_usize;
    while let Some(pid) = q.pop_front() {
        if !seen.insert(pid) {
            continue;
        }
        let Some(proc) = sys.processes().get(&pid) else {
            continue;
        };
        if pid != root && is_embedded_webview_name(proc.name()) {
            webview_nodes += 1;
            cpu_sum += proc.cpu_usage() as f64;
            mem_sum = mem_sum.saturating_add(proc.memory());
        }
        if let Some(kids) = by_parent.get(&pid) {
            for c in kids {
                q.push_back(*c);
            }
        }
    }
    if webview_nodes == 0 {
        (None, None)
    } else {
        (Some(cpu_sum), Some(mem_sum / 1024 / 1024))
    }
}

#[derive(Debug, Clone, Default)]
struct AggregatedGpuMetrics {
    gpu_pct: Option<f64>,
    gpu_power_watts: Option<f64>,
    gpu_power_source: Option<String>,
    gpu_power_confidence: Option<String>,
    gpu_core_clock_mhz: Option<f64>,
    gpu_mem_clock_mhz: Option<f64>,
    gpu_temp_c: Option<f64>,
    gpu_mem_used_mb: Option<u64>,
    gpu_mem_total_mb: Option<u64>,
    gpu_devices: Vec<GpuDeviceMetrics>,
}

fn aggregate_gpu_devices(devices: Vec<GpuDeviceMetrics>) -> AggregatedGpuMetrics {
    let gpu_pct_values: Vec<f64> = devices
        .iter()
        .filter_map(|d| d.utilization_pct)
        .filter(|v| v.is_finite() && *v >= 0.0)
        .collect();
    let power_values: Vec<f64> = devices
        .iter()
        .filter_map(|d| d.power_watts)
        .filter(|v| v.is_finite() && *v >= 0.0)
        .collect();
    let gpu_pct = if gpu_pct_values.is_empty() {
        None
    } else {
        Some(gpu_pct_values.iter().sum::<f64>() / gpu_pct_values.len() as f64)
    };
    let gpu_power_watts = if power_values.is_empty() {
        None
    } else {
        Some(power_values.iter().sum::<f64>())
    };
    let gpu_power_source = devices.iter().find_map(|d| d.source.clone());
    let gpu_power_confidence = if gpu_power_watts.is_some() {
        devices.iter().find_map(|d| d.confidence.clone())
    } else if gpu_pct.is_some() {
        Some("activity_only".to_string())
    } else {
        None
    };
    let gpu_core_clock_mhz = devices
        .iter()
        .filter_map(|d| d.core_clock_mhz)
        .filter(|v| v.is_finite() && *v > 0.0)
        .reduce(f64::max);
    let gpu_mem_clock_mhz = devices
        .iter()
        .filter_map(|d| d.mem_clock_mhz)
        .filter(|v| v.is_finite() && *v > 0.0)
        .reduce(f64::max);
    let gpu_temp_c = devices
        .iter()
        .filter_map(|d| d.temperature_c)
        .filter(|v| v.is_finite() && *v > 0.0)
        .reduce(f64::max);
    let gpu_mem_used_mb = {
        let total = devices.iter().filter_map(|d| d.memory_used_mb).sum::<u64>();
        if total > 0 {
            Some(total)
        } else {
            None
        }
    };
    let gpu_mem_total_mb = {
        let total = devices
            .iter()
            .filter_map(|d| d.memory_total_mb)
            .sum::<u64>();
        if total > 0 {
            Some(total)
        } else {
            None
        }
    };

    AggregatedGpuMetrics {
        gpu_pct,
        gpu_power_watts,
        gpu_power_source,
        gpu_power_confidence,
        gpu_core_clock_mhz,
        gpu_mem_clock_mhz,
        gpu_temp_c,
        gpu_mem_used_mb,
        gpu_mem_total_mb,
        gpu_devices: devices,
    }
}

fn estimate_reconciled_gpu_power_watts(
    machine_power_watts: Option<f64>,
    gpu_pct: Option<f64>,
    cpu_pct: f64,
    io_read_mb_s: Option<f64>,
    io_write_mb_s: Option<f64>,
    mem_used: u64,
    mem_total: u64,
    on_battery: Option<bool>,
) -> Option<f64> {
    let machine_power = machine_power_watts?;
    let gpu_norm = (gpu_pct? / 100.0).clamp(0.0, 1.0);
    if gpu_norm <= 0.01 || !machine_power.is_finite() || machine_power <= 0.0 {
        return None;
    }
    let cpu_norm = (cpu_pct / 100.0).clamp(0.0, 1.0);
    let io_norm =
        ((io_read_mb_s.unwrap_or(0.0) + io_write_mb_s.unwrap_or(0.0)) / 1500.0).clamp(0.0, 1.0);
    let mem_norm = if mem_total > 0 {
        (mem_used as f64 / mem_total as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let idle_floor = if on_battery == Some(true) { 8.0 } else { 15.0 };
    let active_power = (machine_power - idle_floor).max(0.0);
    if active_power <= 0.0 {
        return None;
    }
    let gpu_weight =
        gpu_norm / (gpu_norm + 0.8 * cpu_norm + 0.25 * io_norm + 0.15 * mem_norm + 0.10);
    let estimate = (active_power * gpu_weight).clamp(0.0, machine_power * 0.85);
    if estimate > 0.0 {
        Some(estimate)
    } else {
        None
    }
}

pub fn collect() -> Result<ResourceState> {
    let mut sys = System::new_all();
    sys.refresh_all();

    let (webview_host_cpu_sum, webview_host_mem_mb) = webview_host_aggregate(&sys);

    // CPU
    let cpu_pct = sys.cpus().iter().map(|c| c.cpu_usage() as f64).sum::<f64>()
        / sys.cpus().len().max(1) as f64;
    let cpu = (cpu_pct / 100.0).clamp(0.0, 1.0);
    let cpu_clock_mhz = {
        let vals: Vec<f64> = sys
            .cpus()
            .iter()
            .map(|c| c.frequency() as f64)
            .filter(|v| v.is_finite() && *v > 0.0)
            .collect();
        if vals.is_empty() {
            None
        } else {
            Some(vals.iter().sum::<f64>() / vals.len() as f64)
        }
    };

    #[cfg(target_os = "linux")]
    let logical_cores = sys.cpus().len().max(1) as f64;

    // Memory (native only)
    #[cfg(target_os = "windows")]
    let native = crate::platform::windows::raw_system_memory();
    #[cfg(target_os = "linux")]
    let native = crate::platform::linux::raw_system_memory();
    #[cfg(target_os = "macos")]
    let native = crate::platform::macos::raw_system_memory();
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    let native: Option<(u64, u64)> = None;

    let (mem_total, mem_avail) = native
        .filter(|(t, _)| *t > 0)
        .ok_or_else(|| anyhow::anyhow!("RAM: native read unavailable (raw_system_memory)"))?;

    let mem_used = mem_total.saturating_sub(mem_avail);
    let mem = (mem_avail as f64 / mem_total.max(1) as f64).clamp(0.0, 1.0);

    let swap_total = sys.total_swap();
    let swap_used = sys.used_swap();

    // Platform-specific compression/PSI/zram
    #[cfg(target_os = "linux")]
    let (compression, psi_cpu, psi_mem, zram_mb) = crate::platform::linux::compression_and_psi()?;

    #[cfg(target_os = "windows")]
    let (compression, psi_cpu, psi_mem, zram_mb): (
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<u64>,
    ) = (None, None, None, None);

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    let (compression, psi_cpu, psi_mem, zram_mb): (
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<u64>,
    ) = (None, None, None, None);

    // Real-time samplers (Windows)
    #[cfg(target_os = "windows")]
    let (
        io_read_mb_s,
        io_write_mb_s,
        gpu_pct,
        win_compression,
        mut power_watts,
        page_faults_per_sec,
    ) = crate::platform::windows::sample_realtime_metrics();

    #[cfg(target_os = "windows")]
    let compression = win_compression.or(compression);

    #[cfg(not(target_os = "windows"))]
    let (io_read_mb_s, io_write_mb_s, page_faults_per_sec): (
        Option<f64>,
        Option<f64>,
        Option<f64>,
    ) = (None, None, None);

    #[cfg(target_os = "linux")]
    let power_watts: Option<f64> = crate::platform::linux::sample_power_watts();
    #[cfg(target_os = "macos")]
    let power_watts: Option<f64> = crate::platform::macos::sample_power_watts();
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    let power_watts: Option<f64> = None;

    let host_power_watts = power_watts;
    let host_power_watts_source: Option<String> = {
        #[cfg(target_os = "linux")]
        {
            host_power_watts.map(|_| "rapl".to_string())
        }
        #[cfg(target_os = "windows")]
        {
            host_power_watts.map(|_| "windows_meter".to_string())
        }
        #[cfg(target_os = "macos")]
        {
            host_power_watts.map(|_| "rapl".to_string())
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            None
        }
    };
    let (wall_power_watts, wall_power_watts_source) = crate::external_power::merge_wall_power()
        .map(|(w, tag)| (Some(w), Some(tag)))
        .unwrap_or((None, None));
    let power_watts = host_power_watts.or(wall_power_watts);
    let power_watts_source = host_power_watts_source
        .clone()
        .or_else(|| wall_power_watts_source.clone());

    #[cfg(target_os = "windows")]
    let (on_battery, battery_percent) = crate::platform::windows::battery_status()
        .map(|(on_dc, pct)| (Some(on_dc), Some(pct as f64)))
        .unwrap_or((None, None));
    #[cfg(not(target_os = "windows"))]
    let (on_battery, battery_percent): (Option<bool>, Option<f64>) = (None, None);
    #[cfg(target_os = "linux")]
    let gpu_pct: Option<f64> = None;
    #[cfg(target_os = "macos")]
    let gpu_pct: Option<f64> = None;
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    let gpu_pct: Option<f64> = None;

    #[cfg(target_os = "windows")]
    let mut gpu_agg = aggregate_gpu_devices(crate::platform::windows::gpu_devices(gpu_pct));
    #[cfg(target_os = "linux")]
    let mut gpu_agg = aggregate_gpu_devices(crate::platform::linux::gpu_devices());
    #[cfg(target_os = "macos")]
    let mut gpu_agg = aggregate_gpu_devices(crate::platform::macos::gpu_devices());
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    let mut gpu_agg = AggregatedGpuMetrics::default();

    #[cfg(target_os = "windows")]
    let (ram_clock_mhz, _, _) = crate::platform::windows::sample_hardware_clocks();
    #[cfg(target_os = "linux")]
    let (ram_clock_mhz, _, _) = crate::platform::linux::sample_hardware_clocks();
    #[cfg(target_os = "macos")]
    let (ram_clock_mhz, _, _) = crate::platform::macos::sample_hardware_clocks();
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    let (ram_clock_mhz, _, _): (Option<f64>, Option<f64>, Option<f64>) = (None, None, None);

    #[cfg(target_os = "linux")]
    let advanced = crate::platform::linux::sample_advanced_metrics(logical_cores);
    #[cfg(not(target_os = "linux"))]
    let advanced = (
        None::<f64>,
        None::<f64>,
        None::<f64>,
        None::<u64>,
        None::<f64>,
        None::<f64>,
        None::<u64>,
        None::<u64>,
    );

    #[cfg(target_os = "linux")]
    let (
        cpu_max_clock_mhz,
        cpu_temp_c,
        load_avg_1m_norm,
        runnable_tasks,
        _gpu_temp_c,
        _gpu_power_watts,
        _gpu_mem_used_mb,
        _gpu_mem_total_mb,
    ) = (
        advanced.cpu_max_clock_mhz,
        advanced.cpu_temp_c,
        advanced.load_avg_1m_norm,
        advanced.runnable_tasks,
        advanced.gpu_temp_c,
        advanced.gpu_power_watts,
        advanced.gpu_mem_used_mb,
        advanced.gpu_mem_total_mb,
    );
    #[cfg(not(target_os = "linux"))]
    let (
        cpu_max_clock_mhz,
        cpu_temp_c,
        load_avg_1m_norm,
        runnable_tasks,
        _gpu_temp_c,
        _gpu_power_watts,
        _gpu_mem_used_mb,
        _gpu_mem_total_mb,
    ) = advanced;

    if gpu_agg.gpu_power_watts.is_none() {
        gpu_agg.gpu_power_watts = estimate_reconciled_gpu_power_watts(
            power_watts,
            gpu_agg.gpu_pct.or(gpu_pct),
            cpu_pct,
            io_read_mb_s,
            io_write_mb_s,
            mem_used,
            mem_total,
            on_battery,
        );
        if gpu_agg.gpu_power_watts.is_some() {
            gpu_agg.gpu_power_source = Some("reconciled_estimate".to_string());
            gpu_agg.gpu_power_confidence = Some("reconciled_estimated".to_string());
            if let Some(first) = gpu_agg.gpu_devices.get_mut(0) {
                first.power_watts = gpu_agg.gpu_power_watts;
                first.source = gpu_agg.gpu_power_source.clone();
                first.confidence = gpu_agg.gpu_power_confidence.clone();
            }
        }
    }

    if gpu_agg.gpu_pct.is_none() {
        gpu_agg.gpu_pct = gpu_pct;
    }

    let gpu_pct = gpu_agg.gpu_pct;
    let gpu_core_clock_mhz = gpu_agg.gpu_core_clock_mhz;
    let gpu_mem_clock_mhz = gpu_agg.gpu_mem_clock_mhz;
    let gpu_temp_c = gpu_agg.gpu_temp_c;
    let gpu_power_watts = gpu_agg.gpu_power_watts;
    let gpu_power_source = gpu_agg.gpu_power_source.clone();
    let gpu_power_confidence = gpu_agg.gpu_power_confidence.clone();
    let gpu_mem_used_mb = gpu_agg.gpu_mem_used_mb;
    let gpu_mem_total_mb = gpu_agg.gpu_mem_total_mb;

    let cpu_freq_ratio = cpu_clock_mhz.zip(cpu_max_clock_mhz).and_then(|(cur, max)| {
        if max > 0.0 && cur.is_finite() && max.is_finite() {
            Some((cur / max).clamp(0.0, 1.5))
        } else {
            None
        }
    });

    // Normalisation for B_io(t): 1500 MB/s reference cap.
    let io_bandwidth = io_read_mb_s
        .zip(io_write_mb_s)
        .map(|(r, w)| ((r + w) / 1500.0).clamp(0.0, 1.0));

    let gpu = gpu_pct.map(|p| (p / 100.0).clamp(0.0, 1.0));

    // Sigma(t): PSI-driven when available, robust proxy fallback otherwise.
    let mem_pressure = (mem_used as f64 / mem_total.max(1) as f64).clamp(0.0, 1.0);
    let swap_pressure = if swap_total > 0 {
        (swap_used as f64 / swap_total as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let io_pressure = io_bandwidth.unwrap_or(0.0).clamp(0.0, 1.0);
    let gpu_pressure = gpu.unwrap_or(0.0).clamp(0.0, 1.0);
    let battery_penalty = if on_battery == Some(true) { 0.08 } else { 0.0 };

    let sigma = if psi_cpu.is_some() || psi_mem.is_some() {
        (0.35 * psi_cpu.unwrap_or(0.0)
            + 0.35 * psi_mem.unwrap_or(0.0)
            + 0.2 * mem_pressure
            + 0.1 * cpu
            + battery_penalty)
            .clamp(0.0, 1.0)
    } else {
        (0.45 * cpu
            + 0.35 * mem_pressure
            + 0.1 * io_pressure
            + 0.05 * swap_pressure
            + 0.05 * gpu_pressure
            + battery_penalty)
            .clamp(0.0, 1.0)
    };

    let platform_name = {
        #[cfg(target_os = "linux")]
        {
            "Linux".to_string()
        }
        #[cfg(target_os = "windows")]
        {
            "Windows".to_string()
        }
        #[cfg(target_os = "macos")]
        {
            "macOS".to_string()
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            "Unknown".to_string()
        }
    };

    let mut state = ResourceState {
        cpu,
        mem,
        compression,
        io_bandwidth,
        gpu,
        sigma,
        epsilon: [0.0; 5],
        raw: RawMetrics {
            cpu_pct,
            cpu_clock_mhz,
            cpu_max_clock_mhz,
            cpu_freq_ratio,
            cpu_temp_c,
            mem_used_mb: mem_used / 1024 / 1024,
            mem_total_mb: mem_total / 1024 / 1024,
            ram_clock_mhz,
            swap_used_mb: swap_used / 1024 / 1024,
            swap_total_mb: swap_total / 1024 / 1024,
            zram_used_mb: zram_mb,
            io_read_mb_s,
            io_write_mb_s,
            gpu_pct,
            gpu_core_clock_mhz,
            gpu_mem_clock_mhz,
            gpu_temp_c,
            gpu_power_watts,
            gpu_power_source,
            gpu_power_confidence,
            gpu_mem_used_mb,
            gpu_mem_total_mb,
            gpu_devices: gpu_agg.gpu_devices,
            power_watts,
            power_watts_source,
            host_power_watts,
            host_power_watts_source,
            wall_power_watts,
            wall_power_watts_source,
            psi_cpu,
            psi_mem,
            load_avg_1m_norm,
            runnable_tasks,
            on_battery,
            battery_percent,
            page_faults_per_sec,
            platform: platform_name.clone(),
            webview_host_cpu_sum,
            webview_host_mem_mb,
        },
    };

    let mem_guard_penalty = crate::memory_policy::tick_from_baseline(&state);

    // epsilon contention — facteur mémoire OS atténué si garde-fous adaptatifs actifs
    let memory_optimizer_factor = crate::platform::memory_optimizer_factor().clamp(0.0, 1.0)
        * (1.0 - 0.35 * mem_guard_penalty);
    let mem_eps_scale = 1.0 - 0.35 * memory_optimizer_factor;
    let comp_eps_scale = 1.0 - 0.25 * memory_optimizer_factor;

    state.epsilon = [
        (cpu * 0.15).clamp(0.0, 0.4),
        (mem_pressure * 0.20 * mem_eps_scale).clamp(0.0, 0.4),
        (compression.unwrap_or(0.0) * 0.10 * comp_eps_scale).clamp(0.0, 0.3),
        (io_bandwidth.unwrap_or(0.0) * 0.15).clamp(0.0, 0.4),
        (gpu.unwrap_or(0.0) * 0.10).clamp(0.0, 0.3),
    ];

    Ok(state)
}
