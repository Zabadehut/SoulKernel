//! platform/linux.rs — Linux full orchestration
//!
//! Real kernel writes via /proc, /sys, cgroups v2.
//! Requires root OR CAP_SYS_ADMIN for write operations.

use crate::{formula::WorkloadProfile, metrics::ResourceState, platform::PlatformInfo};
use anyhow::{Context, Result};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

#[derive(Debug, Clone, Default)]
pub struct LinuxAdvancedMetrics {
    pub cpu_max_clock_mhz: Option<f64>,
    pub cpu_temp_c: Option<f64>,
    pub load_avg_1m_norm: Option<f64>,
    pub runnable_tasks: Option<u64>,
    pub gpu_temp_c: Option<f64>,
    pub gpu_power_watts: Option<f64>,
    pub gpu_mem_used_mb: Option<u64>,
    pub gpu_mem_total_mb: Option<u64>,
}

// ─── Platform info ────────────────────────────────────────────────────────────

pub fn platform_info() -> PlatformInfo {
    let kernel = std::fs::read_to_string("/proc/version")
        .unwrap_or_default()
        .split_whitespace()
        .nth(2)
        .unwrap_or("unknown")
        .to_string();

    let has_cgroups_v2 = Path::new("/sys/fs/cgroup/cgroup.controllers").exists();
    let has_zram = Path::new("/sys/block/zram0").exists();
    let has_gpu_sysfs = Path::new("/sys/class/drm").exists();
    let is_root = libc_getuid() == 0;

    PlatformInfo {
        os: "Linux".into(),
        kernel,
        features: build_feature_list(has_cgroups_v2, has_zram, has_gpu_sysfs, is_root),
        has_cgroups_v2,
        has_zram,
        has_gpu_sysfs,
        is_root,
    }
}

fn build_feature_list(cgv2: bool, zram: bool, gpu: bool, root: bool) -> Vec<String> {
    let mut f = vec![];
    if cgv2 {
        f.push("cgroups-v2".into());
    }
    if zram {
        f.push("zRAM".into());
    }
    if gpu {
        f.push("GPU-sysfs".into());
    }
    if root {
        f.push("root-access".into());
    } else {
        f.push("unprivileged".into());
    }
    f.push("PSI".into());
    f.push("io_uring".into());
    f
}

// ─── Metrics: RAM native (aligné avec Windows) ───────────────────────────────

/// Retourne (total_phys_bytes, available_bytes) via /proc/meminfo.
/// Utilisé en priorité dans metrics pour cohérence multi-OS.
pub fn raw_system_memory() -> Option<(u64, u64)> {
    let content = std::fs::read_to_string("/proc/meminfo").ok()?;
    let mut total_kb: Option<u64> = None;
    let mut avail_kb: Option<u64> = None;
    for line in content.lines() {
        if let Some(v) = line.strip_prefix("MemTotal:") {
            total_kb = v.split_whitespace().next().and_then(|s| s.parse().ok());
        } else if let Some(v) = line.strip_prefix("MemAvailable:") {
            avail_kb = v.split_whitespace().next().and_then(|s| s.parse().ok());
        }
        if total_kb.is_some() && avail_kb.is_some() {
            break;
        }
    }
    let total = total_kb?.checked_mul(1024)?;
    let avail = avail_kb?.checked_mul(1024)?;
    if total == 0 {
        return None;
    }
    Some((total, avail))
}

// ─── Metrics: compression + PSI (aucune simulation : Some uniquement si lecture OK) ──

/// Returns (compression, psi_cpu, psi_mem, zram_used_mb). Option = pas de donnée, pas de 0 fictif.
pub fn compression_and_psi() -> Result<(Option<f64>, Option<f64>, Option<f64>, Option<u64>)> {
    let compression = zram_compression_ratio();
    let psi_cpu = read_psi_avg10("/proc/pressure/cpu").map(|x| x / 100.0);
    let psi_mem = read_psi_avg10("/proc/pressure/memory").map(|x| x / 100.0);
    let zram_mb = zram_used_bytes().map(|b| b / 1024 / 1024);
    Ok((compression, psi_cpu, psi_mem, zram_mb))
}

