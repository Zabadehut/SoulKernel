//! platform/mod.rs - OS detection & dispatch layer
//!
//! Routes all hardware reads/writes to the correct platform implementation.
//! Linux: full orchestration (cgroups, zRAM, governor, cpuset)
//! Windows: job objects, affinity, working set, power plans
//! macOS: QoS classes, thread affinity, pmset power hints

pub mod linux;
pub mod macos;
pub mod windows;

use crate::{formula::WorkloadProfile, metrics::ResourceState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyMode {
    Safe,
    Privileged,
}

impl PolicyMode {
    pub fn from_name(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "safe" => Self::Safe,
            _ => Self::Privileged,
        }
    }

    pub fn as_name(&self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Privileged => "privileged",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyStatus {
    pub mode: String,
    pub is_admin: bool,
    pub reboot_pending: bool,
    pub memory_compression_enabled: Option<bool>,
}

// Platform info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformInfo {
    pub os: String,
    pub kernel: String,
    pub features: Vec<String>,
    pub has_cgroups_v2: bool,
    pub has_zram: bool,
    pub has_gpu_sysfs: bool,
    pub is_root: bool,
}

pub fn info() -> PlatformInfo {
    #[cfg(target_os = "linux")]
    return linux::platform_info();

    #[cfg(target_os = "windows")]
    return windows::platform_info();

    #[cfg(target_os = "macos")]
    return macos::platform_info();

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    PlatformInfo {
        os: "Unknown".into(),
        kernel: "Unknown".into(),
        features: vec![],
        has_cgroups_v2: false,
        has_zram: false,
        has_gpu_sysfs: false,
        is_root: false,
    }
}

// Dome application
pub async fn apply_dome_profile(
    profile: &WorkloadProfile,
    eta: f64,
    baseline: &ResourceState,
    policy: PolicyMode,
    target_pid: Option<u32>,
) -> Vec<(String, bool)> {
    #[cfg(target_os = "linux")]
    return linux::apply_dome(profile, eta, baseline, policy, target_pid).await;

    #[cfg(target_os = "windows")]
    return windows::apply_dome(profile, eta, baseline, policy, target_pid).await;

    #[cfg(target_os = "macos")]
    return macos::apply_dome(profile, eta, baseline, policy, target_pid).await;

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    let _ = (profile, eta, baseline, policy, target_pid);
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    vec![("No platform implementation".into(), false)]
}

pub async fn rollback_dome_profile(
    snapshot: Option<ResourceState>,
    target_pid: Option<u32>,
) -> Vec<(String, bool)> {
    #[cfg(target_os = "linux")]
    return linux::rollback(snapshot, target_pid).await;

    #[cfg(target_os = "windows")]
    return windows::rollback(snapshot, target_pid).await;

    #[cfg(target_os = "macos")]
    return macos::rollback(snapshot, target_pid).await;

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    let _ = (snapshot, target_pid);
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    vec![("No platform implementation".into(), false)]
}

// SoulRAM
pub async fn enable_soulram(percent: u8) -> Vec<(String, bool)> {
    #[cfg(target_os = "linux")]
    return linux::enable_soulram(percent).await;

    #[cfg(target_os = "windows")]
    return windows::enable_soulram(percent).await;

    #[cfg(target_os = "macos")]
    return macos::enable_soulram(percent).await;

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    let _ = percent;
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    vec![("SoulRAM not supported on this platform".into(), false)]
}

pub async fn disable_soulram() -> Vec<(String, bool)> {
    #[cfg(target_os = "linux")]
    return linux::disable_soulram().await;

    #[cfg(target_os = "windows")]
    return windows::disable_soulram().await;

    #[cfg(target_os = "macos")]
    return macos::disable_soulram().await;

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    vec![("SoulRAM not supported on this platform".into(), false)]
}

/// Returns a normalized memory-optimizer capability factor in [0,1].
/// Higher means the OS/runtime can reclaim memory more effectively right now.
pub fn memory_optimizer_factor() -> f64 {
    #[cfg(target_os = "linux")]
    return linux::memory_optimizer_factor();

    #[cfg(target_os = "windows")]
    return windows::memory_optimizer_factor();

    #[cfg(target_os = "macos")]
    return macos::memory_optimizer_factor();

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    0.0
}

pub fn policy_status(mode: PolicyMode) -> PolicyStatus {
    #[cfg(target_os = "windows")]
    return windows::policy_status(mode);

    #[cfg(target_os = "linux")]
    return linux::policy_status(mode);

    #[cfg(target_os = "macos")]
    return macos::policy_status(mode);

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    PolicyStatus {
        mode: mode.as_name().into(),
        is_admin: false,
        reboot_pending: false,
        memory_compression_enabled: None,
    }
}

pub fn ensure_admin_or_relaunch() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    return windows::ensure_admin_or_relaunch();

    #[cfg(not(target_os = "windows"))]
    Ok(true)
}

pub fn soulram_backend_name() -> String {
    #[cfg(target_os = "linux")]
    return linux::soulram_backend_name();

    #[cfg(target_os = "windows")]
    return windows::soulram_backend_name();

    #[cfg(target_os = "macos")]
    return macos::soulram_backend_name();

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    "Unsupported".into()
}
