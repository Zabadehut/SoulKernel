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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoulRamBackendInfo {
    pub platform: String,
    pub backend: String,
    pub equivalent_goal: String,
    pub roadmap: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CpuBias {
    Eco,
    Balanced,
    Boost,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IoBias {
    Eco,
    Balanced,
    Boost,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GpuBias {
    Eco,
    Balanced,
    Boost,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryBias {
    Eco,
    Balanced,
    Boost,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveActionProfile {
    pub cpu_bias: CpuBias,
    pub io_bias: IoBias,
    pub gpu_bias: GpuBias,
    pub memory_bias: MemoryBias,
    pub sigma_guard: bool,
    pub thermal_guard: bool,
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

pub fn derive_adaptive_action_profile(
    profile: &WorkloadProfile,
    eta: f64,
    baseline: &ResourceState,
    policy: PolicyMode,
) -> AdaptiveActionProfile {
    let cpu_hot = baseline.raw.cpu_temp_c.map(|t| t >= 82.0).unwrap_or(false);
    let gpu_hot = baseline.raw.gpu_temp_c.map(|t| t >= 78.0).unwrap_or(false);
    let sigma_guard = baseline.sigma >= 0.68 || matches!(policy, PolicyMode::Safe);
    let thermal_guard = cpu_hot || gpu_hot;

    let cpu_bias = if sigma_guard || cpu_hot {
        CpuBias::Eco
    } else if profile.alpha[0] > 0.42 || eta >= 0.22 {
        CpuBias::Boost
    } else {
        CpuBias::Balanced
    };

    let io_bias = if sigma_guard {
        IoBias::Eco
    } else if profile.alpha[3] > 0.35 {
        IoBias::Boost
    } else {
        IoBias::Balanced
    };

    let gpu_bias = if gpu_hot || baseline.gpu.unwrap_or(0.0) > 0.82 {
        GpuBias::Eco
    } else if profile.alpha[4] > 0.35 && baseline.sigma < 0.65 {
        GpuBias::Boost
    } else {
        GpuBias::Balanced
    };

    let memory_bias = if thermal_guard || sigma_guard {
        MemoryBias::Eco
    } else if profile.alpha[1] + profile.alpha[2] > 0.36 {
        MemoryBias::Boost
    } else {
        MemoryBias::Balanced
    };

    AdaptiveActionProfile {
        cpu_bias,
        io_bias,
        gpu_bias,
        memory_bias,
        sigma_guard,
        thermal_guard,
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

/// Indique si au moins une action SoulRAM a eu un effet réel (hors libellés purement informatifs).
/// Sur Windows, la ligne « SoulRAM target ratio » et les notes MemoryPolicy (cooldown) ne comptent pas.
pub fn soulram_enablement_effective(actions: &[(String, bool)]) -> bool {
    actions.iter().any(|(msg, ok)| {
        if !*ok {
            return false;
        }
        if msg.contains("SoulRAM target ratio") {
            return false;
        }
        if msg.contains("MemoryPolicy:") {
            return false;
        }
        true
    })
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

pub fn soulram_backend_info() -> SoulRamBackendInfo {
    #[cfg(target_os = "linux")]
    return linux::soulram_backend_info();

    #[cfg(target_os = "windows")]
    return windows::soulram_backend_info();

    #[cfg(target_os = "macos")]
    return macos::soulram_backend_info();

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    SoulRamBackendInfo {
        platform: "unknown".into(),
        backend: "Unsupported".into(),
        equivalent_goal: "Memory relief backend unavailable on this OS".into(),
        roadmap: vec![
            "Identifier un backend memoire natif defensable.".into(),
            "Exposer les preconditions privilegees et l'etat reel.".into(),
            "Mesurer l'effet host avant/apres au lieu de promettre un gain fixe.".into(),
        ],
    }
}