fn read_psi_avg10(path: &str) -> Option<f64> {
    // Format: "some avg10=X.XX avg60=... avg300=... total=..."
    let content = std::fs::read_to_string(path).ok()?;
    for line in content.lines() {
        if line.starts_with("some") {
            for part in line.split_whitespace() {
                if let Some(v) = part.strip_prefix("avg10=") {
                    return v.parse().ok();
                }
            }
        }
    }
    None
}

fn zram_compression_ratio() -> Option<f64> {
    // Read /sys/block/zram0/mm_stat: orig_data_size compr_data_size ...
    let stat = std::fs::read_to_string("/sys/block/zram0/mm_stat").ok()?;
    let parts: Vec<u64> = stat
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    if parts.len() >= 2 && parts[0] > 0 {
        Some((parts[1] as f64 / parts[0] as f64).clamp(0.0, 1.0))
    } else {
        None
    }
}

fn zram_used_bytes() -> Option<u64> {
    let stat = std::fs::read_to_string("/sys/block/zram0/mm_stat").ok()?;
    stat.split_whitespace().next()?.parse().ok()
}

pub fn gpu_utilisation() -> Option<f64> {
    for card in drm_card_device_paths() {
        let path = card.join("gpu_busy_percent");
        if let Ok(s) = std::fs::read_to_string(path) {
            if let Ok(v) = s.trim().parse::<f64>() {
                if (0.0..=100.0).contains(&v) {
                    return Some(v);
                }
            }
        }
    }
    // NVIDIA fallback: parse nvidia-smi output
    // (would require subprocess call — omitted for brevity)
    None
}

pub fn sample_hardware_clocks() -> (Option<f64>, Option<f64>, Option<f64>) {
    let ram_clock_mhz = None;
    let mut core_paths = Vec::new();
    let mut mem_paths = Vec::new();
    for card in drm_card_paths() {
        core_paths.push(card.join("device/pp_dpm_sclk"));
        core_paths.push(card.join("gt_cur_freq_mhz"));
        core_paths.push(card.join("gt/gt0/freq0/cur_freq"));
        mem_paths.push(card.join("device/pp_dpm_mclk"));
    }
    let gpu_core_clock_mhz = read_gpu_clock_mhz(&core_paths);
    let gpu_mem_clock_mhz = read_gpu_clock_mhz(&mem_paths);
    (ram_clock_mhz, gpu_core_clock_mhz, gpu_mem_clock_mhz)
}

pub fn sample_advanced_metrics(logical_cores: f64) -> LinuxAdvancedMetrics {
    LinuxAdvancedMetrics {
        cpu_max_clock_mhz: read_cpu_max_clock_mhz(),
        cpu_temp_c: read_cpu_temperature_c(),
        load_avg_1m_norm: read_load_avg_norm(logical_cores),
        runnable_tasks: read_runnable_tasks(),
        gpu_temp_c: read_gpu_temperature_c(),
        gpu_power_watts: read_gpu_power_watts(),
        gpu_mem_used_mb: read_gpu_mem_info_mb("mem_info_vram_used"),
        gpu_mem_total_mb: read_gpu_mem_info_mb("mem_info_vram_total"),
    }
}

fn read_gpu_clock_mhz(paths: &[std::path::PathBuf]) -> Option<f64> {
    for path in paths {
        if let Some(v) = read_active_mhz_from_file(path) {
            return Some(v);
        }
        if let Some(v) = read_scalar_mhz_from_file(path) {
            return Some(v);
        }
    }
    None
}

fn read_active_mhz_from_file(path: &Path) -> Option<f64> {
    let content = std::fs::read_to_string(path).ok()?;
    for line in content.lines() {
        if !line.contains('*') {
            continue;
        }
        let mhz = line
            .split_whitespace()
            .find_map(parse_mhz_token)
            .filter(|v| *v > 0.0);
        if mhz.is_some() {
            return mhz;
        }
    }
    None
}

fn read_scalar_mhz_from_file(path: &Path) -> Option<f64> {
    let raw = std::fs::read_to_string(path).ok()?;
    let val = raw.trim().parse::<f64>().ok()?;
    let path_str = path.to_string_lossy();
    if path_str.ends_with("_freq") || path_str.contains("cur_freq") {
        if val > 10_000.0 {
            Some(val / 1000.0)
        } else {
            Some(val)
        }
    } else if val > 0.0 {
        Some(val)
    } else {
        None
    }
}

