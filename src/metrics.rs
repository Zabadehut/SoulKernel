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
    /// System power draw from host power meter (Watts). None = unavailable.
    pub power_watts: Option<f64>,
    /// Linux PSI only. None = unavailable.
    pub psi_cpu: Option<f64>,
    pub psi_mem: Option<f64>,
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
        Some(p) => p,
        None => return (None, None),
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
        (
            Some(cpu_sum),
            Some(mem_sum / 1024 / 1024),
        )
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
    let (io_read_mb_s, io_write_mb_s, gpu_pct, win_compression, power_watts, page_faults_per_sec) =
        crate::platform::windows::sample_realtime_metrics();

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

    #[cfg(target_os = "windows")]
    let (on_battery, battery_percent) = crate::platform::windows::battery_status()
        .map(|(on_dc, pct)| (Some(on_dc), Some(pct as f64)))
        .unwrap_or((None, None));
    #[cfg(not(target_os = "windows"))]
    let (on_battery, battery_percent): (Option<bool>, Option<f64>) = (None, None);

    #[cfg(target_os = "linux")]
    let gpu_pct = crate::platform::linux::gpu_utilisation();
    #[cfg(target_os = "macos")]
    let gpu_pct = crate::platform::macos::gpu_utilisation();
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    let gpu_pct: Option<f64> = None;

    #[cfg(target_os = "windows")]
    let (ram_clock_mhz, gpu_core_clock_mhz, gpu_mem_clock_mhz) =
        crate::platform::windows::sample_hardware_clocks();
    #[cfg(target_os = "linux")]
    let (ram_clock_mhz, gpu_core_clock_mhz, gpu_mem_clock_mhz) =
        crate::platform::linux::sample_hardware_clocks();
    #[cfg(target_os = "macos")]
    let (ram_clock_mhz, gpu_core_clock_mhz, gpu_mem_clock_mhz) =
        crate::platform::macos::sample_hardware_clocks();
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    let (ram_clock_mhz, gpu_core_clock_mhz, gpu_mem_clock_mhz): (
        Option<f64>,
        Option<f64>,
        Option<f64>,
    ) = (None, None, None);

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
            power_watts,
            psi_cpu,
            psi_mem,
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
