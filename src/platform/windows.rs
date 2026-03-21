//! platform/windows.rs — Windows full orchestration
//!
//! Uses:
//!   - Job Objects → CPU affinity, memory limits
//!   - SetProcessAffinityMask → cpuset equivalent
//!   - SetProcessWorkingSetSize → RAM pressure tuning
//!   - Power plans via powercfg (subprocess)
//!   - Registry → virtual memory tuning
//!   - GlobalMemoryStatusEx pour RAM physique réelle (évite les erreurs sysinfo >16 Go)

use crate::{formula::WorkloadProfile, metrics::ResourceState, platform::PlatformInfo};

#[cfg(target_os = "windows")]
use std::sync::{Mutex, OnceLock};
#[cfg(target_os = "windows")]
fn command_hidden(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new(program);
    // CREATE_NO_WINDOW
    cmd.creation_flags(0x0800_0000);
    cmd
}

#[cfg(target_os = "windows")]
#[derive(Clone)]
struct WindowsPrivCache {
    at: std::time::Instant,
    is_admin: bool,
    memory_compression: Option<bool>,
}

#[cfg(target_os = "windows")]
static WINDOWS_PRIV_CACHE: OnceLock<Mutex<Option<WindowsPrivCache>>> = OnceLock::new();

/// Retourne (total_phys_bytes, available_phys_bytes) via l’API Windows.
/// À privilégier sur sysinfo pour les machines avec >16 Go (bugs connus).
pub fn raw_system_memory() -> Option<(u64, u64)> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut status = MEMORYSTATUSEX::default();
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        unsafe {
            GlobalMemoryStatusEx(&mut status).ok()?;
            Some((status.ullTotalPhys, status.ullAvailPhys))
        }
    }
    #[cfg(not(target_os = "windows"))]
    None
}

/// Returns (on_battery, battery_percent) when available.
/// on_battery=true means system currently runs on battery/DC.
pub fn battery_status() -> Option<(bool, u8)> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

        let mut s = SYSTEM_POWER_STATUS::default();
        unsafe {
            GetSystemPowerStatus(&mut s).ok()?;
        }

        let on_battery = s.ACLineStatus == 0;
        let pct = if s.BatteryLifePercent == u8::MAX {
            0
        } else {
            s.BatteryLifePercent
        };
        Some((on_battery, pct))
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}
pub fn platform_info() -> PlatformInfo {
    PlatformInfo {
        os: "Windows".into(),
        kernel: os_version(),
        features: vec![
            "Job Objects".into(),
            "CPU Affinity".into(),
            "Working Set".into(),
            "Power Plans".into(),
            "DXGI GPU metrics".into(),
        ],
        has_cgroups_v2: false, // Windows equivalent = Job Objects
        has_zram: false,       // Windows equivalent = ReadyBoost / pagefile
        has_gpu_sysfs: true,   // via DXGI/WMI
        is_root: is_elevated(),
    }
}

pub async fn apply_dome(
    profile: &WorkloadProfile,
    eta: f64,
    baseline: &ResourceState,
    policy: crate::platform::PolicyMode,
    target_pid: Option<u32>,
) -> Vec<(String, bool)> {
    let mut actions = Vec::new();
    let is_target_process = target_pid.is_some();

    // Safe mode: avoid privileged per-process mutations; keep only low-risk system hints.
    if profile.alpha[0] > 0.3 {
        actions.push(set_power_plan("8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c"));
    }

    if matches!(policy, crate::platform::PolicyMode::Safe) {
        actions.push((
            "Policy SAFE: skipped affinity/working-set/priority/memory-compression".into(),
            true,
        ));
        return actions;
    }

    let mem_plan = crate::memory_policy::plan_for_dome_activation(baseline, profile);
    for note in &mem_plan.notes {
        actions.push((note.clone(), true));
    }

    // Privileged mode path
    if profile.alpha[0] > 0.4 {
        let mask = if is_target_process { 0xFFFF } else { 0x0F0F };
        actions.push(set_process_affinity(mask, target_pid));
    }

    if profile.alpha[1] > 0.2 {
        if mem_plan.apply_working_set {
            let (min_b, max_b) = if is_target_process {
                let min_b = 512 * 1024 * 1024;
                let max_mb_mb = (2048.0_f64 + eta * 2048.0).min(4096.0);
                let max_b = (max_mb_mb as usize) * 1024 * 1024;
                (min_b, max_b)
            } else {
                (256 * 1024 * 1024, 1024 * 1024 * 1024)
            };
            let (msg, ok) = set_working_set(min_b, max_b, target_pid);
            if ok {
                crate::memory_policy::record_working_set_adjustment();
            }
            actions.push((msg, ok));
        }
    }

    if profile.alpha[3] > 0.4 {
        if mem_plan.apply_disable_compression {
            crate::memory_policy::record_compression_toggle();
            actions.push(disable_memory_compression());
        }
    }

    actions.push(set_io_priority_high(target_pid));
    actions
}