fn parse_mhz_token(token: &str) -> Option<f64> {
    let cleaned = token
        .trim()
        .trim_end_matches('*')
        .trim_end_matches("Mhz")
        .trim_end_matches("MHz")
        .trim_end_matches("mhz");
    cleaned.parse::<f64>().ok()
}

fn drm_card_paths() -> Vec<std::path::PathBuf> {
    let mut cards = Vec::new();
    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return cards;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("card") || name.contains('-') || !path.is_dir() {
            continue;
        }
        cards.push(path);
    }
    cards.sort();
    cards
}

fn drm_card_device_paths() -> Vec<std::path::PathBuf> {
    drm_card_paths()
        .into_iter()
        .map(|card| card.join("device"))
        .filter(|p| p.exists())
        .collect()
}

fn read_cpu_max_clock_mhz() -> Option<f64> {
    let mut vals = Vec::new();
    let entries = std::fs::read_dir("/sys/devices/system/cpu").ok()?;
    for entry in entries.flatten() {
        let path = entry.path().join("cpufreq/cpuinfo_max_freq");
        let Ok(raw) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(khz) = raw.trim().parse::<f64>() else {
            continue;
        };
        if khz > 0.0 {
            vals.push(khz / 1000.0);
        }
    }
    if vals.is_empty() {
        None
    } else {
        Some(vals.iter().sum::<f64>() / vals.len() as f64)
    }
}

fn read_cpu_temperature_c() -> Option<f64> {
    read_best_hwmon_temp(
        "/sys/class/thermal",
        &[
            "x86_pkg_temp",
            "pkg_temp",
            "k10temp",
            "zenpower",
            "coretemp",
            "acpitz",
        ],
    )
    .or_else(|| read_best_hwmon_temp("/sys/class/hwmon", &["k10temp", "zenpower", "coretemp"]))
}

fn read_gpu_temperature_c() -> Option<f64> {
    for device in drm_card_device_paths() {
        if let Some(v) = read_hwmon_number_scaled(&device.join("hwmon"), "temp1_input", 1000.0) {
            return Some(v);
        }
    }
    None
}

fn read_gpu_power_watts() -> Option<f64> {
    for device in drm_card_device_paths() {
        if let Some(v) =
            read_hwmon_number_scaled(&device.join("hwmon"), "power1_average", 1_000_000.0).or_else(
                || read_hwmon_number_scaled(&device.join("hwmon"), "power1_input", 1_000_000.0),
            )
        {
            return Some(v);
        }
    }
    None
}

fn read_gpu_mem_info_mb(name: &str) -> Option<u64> {
    for device in drm_card_device_paths() {
        let path = device.join(name);
        let Ok(raw) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(bytes) = raw.trim().parse::<u64>() else {
            continue;
        };
        if bytes > 0 {
            return Some(bytes / 1024 / 1024);
        }
    }
    None
}

fn read_load_avg_norm(logical_cores: f64) -> Option<f64> {
    let load1 = std::fs::read_to_string("/proc/loadavg")
        .ok()?
        .split_whitespace()
        .next()?
        .parse::<f64>()
        .ok()?;
    if logical_cores <= 0.0 {
        None
    } else {
        Some((load1 / logical_cores).clamp(0.0, 4.0))
    }
}

fn read_runnable_tasks() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/loadavg").ok()?;
    let token = s.split_whitespace().nth(3)?;
    let runnable = token.split('/').next()?;
    runnable.parse::<u64>().ok()
}

fn read_best_hwmon_temp(root: &str, preferred_types: &[&str]) -> Option<f64> {
    let entries = std::fs::read_dir(root).ok()?;
    let mut fallback = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let sensor_name = std::fs::read_to_string(path.join("type"))
            .or_else(|_| std::fs::read_to_string(path.join("name")))
            .ok()
            .map(|v| v.trim().to_ascii_lowercase());
        let temp = read_temp_input_from_dir(&path);
        let Some(temp) = temp else {
            continue;
        };
        match sensor_name {
            Some(ref name)
                if preferred_types
                    .iter()
                    .any(|p| name.contains(&p.to_ascii_lowercase())) =>
            {
                return Some(temp);
            }
            _ if fallback.is_none() => fallback = Some(temp),
            _ => {}
        }
    }
    fallback
}

