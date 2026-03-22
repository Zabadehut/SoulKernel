//! formula.rs — Pure mathematics engine
//!
//! Implements the unified formula:
//!
//!   D*[τ₀,τ₁] = max_P [ ∫ π(t) dt  −  C_setup  −  C_rollback ]
//!
//!   where  π(t) = (𝒲 · r(t)) · ∏_k (1−ε_k)^α_k · e^{−κΣ(t)}

use crate::metrics::ResourceState;
use serde::{Deserialize, Serialize};

// ─── Workload profiles (tensor 𝒲) ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadProfile {
    pub name: String,
    /// α = [α_C, α_M, α_Λ, α_io, α_G]  with Σα = 1
    pub alpha: [f64; 5],
    /// Estimated task duration for integral approximation (seconds)
    pub duration_estimate_s: f64,
}

impl WorkloadProfile {
    pub fn all() -> Vec<Self> {
        vec![
            Self {
                name: "es".into(),
                alpha: [0.20, 0.35, 0.20, 0.25, 0.00],
                duration_estimate_s: 60.0,
            },
            Self {
                name: "compile".into(),
                alpha: [0.55, 0.25, 0.10, 0.10, 0.00],
                duration_estimate_s: 120.0,
            },
            Self {
                name: "gamer".into(),
                alpha: [0.45, 0.20, 0.05, 0.05, 0.25],
                duration_estimate_s: 90.0,
            },
            Self {
                name: "ai".into(),
                alpha: [0.15, 0.20, 0.05, 0.05, 0.55],
                duration_estimate_s: 30.0,
            },
            Self {
                name: "backup".into(),
                alpha: [0.15, 0.15, 0.30, 0.40, 0.00],
                duration_estimate_s: 300.0,
            },
            Self {
                name: "sqlite".into(),
                alpha: [0.20, 0.10, 0.05, 0.65, 0.00],
                duration_estimate_s: 20.0,
            },
            Self {
                name: "oracle".into(),
                alpha: [0.25, 0.30, 0.10, 0.35, 0.00],
                duration_estimate_s: 180.0,
            },
        ]
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::all().into_iter().find(|p| p.name == name)
    }
}

// ─── Output ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormulaResult {
    /// π(t) — instantaneous performance yield
    pub pi: f64,
    /// 𝒲 · r(t) — raw weighted gain before penalties
    pub brut: f64,
    /// ∏(1−ε_k)^α_k — contention friction factor
    pub friction: f64,
    /// e^{−κΣ} — stability brake
    pub brake: f64,
    /// ∫π dt − C_setup − C_rollback — net dome gain
    pub dome_gain: f64,
    /// B_idle — borrowable idle capacity
    pub b_idle: f64,
    /// Whether dome would be profitable AND safe
    pub rentable: bool,
    /// Breakdown per resource dimension
    pub dimension_weights: [f64; 5],
}

// ─── Constants ───────────────────────────────────────────────────────────────

const C_SETUP: f64 = 0.040; // cost of kernel reconfiguration
const C_ROLLBACK: f64 = 0.020; // cost of restoring defaults

// δ_k — "mobilisability" coefficients per resource
const DELTA: [f64; 5] = [0.90, 0.85, 0.95, 0.80, 0.70];

// ─── Core computation ─────────────────────────────────────────────────────────

pub fn compute(state: &ResourceState, profile: &WorkloadProfile, kappa: f64) -> FormulaResult {
    let r = resource_vec(state);
    let alpha = profile.alpha;
    let eps = state.epsilon;

    // ── Brut gain: 𝒲 · r(t) ────────────────────────────────────────────────
    let brut: f64 = alpha.iter().zip(r.iter()).map(|(a, rv)| a * rv).sum();

    // ── Friction: ∏_k (1 − ε_k)^α_k ────────────────────────────────────────
    let friction: f64 = alpha
        .iter()
        .zip(eps.iter())
        .map(|(a, e)| (1.0_f64 - e).max(0.0).powf(*a))
        .product();

    // ── Stability brake: e^{−κΣ(t)} ─────────────────────────────────────────
    let brake = (-kappa * state.sigma).exp();

    // ── π(t) ────────────────────────────────────────────────────────────────
    let pi = brut * friction * brake;

    // ── Dome integral approximation: π · T ──────────────────────────────────
    let dome_gain = pi * profile.duration_estimate_s - C_SETUP - C_ROLLBACK;

    // ── B_idle: borrowable idle capacity ─────────────────────────────────────
    // B_idle = Σ_k α_k · (1 − ū_k) · δ_k   where ū_k ≈ r_k · 0.7
    let b_idle: f64 = alpha
        .iter()
        .zip(r.iter())
        .zip(DELTA.iter())
        .map(|((a, rv), dk)| a * (1.0 - rv * 0.7) * dk)
        .sum();

    // ── Dimension weights for visualisation ─────────────────────────────────
    let dimension_weights = std::array::from_fn(|i| alpha[i] * r[i]);

    FormulaResult {
        pi,
        brut,
        friction,
        brake,
        dome_gain,
        b_idle,
        rentable: dome_gain > 0.0,
        dimension_weights,
    }
}

// ─── Helper ──────────────────────────────────────────────────────────────────