pub async fn rollback(
    _snapshot: Option<ResourceState>,
    target_pid: Option<u32>,
) -> Vec<(String, bool)> {
    vec![
        set_power_plan("381b4222-f694-41f0-9685-ff5bb260df2e"),
        set_process_affinity(0xFFFF, target_pid),
        restore_working_set(target_pid),
        restore_memory_compression(),
        restore_io_priority(target_pid),
    ]
}

#[cfg(target_os = "windows")]
struct WindowsRealtimeCounters {
    query: isize,
    disk_read_counter: Option<isize>,
    disk_write_counter: Option<isize>,
    gpu_counter: Option<isize>,
    compressed_counter: Option<isize>,
    page_faults_counter: Option<isize>,
    power_counter: Option<isize>,
    battery_discharge_counter: Option<isize>,
}

#[cfg(target_os = "windows")]
static WINDOWS_COUNTERS: OnceLock<Option<Mutex<WindowsRealtimeCounters>>> = OnceLock::new();

#[cfg(target_os = "windows")]
impl Drop for WindowsRealtimeCounters {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::System::Performance::PdhCloseQuery(self.query);
        }
    }
}

#[cfg(target_os = "windows")]
impl WindowsRealtimeCounters {
    fn new() -> Option<Self> {
        use windows::core::PCWSTR;
        use windows::Win32::System::Performance::{PdhCollectQueryData, PdhOpenQueryW};

        let mut query = 0isize;
        unsafe {
            if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != 0 {
                return None;
            }
        }

        let mut me = Self {
            query,
            disk_read_counter: None,
            disk_write_counter: None,
            gpu_counter: None,
            compressed_counter: None,
            page_faults_counter: None,
            power_counter: None,
            battery_discharge_counter: None,
        };

        me.disk_read_counter =
            Self::add_counter(me.query, "\\PhysicalDisk(_Total)\\Disk Read Bytes/sec");
        me.disk_write_counter =
            Self::add_counter(me.query, "\\PhysicalDisk(_Total)\\Disk Write Bytes/sec");
        me.gpu_counter = Self::add_counter(me.query, "\\GPU Engine(*)\\Utilization Percentage");
        me.compressed_counter = Self::add_counter(me.query, "\\Memory\\Compressed Page Size");
        me.page_faults_counter = Self::add_counter(me.query, "\\Memory\\Page Faults/sec");
        me.power_counter = Self::add_counter(me.query, "\\Power Meter(_Total)\\Power");
        me.battery_discharge_counter =
            Self::add_counter(me.query, "\\Battery Status(*)\\Discharge Rate");

        if me.disk_read_counter.is_none()
            && me.disk_write_counter.is_none()
            && me.gpu_counter.is_none()
            && me.compressed_counter.is_none()
            && me.page_faults_counter.is_none()
            && me.power_counter.is_none()
            && me.battery_discharge_counter.is_none()
        {
            return None;
        }

        unsafe {
            let _ = PdhCollectQueryData(me.query);
        }
        Some(me)
    }