fn read_temp_input_from_dir(dir: &Path) -> Option<f64> {
    for candidate in ["temp1_input", "temp2_input", "temp3_input"] {
        let path = dir.join(candidate);
        let Ok(raw) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(v) = raw.trim().parse::<f64>() else {
            continue;
        };
        if v > 1000.0 {
            return Some(v / 1000.0);
        }
        if v > 0.0 {
            return Some(v);
        }
    }
    None
}

fn read_hwmon_number_scaled(hwmon_root: &Path, file_name: &str, scale: f64) -> Option<f64> {
    let entries = std::fs::read_dir(hwmon_root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path().join(file_name);
        let Ok(raw) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(v) = raw.trim().parse::<f64>() else {
            continue;
        };
        if v > 0.0 {
            return Some(v / scale);
        }
    }
    None
}

// ─── Dome activation ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct RaplSampleState {
    ts: Instant,
    total_energy_uj: f64,
}

static RAPL_SAMPLE_STATE: OnceLock<Mutex<Option<RaplSampleState>>> = OnceLock::new();

/// Real power in Watts when available.
/// Priority:
/// 1) Intel/AMD RAPL counters (package-level real energy).
/// 2) Battery discharge power (laptops).
pub fn sample_power_watts() -> Option<f64> {
    sample_rapl_power_watts().or_else(sample_battery_power_watts)
}

fn sample_rapl_power_watts() -> Option<f64> {
    let total_energy_uj = rapl_total_energy_uj()?;
    let now = Instant::now();
    let lock = RAPL_SAMPLE_STATE.get_or_init(|| Mutex::new(None));
    let mut guard = lock.lock().ok()?;

    match guard.as_ref() {
        None => {
            *guard = Some(RaplSampleState {
                ts: now,
                total_energy_uj,
            });
            None
        }
        Some(prev) => {
            let dt_s = now.duration_since(prev.ts).as_secs_f64();
            if !(0.05..=30.0).contains(&dt_s) {
                *guard = Some(RaplSampleState {
                    ts: now,
                    total_energy_uj,
                });
                return None;
            }
            let delta_uj = total_energy_uj - prev.total_energy_uj;
            *guard = Some(RaplSampleState {
                ts: now,
                total_energy_uj,
            });
            if delta_uj <= 0.0 {
                return None;
            }
            let watts = (delta_uj / 1_000_000.0) / dt_s;
            if watts.is_finite() && (0.0..=2000.0).contains(&watts) {
                Some(watts)
            } else {
                None
            }
        }
    }
}

fn rapl_total_energy_uj() -> Option<f64> {
    let root = Path::new("/sys/class/powercap");
    if !root.exists() {
        return None;
    }

    let mut sum = 0.0;
    let mut found = false;

    fn visit(dir: &Path, sum: &mut f64, found: &mut bool, depth: usize) {
        if depth > 3 {
            return;
        }
        let entries = match std::fs::read_dir(dir) {
            Ok(v) => v,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                visit(&p, sum, found, depth + 1);
                continue;
            }
            if p.file_name().and_then(|n| n.to_str()) != Some("energy_uj") {
                continue;
            }
            if let Ok(raw) = std::fs::read_to_string(&p) {
                if let Ok(v) = raw.trim().parse::<f64>() {
                    if v >= 0.0 {
                        *sum += v;
                        *found = true;
                    }
                }
            }
        }
    }

    visit(root, &mut sum, &mut found, 0);
    if found {
        Some(sum)
    } else {
        None
    }
}