fn resource_vec(s: &ResourceState) -> [f64; 5] {
    [
        s.cpu,
        s.mem,
        s.compression.unwrap_or(0.0),
        s.io_bandwidth.unwrap_or(0.0),
        s.gpu.unwrap_or(0.0),
    ]
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{RawMetrics, ResourceState};

    fn mock_state(cpu: f64, mem: f64, sigma: f64) -> ResourceState {
        ResourceState {
            cpu,
            mem,
            compression: Some(0.5),
            io_bandwidth: Some(0.3),
            gpu: Some(0.2),
            sigma,
            epsilon: [0.05, 0.08, 0.03, 0.04, 0.02],
            raw: RawMetrics {
                cpu_pct: cpu * 100.0,
                cpu_clock_mhz: None,
                mem_used_mb: 4096,
                mem_total_mb: 16384,
                ram_clock_mhz: None,
                swap_used_mb: 0,
                swap_total_mb: 8192,
                zram_used_mb: None,
                io_read_mb_s: Some(150.0),
                io_write_mb_s: Some(50.0),
                gpu_pct: Some(20.0),
                gpu_core_clock_mhz: None,
                gpu_mem_clock_mhz: None,
                power_watts: None,
                psi_cpu: None,
                psi_mem: None,
                on_battery: None,
                battery_percent: None,
                page_faults_per_sec: None,
                platform: "Test".into(),
                webview_host_cpu_sum: None,
                webview_host_mem_mb: None,
            },
        }
    }

    #[test]
    fn pi_is_in_unit_range() {
        for profile in WorkloadProfile::all() {
            let state = mock_state(0.5, 0.7, 0.3);
            let result = compute(&state, &profile, 2.0);
            assert!(result.pi >= 0.0, "π must be ≥ 0 for {}", profile.name);
            assert!(result.pi <= 1.0, "π must be ≤ 1 for {}", profile.name);
        }
    }

    #[test]
    fn friction_is_in_unit_range() {
        let state = mock_state(0.5, 0.7, 0.3);
        let profile = WorkloadProfile::from_name("es").unwrap();
        let result = compute(&state, &profile, 2.0);
        assert!(result.friction >= 0.0 && result.friction <= 1.0);
    }

    #[test]
    fn brake_decreases_with_sigma() {
        let profile = WorkloadProfile::from_name("compile").unwrap();
        let r_low = compute(&mock_state(0.3, 0.8, 0.1), &profile, 2.0);
        let r_high = compute(&mock_state(0.3, 0.8, 0.9), &profile, 2.0);
        assert!(
            r_low.brake > r_high.brake,
            "brake must decrease as sigma increases"
        );
    }

    #[test]
    fn dome_gain_higher_for_longer_duration() {
        let state = mock_state(0.4, 0.8, 0.2);
        let short = WorkloadProfile {
            name: "short".into(),
            alpha: [0.2, 0.35, 0.2, 0.25, 0.0],
            duration_estimate_s: 10.0,
        };
        let long = WorkloadProfile {
            name: "long".into(),
            alpha: [0.2, 0.35, 0.2, 0.25, 0.0],
            duration_estimate_s: 300.0,
        };
        let r_short = compute(&state, &short, 2.0);
        let r_long = compute(&state, &long, 2.0);
        assert!(r_long.dome_gain > r_short.dome_gain);
    }

    #[test]
    fn b_idle_positive_when_resources_available() {
        let state = mock_state(0.3, 0.8, 0.2);
        let profile = WorkloadProfile::from_name("es").unwrap();
        let result = compute(&state, &profile, 2.0);
        assert!(
            result.b_idle > 0.0,
            "B_idle must be positive with available resources"
        );
    }

    #[test]
    fn all_profiles_exist() {
        let profiles = WorkloadProfile::all();
        assert!(profiles.len() >= 7);
        for name in &["es", "compile", "gamer", "ai", "backup", "sqlite", "oracle"] {
            assert!(
                WorkloadProfile::from_name(name).is_some(),
                "Missing profile: {}",
                name
            );
        }
    }

    #[test]
    fn alpha_weights_sum_to_one() {
        for profile in WorkloadProfile::all() {
            let sum: f64 = profile.alpha.iter().sum();
            assert!(
                (sum - 1.0).abs() < 1e-10,
                "α weights must sum to 1.0 for {}: got {}",
                profile.name,
                sum
            );
        }
    }

    #[test]
    fn zero_kappa_means_no_brake() {
        let state = mock_state(0.5, 0.7, 0.5);
        let profile = WorkloadProfile::from_name("es").unwrap();
        let result = compute(&state, &profile, 0.0);
        assert!(
            (result.brake - 1.0).abs() < 1e-10,
            "brake must be 1.0 when κ=0"
        );
    }

    #[test]
    fn dimension_weights_consistent() {
        let state = mock_state(0.5, 0.7, 0.3);
        let profile = WorkloadProfile::from_name("gamer").unwrap();
        let result = compute(&state, &profile, 2.0);
        for (i, dw) in result.dimension_weights.iter().enumerate() {
            assert!(*dw >= 0.0, "dimension_weight[{}] must be ≥ 0", i);
        }
    }
}