    fn add_counter(query: isize, path: &str) -> Option<isize> {
        use windows::core::PCWSTR;
        use windows::Win32::System::Performance::PdhAddEnglishCounterW;

        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let mut out = 0isize;
        unsafe {
            if PdhAddEnglishCounterW(query, PCWSTR(wide.as_ptr()), 0, &mut out) == 0 {
                Some(out)
            } else {
                None
            }
        }
    }

    fn sample(
        &mut self,
    ) -> (
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
    ) {
        use windows::Win32::System::Performance::PdhCollectQueryData;

        let status = unsafe { PdhCollectQueryData(self.query) };
        if status != 0 {
            return (None, None, None, None, None, None);
        }

        let read_b_s = self.disk_read_counter.and_then(Self::counter_value);
        let write_b_s = self.disk_write_counter.and_then(Self::counter_value);
        let gpu_pct = self
            .gpu_counter
            .and_then(Self::counter_array_sum)
            .map(|v| v.clamp(0.0, 100.0));
        let compression_ratio = self
            .compressed_counter
            .and_then(Self::counter_value)
            .and_then(|bytes| {
                raw_system_memory().map(|(total, _)| (bytes / total.max(1) as f64).clamp(0.0, 1.0))
            });
        let page_faults_per_sec = self.page_faults_counter.and_then(Self::counter_value);
        let power_meter_watts = self
            .power_counter
            .and_then(Self::counter_value)
            .and_then(|v| {
                if v.is_finite() && v >= 0.0 {
                    Some(v)
                } else {
                    None
                }
            });
        // Laptop fallback: PDH battery discharge rate is typically exposed in mW.
        let battery_watts = self
            .battery_discharge_counter
            .and_then(Self::counter_array_sum)
            .and_then(|mw| {
                if mw.is_finite() {
                    let w = mw.abs() / 1000.0;
                    if w > 0.0 {
                        Some(w)
                    } else {
                        None
                    }
                } else {
                    None
                }
            });
        let power_watts = power_meter_watts.or(battery_watts);

        (
            read_b_s.map(|v| (v / 1024.0 / 1024.0).max(0.0)),
            write_b_s.map(|v| (v / 1024.0 / 1024.0).max(0.0)),
            gpu_pct,
            compression_ratio,
            power_watts,
            page_faults_per_sec,
        )
    }

    fn counter_value(counter: isize) -> Option<f64> {
        use windows::Win32::System::Performance::{
            PdhGetFormattedCounterValue, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE,
            PDH_FMT_DOUBLE,
        };

        let mut val = PDH_FMT_COUNTERVALUE::default();
        let st = unsafe { PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, None, &mut val) };
        if st != 0 || val.CStatus != PDH_CSTATUS_VALID_DATA {
            return None;
        }
        Some(unsafe { val.Anonymous.doubleValue })
    }

    fn counter_array_sum(counter: isize) -> Option<f64> {
        use windows::Win32::System::Performance::{
            PdhGetFormattedCounterArrayW, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE_ITEM_W,
            PDH_FMT_DOUBLE, PDH_MORE_DATA,
        };

        let mut buffer_size = 0u32;
        let mut item_count = 0u32;
        let st = unsafe {
            PdhGetFormattedCounterArrayW(
                counter,
                PDH_FMT_DOUBLE,
                &mut buffer_size,
                &mut item_count,
                None,
            )
        };
        if st != PDH_MORE_DATA || buffer_size == 0 || item_count == 0 {
            return None;
        }

        let mut buffer = vec![0u8; buffer_size as usize];
        let ptr = buffer.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
        let st2 = unsafe {
            PdhGetFormattedCounterArrayW(
                counter,
                PDH_FMT_DOUBLE,
                &mut buffer_size,
                &mut item_count,
                Some(ptr),
            )
        };
        if st2 != 0 {
            return None;
        }

        let items = unsafe { std::slice::from_raw_parts(ptr, item_count as usize) };
        let sum = items.iter().fold(0.0, |acc, it| {
            if it.FmtValue.CStatus == PDH_CSTATUS_VALID_DATA {
                acc + unsafe { it.FmtValue.Anonymous.doubleValue }
            } else {
                acc
            }
        });
        Some(sum)
    }
}