fn sample_battery_power_watts() -> Option<f64> {
    let root = Path::new("/sys/class/power_supply");
    let entries = std::fs::read_dir(root).ok()?;
    let mut total_w = 0.0;
    let mut found = false;

    for entry in entries.flatten() {
        let p = entry.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if !name.starts_with("BAT") {
            continue;
        }

        let power_now = p.join("power_now");
        if let Ok(s) = std::fs::read_to_string(&power_now) {
            if let Ok(uw) = s.trim().parse::<f64>() {
                if uw > 0.0 {
                    total_w += uw / 1_000_000.0;
                    found = true;
                    continue;
                }
            }
        }

        let current_now = p.join("current_now");
        let voltage_now = p.join("voltage_now");
        if let (Ok(c), Ok(v)) = (
            std::fs::read_to_string(&current_now),
            std::fs::read_to_string(&voltage_now),
        ) {
            if let (Ok(ua), Ok(uv)) = (c.trim().parse::<f64>(), v.trim().parse::<f64>()) {
                let w = (ua * uv) / 1_000_000_000_000.0;
                if w > 0.0 {
                    total_w += w;
                    found = true;
                }
            }
        }
    }

    if found {
        Some(total_w)
    } else {
        None
    }
}
pub async fn apply_dome(
    profile: &WorkloadProfile,
    eta: f64,
    baseline: &ResourceState,
    _policy: crate::platform::PolicyMode,
    _target_pid: Option<u32>,
) -> Vec<(String, bool)> {
    let mut actions = Vec::new();
    let adaptive = crate::platform::derive_adaptive_action_profile(profile, eta, baseline, _policy);

    let mem_plan = crate::memory_policy::plan_for_dome_activation(baseline, profile);
    for note in &mem_plan.notes {
        actions.push((note.clone(), true));
    }

    // Determine profile intensities via gradient
    // r_new[i] = r[i] + η · α[i] · (1 − Σ)
    let boost =
        |i: usize| -> f64 { (profile.alpha[i] * eta * (1.0 - baseline.sigma)).clamp(0.0, 0.3) };
    let cpu_hot =
        adaptive.thermal_guard && baseline.raw.cpu_temp_c.map(|t| t >= 82.0).unwrap_or(false);
    let gpu_hot =
        adaptive.thermal_guard && baseline.raw.gpu_temp_c.map(|t| t >= 78.0).unwrap_or(false);
    let io_busy = baseline.io_bandwidth.unwrap_or(0.0) >= 0.42;

    // ── CPU governor ─────────────────────────────────────────────────────────
    let cpu_governor = match adaptive.cpu_bias {
        crate::platform::CpuBias::Boost => "performance",
        crate::platform::CpuBias::Eco => "powersave",
        crate::platform::CpuBias::Balanced => "schedutil",
    };
    actions.push(write_cpu_governor(cpu_governor));
    actions.push(write_cpu_energy_preference(match adaptive.cpu_bias {
        crate::platform::CpuBias::Boost => "performance",
        crate::platform::CpuBias::Eco => "balance_power",
        crate::platform::CpuBias::Balanced => "balance_performance",
    }));
    actions.push(write_cpu_boost(matches!(
        adaptive.cpu_bias,
        crate::platform::CpuBias::Boost
    )));

    // ── Swappiness ───────────────────────────────────────────────────────────
    let target_swappiness: u64 =
        if matches!(adaptive.memory_bias, crate::platform::MemoryBias::Boost) && baseline.mem < 0.4
        {
            80
        } else if adaptive.sigma_guard || cpu_hot || gpu_hot {
            20
        } else {
            30
        };
    actions.push(write_sysctl(
        "vm.swappiness",
        &target_swappiness.to_string(),
    ));

    // ── zRAM resize ──────────────────────────────────────────────────────────
    if Path::new("/sys/block/zram0").exists()
        && profile.alpha[2] > 0.1
        && mem_plan.apply_zram_resize
    {
        let boost_factor = 1.0 + boost(2) * 2.0; // up to 60% more zRAM
        let (msg, ok) = resize_zram(boost_factor);
        if ok {
            crate::memory_policy::record_linux_aggressive_memory();
        }
        actions.push((msg, ok));
    }

    // ── I/O scheduler ────────────────────────────────────────────────────────
    let scheduler = if matches!(adaptive.io_bias, crate::platform::IoBias::Boost) && !io_busy {
        "mq-deadline"
    } else if adaptive.sigma_guard {
        "none"
    } else {
        "bfq"
    };
    actions.push(write_io_scheduler(scheduler));

    // ── read_ahead_kb ─────────────────────────────────────────────────────────
    if !matches!(adaptive.io_bias, crate::platform::IoBias::Eco) || profile.alpha[3] > 0.3 {
        let read_ahead = if io_busy { 1024 } else { 2048 };
        actions.push(write_read_ahead(read_ahead));
    }

    actions.push(write_block_nomerges(if io_busy || adaptive.sigma_guard {
        2
    } else {
        0
    }));

    // ── CPU pinning via cgroups v2 ────────────────────────────────────────────
    if Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
        actions.push(pin_process_to_cpuset());
    }

    // ── GPU runtime policy (best-effort, reversible) ────────────────────────
    actions.extend(write_gpu_runtime_policy(match adaptive.gpu_bias {
        crate::platform::GpuBias::Eco => GpuRuntimePolicy::Auto,
        crate::platform::GpuBias::Boost => GpuRuntimePolicy::High,
        crate::platform::GpuBias::Balanced => GpuRuntimePolicy::Balanced,
    }));

    // ── Page cache drop (free stale cache for I/O-heavy workloads) ────────────
    if profile.alpha[3] > 0.5 && mem_plan.apply_drop_caches {
        let (msg, ok) = drop_caches_level(1);
        if ok {
            crate::memory_policy::record_linux_aggressive_memory();
        }
        actions.push((msg, ok));
    }

    actions
}

// ─── Rollback ─────────────────────────────────────────────────────────────────

pub async fn rollback(
    _snapshot: Option<ResourceState>,
    _target_pid: Option<u32>,
) -> Vec<(String, bool)> {
    let mut actions = vec![
        write_cpu_governor("schedutil"),
        write_cpu_energy_preference("balance_performance"),
        write_cpu_boost(true),
        write_sysctl("vm.swappiness", "60"),
        write_io_scheduler("bfq"),
        write_read_ahead(128),
        write_block_nomerges(0),
        ("cgroup cpuset released".into(), remove_soulkernel_cgroup()),
    ];
    actions.extend(write_gpu_runtime_policy(GpuRuntimePolicy::Auto));
    actions
}

// ─── Kernel write primitives ──────────────────────────────────────────────────

fn write_sysctl(key: &str, value: &str) -> (String, bool) {
    let path = format!("/proc/sys/{}", key.replace('.', "/"));
    let ok = std::fs::write(&path, value).is_ok();
    (format!("sysctl {} = {}", key, value), ok)
}

fn write_cpu_governor(governor: &str) -> (String, bool) {
    let mut any_ok = false;
    let entries = std::fs::read_dir("/sys/devices/system/cpu")
        .into_iter()
        .flatten()
        .flatten();

    for entry in entries {
        let path = entry.path().join("cpufreq/scaling_governor");
        if path.exists() && std::fs::write(&path, governor).is_ok() {
            any_ok = true;
        }
    }
    (format!("CPU governor → {}", governor), any_ok)
}

fn write_cpu_energy_preference(pref: &str) -> (String, bool) {
    let mut any_ok = false;
    let entries = std::fs::read_dir("/sys/devices/system/cpu")
        .into_iter()
        .flatten()
        .flatten();
    for entry in entries {
        let path = entry.path().join("cpufreq/energy_performance_preference");
        if path.exists() && std::fs::write(&path, pref).is_ok() {
            any_ok = true;
        }
    }
    (format!("CPU energy preference → {}", pref), any_ok)
}

fn write_cpu_boost(enabled: bool) -> (String, bool) {
    let value = if enabled { "1" } else { "0" };
    let candidates = [
        "/sys/devices/system/cpu/cpufreq/boost",
        "/sys/devices/system/cpu/intel_pstate/no_turbo",
    ];
    let mut any_ok = false;
    for path in candidates {
        let write_val = if path.ends_with("no_turbo") {
            if enabled {
                "0"
            } else {
                "1"
            }
        } else {
            value
        };
        if Path::new(path).exists() && std::fs::write(path, write_val).is_ok() {
            any_ok = true;
        }
    }
    (
        format!("CPU boost → {}", if enabled { "on" } else { "off" }),
        any_ok,
    )
}

/// Detect the primary block device (first non-virtual disk with a scheduler).
fn detect_primary_block_device() -> Option<String> {
    let entries = std::fs::read_dir("/sys/block").ok()?;
    let mut candidates: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("loop")
            || name.starts_with("zram")
            || name.starts_with("dm-")
            || name.starts_with("sr")
            || name.starts_with("ram")
        {
            continue;
        }
        let sched_path = format!("/sys/block/{}/queue/scheduler", name);
        if Path::new(&sched_path).exists() {
            candidates.push(name);
        }
    }
    // Prefer NVMe, then sd*, then vd*, then others
    candidates.sort_by(|a, b| {
        fn priority(name: &str) -> u8 {
            if name.starts_with("nvme") {
                0
            } else if name.starts_with("sd") {
                1
            } else if name.starts_with("vd") {
                2
            } else {
                3
            }
        }
        priority(a).cmp(&priority(b)).then(a.cmp(b))
    });
    candidates.into_iter().next()
}