pub fn sample_realtime_metrics() -> (
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
) {
    #[cfg(target_os = "windows")]
    {
        let counters =
            WINDOWS_COUNTERS.get_or_init(|| WindowsRealtimeCounters::new().map(Mutex::new));
        if let Some(m) = counters {
            if let Ok(mut g) = m.lock() {
                return g.sample();
            }
        }
        (None, None, None, None, None, None)
    }
    #[cfg(not(target_os = "windows"))]
    {
        (None, None, None, None, None, None)
    }
}

pub fn gpu_utilisation() -> Option<f64> {
    let (_, _, gpu_pct, _, _, _) = sample_realtime_metrics();
    gpu_pct
}

pub fn sample_hardware_clocks() -> (Option<f64>, Option<f64>, Option<f64>) {
    let ram_clock_mhz = query_windows_numeric_lines("wmic", &["memorychip", "get", "speed"])
        .and_then(|vals| vals.into_iter().filter(|v| *v > 0.0).reduce(f64::max));
    (ram_clock_mhz, None, None)
}

fn query_windows_numeric_lines(program: &str, args: &[&str]) -> Option<Vec<f64>> {
    #[cfg(target_os = "windows")]
    {
        let out = command_hidden(program).args(args).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let txt = String::from_utf8(out.stdout).ok()?;
        let vals = txt
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .filter_map(|l| l.parse::<f64>().ok())
            .collect::<Vec<_>>();
        if vals.is_empty() {
            None
        } else {
            Some(vals)
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (program, args);
        None
    }
}
// ─── Win32 write primitives ───────────────────────────────────────────────────

fn set_power_plan(guid: &str) -> (String, bool) {
    #[cfg(target_os = "windows")]
    {
        let ok = command_hidden("powercfg")
            .args(["/setactive", guid])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        (format!("Power plan GUID {} activated", &guid[..8]), ok)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = guid;
        ("Power plan (stub non-Windows)".into(), false)
    }
}

fn set_process_affinity(mask: usize, target_pid: Option<u32>) -> (String, bool) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{
            GetCurrentProcess, OpenProcess, SetProcessAffinityMask, PROCESS_QUERY_INFORMATION,
            PROCESS_SET_INFORMATION,
        };
        unsafe {
            let (handle, own) = match target_pid {
                Some(pid) => match OpenProcess(
                    PROCESS_SET_INFORMATION | PROCESS_QUERY_INFORMATION,
                    false,
                    pid,
                ) {
                    Ok(h) => (h, true),
                    Err(_) => (GetCurrentProcess(), false),
                },
                None => (GetCurrentProcess(), false),
            };
            let ok = SetProcessAffinityMask(handle, mask).is_ok();
            if own {
                let _ = CloseHandle(handle);
            }
            return (format!("CPU affinity mask -> 0x{:04X}", mask), ok);
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = target_pid;
    #[cfg(not(target_os = "windows"))]
    (format!("CPU affinity 0x{:04X} (stub)", mask), false)
}

fn set_working_set(min: usize, max: usize, target_pid: Option<u32>) -> (String, bool) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{
            GetCurrentProcess, OpenProcess, SetProcessWorkingSetSize, PROCESS_QUERY_INFORMATION,
            PROCESS_SET_INFORMATION,
        };
        unsafe {
            let (handle, own, label) = match target_pid {
                Some(pid) => {
                    match OpenProcess(
                        PROCESS_SET_INFORMATION | PROCESS_QUERY_INFORMATION,
                        false,
                        pid,
                    ) {
                        Ok(h) => (h, true, format!("PID {}", pid)),
                        Err(_) => {
                            return (
                                format!("Working set denied for PID {} (access/protection)", pid),
                                false,
                            )
                        }
                    }
                }
                None => (GetCurrentProcess(), false, "current process".to_string()),
            };
            let ok = SetProcessWorkingSetSize(handle, min, max).is_ok();
            if own {
                let _ = CloseHandle(handle);
            }
            if ok {
                return (
                    format!(
                        "Working set {}MB-{}MB locked ({})",
                        min >> 20,
                        max >> 20,
                        label
                    ),
                    true,
                );
            }
            if target_pid.is_some() {
                let min_fb = (min / 2).max(64 * 1024 * 1024);
                let max_fb = (max * 3 / 4).max(min_fb + (64 * 1024 * 1024));
                let ok_fb = SetProcessWorkingSetSize(handle, min_fb, max_fb).is_ok();
                if ok_fb {
                    return (
                        format!(
                            "Working set fallback {}MB-{}MB applied ({})",
                            min_fb >> 20,
                            max_fb >> 20,
                            label
                        ),
                        true,
                    );
                }
            }
            return (format!("Working set lock failed ({})", label), false);
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (target_pid, min, max);
    #[cfg(not(target_os = "windows"))]
    (format!("Working set (stub)"), false)
}

fn disable_memory_compression() -> (String, bool) {
    #[cfg(target_os = "windows")]
    {
        if !is_elevated() {
            return (
                "Memory compression disable skipped (Administrator required)".into(),
                false,
            );
        }
        // Requires PowerShell admin: Disable-MMAgent -MemoryCompression
        let ok = command_hidden("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Disable-MMAgent -MemoryCompression",
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        let msg = if ok {
            "Memory compression disabled"
        } else {
            "Memory compression disable failed"
        };
        (msg.into(), ok)
    }
    #[cfg(not(target_os = "windows"))]
    {
        ("Memory compression disable (stub non-Windows)".into(), false)
    }
}

fn restore_memory_compression() -> (String, bool) {
    #[cfg(target_os = "windows")]
    {
        if !is_elevated() {
            return (
                "Memory compression restore skipped (Administrator required)".into(),
                false,
            );
        }
        let ok = command_hidden("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Enable-MMAgent -MemoryCompression",
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        let msg = if ok {
            "Memory compression restored"
        } else {
            "Memory compression restore failed"
        };
        (msg.into(), ok)
    }
    #[cfg(not(target_os = "windows"))]
    {
        ("Memory compression restore (stub non-Windows)".into(), false)
    }
}

fn set_io_priority_high(target_pid: Option<u32>) -> (String, bool) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{
            GetCurrentProcess, OpenProcess, SetPriorityClass, HIGH_PRIORITY_CLASS,
            PROCESS_QUERY_INFORMATION, PROCESS_SET_INFORMATION,
        };
        unsafe {
            let (handle, own, label) = match target_pid {
                Some(pid) => match OpenProcess(
                    PROCESS_SET_INFORMATION | PROCESS_QUERY_INFORMATION,
                    false,
                    pid,
                ) {
                    Ok(h) => (h, true, format!("PID {}", pid)),
                    Err(_) => {
                        return (
                            format!(
                                "Process priority denied for PID {} (access/protection)",
                                pid
                            ),
                            false,
                        )
                    }
                },
                None => (GetCurrentProcess(), false, "current process".to_string()),
            };
            let ok = SetPriorityClass(handle, HIGH_PRIORITY_CLASS).is_ok();
            if own {
                let _ = CloseHandle(handle);
            }
            if ok {
                return (format!("Process priority -> HIGH ({})", label), true);
            }
            return (format!("Process priority HIGH failed ({})", label), false);
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = target_pid;
    #[cfg(not(target_os = "windows"))]
    ("Process priority HIGH (stub)".into(), false)
}

fn restore_io_priority(target_pid: Option<u32>) -> (String, bool) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{
            GetCurrentProcess, OpenProcess, SetPriorityClass, NORMAL_PRIORITY_CLASS,
            PROCESS_QUERY_INFORMATION, PROCESS_SET_INFORMATION,
        };
        unsafe {
            let (handle, own) = match target_pid {
                Some(pid) => match OpenProcess(
                    PROCESS_SET_INFORMATION | PROCESS_QUERY_INFORMATION,
                    false,
                    pid,
                ) {
                    Ok(h) => (h, true),
                    Err(_) => (GetCurrentProcess(), false),
                },
                None => (GetCurrentProcess(), false),
            };
            let ok = SetPriorityClass(handle, NORMAL_PRIORITY_CLASS).is_ok();
            if own {
                let _ = CloseHandle(handle);
            }
            return ("Process priority → NORMAL".into(), ok);
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = target_pid;
    #[cfg(not(target_os = "windows"))]
    ("Process priority NORMAL (stub)".into(), false)
}

fn restore_working_set(target_pid: Option<u32>) -> (String, bool) {
    set_working_set(1 << 20, 256 << 20, target_pid)
}

fn os_version() -> String {
    #[cfg(target_os = "windows")]
    {
        command_hidden("cmd")
            .args(["/C", "ver"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_else(|| "Windows".into())
            .trim()
            .to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        "Windows".into()
    }
}

fn is_elevated_uncached() -> bool {
    #[cfg(target_os = "windows")]
    {
        command_hidden("net")
            .args(["session"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

fn memory_compression_state_uncached() -> Option<bool> {
    #[cfg(target_os = "windows")]
    {
        let out = command_hidden("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                "(Get-MMAgent).MemoryCompression",
            ])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8(out.stdout).ok()?;
        match s.trim().to_ascii_lowercase().as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

fn windows_priv_snapshot() -> (bool, Option<bool>) {
    #[cfg(target_os = "windows")]
    {
        let cache = WINDOWS_PRIV_CACHE.get_or_init(|| Mutex::new(None));
        let mut guard = match cache.lock() {
            Ok(v) => v,
            Err(_) => return (is_elevated_uncached(), memory_compression_state_uncached()),
        };
        let ttl = std::time::Duration::from_secs(10);
        if let Some(cached) = guard.as_ref() {
            if cached.at.elapsed() < ttl {
                return (cached.is_admin, cached.memory_compression);
            }
        }
        let is_admin = is_elevated_uncached();
        let memory_compression = memory_compression_state_uncached();
        *guard = Some(WindowsPrivCache {
            at: std::time::Instant::now(),
            is_admin,
            memory_compression,
        });
        (is_admin, memory_compression)
    }
    #[cfg(not(target_os = "windows"))]
    {
        (false, None)
    }
}

fn is_elevated() -> bool {
    windows_priv_snapshot().0
}

fn memory_compression_state() -> Option<bool> {
    windows_priv_snapshot().1
}

pub fn ensure_admin_or_relaunch() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        if is_elevated() {
            return Ok(true);
        }

        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let args: Vec<String> = std::env::args().skip(1).collect();

        let quote_ps = |s: &str| -> String { format!("'{}'", s.replace('\'', "''")) };
        let file = quote_ps(exe.to_string_lossy().as_ref());
        let arg_list = if args.is_empty() {
            String::new()
        } else {
            let items = args
                .iter()
                .map(|a| quote_ps(a))
                .collect::<Vec<_>>()
                .join(",");
            format!(" -ArgumentList @({})", items)
        };
        let cmd = format!("Start-Process -FilePath {}{} -Verb RunAs", file, arg_list);

        let status = command_hidden("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &cmd])
            .status()
            .map_err(|e| e.to_string())?;

        if status.success() {
            Ok(false)
        } else {
            Err("UAC elevation refused or failed".into())
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(true)
    }
}

pub fn policy_status(mode: crate::platform::PolicyMode) -> crate::platform::PolicyStatus {
    crate::platform::PolicyStatus {
        mode: mode.as_name().into(),
        is_admin: is_elevated(),
        reboot_pending: is_reboot_pending(),
        memory_compression_enabled: memory_compression_state(),
    }
}

fn is_reboot_pending() -> bool {
    #[cfg(target_os = "windows")]
    {
        let script = "if (Test-Path 'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Component Based Servicing\\RebootPending') { exit 10 }; if (Test-Path 'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\WindowsUpdate\\Auto Update\\RebootRequired') { exit 11 }; $p = Get-ItemProperty -Path 'HKLM:\\SYSTEM\\CurrentControlSet\\Control\\Session Manager' -Name PendingFileRenameOperations -ErrorAction SilentlyContinue; if ($p) { exit 12 }; exit 0";
        command_hidden("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .status()
            .map(|s| !s.success())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

pub fn memory_optimizer_factor() -> f64 {
    // WinMemoryCleaner-like strategy: memory compression + global working set trim.
    // Full efficiency requires elevation; partial efficiency still exists without it.
    let elevated = is_elevated();
    let compression_on = memory_compression_state().unwrap_or(false);
    match (elevated, compression_on) {
        (true, true) => 0.95,
        (true, false) => 0.70,
        (false, true) => 0.60,
        (false, false) => 0.35,
    }
}

pub fn soulram_backend_name() -> String {
    "Windows Memory Compression + WorkingSet Trim".into()
}

pub async fn enable_soulram(percent: u8) -> Vec<(String, bool)> {
    let pct = percent.clamp(5, 60);
    let mut out = vec![(format!("SoulRAM target ratio -> {}%", pct), true)];
    crate::memory_policy::record_compression_toggle();
    out.push(enable_memory_compression());
    let (allow_trim, notes) = crate::memory_policy::allow_global_trim(None);
    for n in notes {
        out.push((n, true));
    }
    if allow_trim {
        let (msg, ok) = trim_working_sets_global();
        if ok {
            crate::memory_policy::record_global_working_set_trim();
        }
        out.push((msg, ok));
    }
    out
}

pub async fn disable_soulram() -> Vec<(String, bool)> {
    vec![
        disable_memory_compression(),
        ("SoulRAM disabled (Windows backend)".into(), true),
    ]
}

fn enable_memory_compression() -> (String, bool) {
    #[cfg(target_os = "windows")]
    {
        if !is_elevated() {
            return (
                "Memory compression enable skipped (Administrator required)".into(),
                false,
            );
        }
        let ok = command_hidden("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Enable-MMAgent -MemoryCompression",
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        let state_after = memory_compression_state();
        let effective_ok = ok || state_after == Some(true);
        let msg = if ok {
            "Memory compression enabled"
        } else if state_after == Some(true) {
            "Memory compression already enabled (Windows reports restart pending)"
        } else {
            "Memory compression enable failed (restart may be required)"
        };
        (msg.into(), effective_ok)
    }
    #[cfg(not(target_os = "windows"))]
    {
        ("Memory compression enable (stub non-Windows)".into(), false)
    }
}

fn trim_working_sets_global() -> (String, bool) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{
            OpenProcess, SetProcessWorkingSetSize, PROCESS_QUERY_INFORMATION, PROCESS_SET_QUOTA,
        };

        let mut sys = sysinfo::System::new_all();
        sys.refresh_all();

        let mut ok_count = 0usize;
        let mut tried = 0usize;

        unsafe {
            for pid in sys.processes().keys() {
                tried += 1;
                let h = OpenProcess(
                    PROCESS_SET_QUOTA | PROCESS_QUERY_INFORMATION,
                    false,
                    pid.as_u32(),
                );
                if let Ok(handle) = h {
                    let ok = SetProcessWorkingSetSize(handle, usize::MAX, usize::MAX).is_ok();
                    if ok {
                        ok_count += 1;
                    }
                    let _ = CloseHandle(handle);
                }
            }
        }

        let ok = ok_count > 0;
        return (
            format!(
                "Global working-set trim -> {}/{} process(es)",
                ok_count, tried
            ),
            ok,
        );
    }
    #[cfg(not(target_os = "windows"))]
    ("Global working-set trim (stub)".into(), false)
}