fn write_io_scheduler(sched: &str) -> (String, bool) {
    let dev = match detect_primary_block_device() {
        Some(d) => d,
        None => {
            return (
                format!("I/O scheduler → {} (no block device found)", sched),
                false,
            )
        }
    };
    let path = format!("/sys/block/{}/queue/scheduler", dev);
    let ok = std::fs::write(&path, sched).is_ok();
    (format!("I/O scheduler ({}) → {}", dev, sched), ok)
}

fn write_read_ahead(kb: u64) -> (String, bool) {
    let dev = match detect_primary_block_device() {
        Some(d) => d,
        None => {
            return (
                format!("read_ahead_kb → {} (no block device found)", kb),
                false,
            )
        }
    };
    let path = format!("/sys/block/{}/queue/read_ahead_kb", dev);
    let ok = std::fs::write(&path, kb.to_string()).is_ok();
    (format!("read_ahead_kb ({}) → {}", dev, kb), ok)
}

fn write_block_nomerges(value: u8) -> (String, bool) {
    let dev = match detect_primary_block_device() {
        Some(d) => d,
        None => {
            return (
                format!("nomerges → {} (no block device found)", value),
                false,
            )
        }
    };
    let path = format!("/sys/block/{}/queue/nomerges", dev);
    let ok = std::fs::write(&path, value.to_string()).is_ok();
    (format!("nomerges ({}) → {}", dev, value), ok)
}

fn resize_zram(factor: f64) -> (String, bool) {
    // Read current disksize, multiply by factor
    let current: u64 = std::fs::read_to_string("/sys/block/zram0/disksize")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(2 * 1024 * 1024 * 1024); // default 2GB

    let new_size = (current as f64 * factor) as u64;

    // zRAM requires: reset → disksize → mkswap → swapon
    // We just update disksize if not in use; full resize needs orchestration
    let ok = std::fs::write("/sys/block/zram0/disksize", new_size.to_string()).is_ok();
    (format!("zRAM resize → {} MB", new_size / 1024 / 1024), ok)
}

/// Returns the online CPU range string (e.g. "0-7").
fn available_cpu_range() -> String {
    if let Ok(range) = std::fs::read_to_string("/sys/devices/system/cpu/online") {
        let trimmed = range.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    format!("0-{}", count.saturating_sub(1))
}

fn pin_process_to_cpuset() -> (String, bool) {
    let cg_path = "/sys/fs/cgroup/soulkernel";
    let cpu_range = available_cpu_range();
    let ok = (|| -> Result<()> {
        std::fs::create_dir_all(cg_path)?;
        std::fs::write(format!("{}/cpuset.cpus", cg_path), &cpu_range).context("cpuset.cpus")?;
        let pid = std::process::id();
        std::fs::write(format!("{}/cgroup.procs", cg_path), pid.to_string())
            .context("cgroup.procs")?;
        Ok(())
    })()
    .is_ok();
    (format!("cgroup cpuset → CPU {}", cpu_range), ok)
}

fn drop_caches_level(level: u8) -> (String, bool) {
    let ok = std::fs::write("/proc/sys/vm/drop_caches", level.to_string()).is_ok();
    (format!("drop_caches → level {}", level), ok)
}

fn remove_soulkernel_cgroup() -> bool {
    std::fs::remove_dir("/sys/fs/cgroup/soulkernel").is_ok()
}

#[derive(Clone, Copy)]
enum GpuRuntimePolicy {
    Auto,
    Balanced,
    High,
}

fn write_gpu_runtime_policy(policy: GpuRuntimePolicy) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for card in drm_card_paths() {
        let label = card
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("gpu")
            .to_string();
        let device = card.join("device");

        let power_control = match policy {
            GpuRuntimePolicy::Auto | GpuRuntimePolicy::Balanced => "auto",
            GpuRuntimePolicy::High => "on",
        };
        let pwr_path = device.join("power/control");
        if pwr_path.exists() {
            let ok = std::fs::write(&pwr_path, power_control).is_ok();
            out.push((format!("GPU runtime ({label}) → {}", power_control), ok));
        }

        let amd_level = match policy {
            GpuRuntimePolicy::Auto => "auto",
            GpuRuntimePolicy::Balanced => "low",
            GpuRuntimePolicy::High => "high",
        };
        let amd_path = device.join("power_dpm_force_performance_level");
        if amd_path.exists() {
            let ok = std::fs::write(&amd_path, amd_level).is_ok();
            out.push((format!("GPU DPM ({label}) → {}", amd_level), ok));
        }
    }
    if out.is_empty() {
        out.push(("GPU runtime policy unavailable".into(), false));
    }
    out
}

// ─── FFI shim (Unix only) ────────────────────────────────────────────────────

#[cfg(unix)]
extern "C" {
    fn getuid() -> u32;
}
#[cfg(unix)]
fn libc_getuid() -> u32 {
    unsafe { getuid() }
}
#[cfg(not(unix))]
fn libc_getuid() -> u32 {
    0
}

pub fn memory_optimizer_factor() -> f64 {
    let has_zram = std::path::Path::new("/sys/block/zram0").exists();
    let root = libc_getuid() == 0;
    match (has_zram, root) {
        (true, true) => 0.95,
        (true, false) => 0.65,
        (false, true) => 0.55,
        (false, false) => 0.30,
    }
}

pub fn policy_status(mode: crate::platform::PolicyMode) -> crate::platform::PolicyStatus {
    crate::platform::PolicyStatus {
        mode: mode.as_name().into(),
        is_admin: libc_getuid() == 0,
        reboot_pending: false,
        memory_compression_enabled: None,
    }
}

pub fn soulram_backend_name() -> String {
    "Linux zRAM".into()
}

pub async fn enable_soulram(percent: u8) -> Vec<(String, bool)> {
    let pct = percent.clamp(5, 60) as u64;
    let mut actions = Vec::new();

    actions.push(ensure_zram_module_loaded());

    let Some((total_b, _)) = raw_system_memory() else {
        actions.push(("Cannot read total RAM to size zRAM".into(), false));
        return actions;
    };

    let target_b = (total_b.saturating_mul(pct) / 100).max(256 * 1024 * 1024);
    actions.push(reset_zram_dev());
    actions.push(write_zram_disksize(target_b));
    actions.push(run_cmd("mkswap", &["/dev/zram0"], "mkswap /dev/zram0"));
    actions.push(run_cmd(
        "swapon",
        &["-p", "100", "/dev/zram0"],
        "swapon /dev/zram0",
    ));
    actions.push((
        format!(
            "SoulRAM active -> zRAM {} MB ({}%)",
            target_b / 1024 / 1024,
            pct
        ),
        true,
    ));

    crate::memory_policy::record_linux_aggressive_memory();

    actions
}

pub async fn disable_soulram() -> Vec<(String, bool)> {
    vec![
        run_cmd("swapoff", &["/dev/zram0"], "swapoff /dev/zram0"),
        reset_zram_dev(),
        ("SoulRAM disabled (Linux zRAM backend)".into(), true),
    ]
}

fn ensure_zram_module_loaded() -> (String, bool) {
    if Path::new("/sys/block/zram0").exists() {
        return ("zRAM device already present".into(), true);
    }

    let modprobe = run_cmd("modprobe", &["zram"], "modprobe zram");
    if modprobe.1 && Path::new("/sys/block/zram0").exists() {
        return ("zRAM module loaded".into(), true);
    }

    // Some kernels expose hot_add for dynamic zram device creation.
    if Path::new("/sys/class/zram-control/hot_add").exists() {
        let wrote = std::fs::write("/sys/class/zram-control/hot_add", "1").is_ok();
        if wrote && Path::new("/sys/block/zram0").exists() {
            return ("zRAM device hot-added".into(), true);
        }
    }

    ("zRAM unavailable (need root/kernel support)".into(), false)
}

fn write_zram_disksize(size_b: u64) -> (String, bool) {
    let ok = std::fs::write("/sys/block/zram0/disksize", size_b.to_string()).is_ok();
    (format!("zRAM disksize -> {} MB", size_b / 1024 / 1024), ok)
}

fn reset_zram_dev() -> (String, bool) {
    let ok = std::fs::write("/sys/block/zram0/reset", "1").is_ok();
    ("zRAM reset".into(), ok)
}

fn run_cmd(bin: &str, args: &[&str], label: &str) -> (String, bool) {
    let ok = std::process::Command::new(bin)
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    (label.into(), ok)
}
