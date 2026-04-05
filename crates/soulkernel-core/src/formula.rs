//! formula.rs — Pure mathematics engine
//!
//! Implements two complementary layers:
//!
//! 1. A real-time dome decision score:
//!
//!   D*[τ₀,τ₁] = max_P [ ∫ π(t) dt  −  C_setup  −  C_rollback ]
//!
//!   where  π(t) = (𝒲 · r(t)) · ∏_k (1−ε_k)^α_k · e^{−κΣ(t)}
//!
//! 2. A measured efficiency KPI for benchmarks / reports:
//!
//!   Utility-per-Watt      = (U / t) / P
//!   Energy-per-Utility   = E / U
//!
//! The first answers "should we enable the dome now?"
//! The second answers "did ON actually improve useful work per watt?"

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
        crate::workload_catalog::all_profiles()
    }

    pub fn from_name(name: &str) -> Option<Self> {
        crate::workload_catalog::from_name(name)
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
    /// Extra opportunity multiplier from advanced metrics [0,1.35].
    pub opportunity: f64,
    /// Extra guard multiplier from advanced metrics [0,1].
    pub advanced_guard: f64,
    /// Effective sigma after advanced penalties.
    pub sigma_effective: f64,
}

/// Measured efficiency input for one completed task / benchmark sample.
/// `useful_output` is generic: completed task count, work units, rows processed, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasuredEfficiencyInput {
    /// Useful output U over the measured window. Must be > 0.
    pub useful_output: f64,
    /// Optional quality/reliability multiplier η in [0,1]. Defaults to 1.
    pub quality_factor: Option<f64>,
    /// Average power over the measured window in Watts. Must be > 0.
    pub avg_power_watts: f64,
    /// Measured duration in seconds. Must be > 0.
    pub duration_s: f64,
}

/// True measured efficiency KPI derived from useful output and measured power.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasuredEfficiency {
    /// Raw useful output U.
    pub useful_output: f64,
    /// Quality-adjusted useful output U × η.
    pub effective_utility: f64,
    /// Applied η factor.
    pub quality_factor: f64,
    /// Average power P in Watts.
    pub avg_power_watts: f64,
    /// Window duration in seconds.
    pub duration_s: f64,
    /// Energy E in kWh over the window.
    pub energy_kwh: f64,
    /// Useful output rate U/t.
    pub utility_rate_per_s: f64,
    /// Main KPI: ((U × η) / t) / P.
    pub utility_per_watt: f64,
    /// Inverse instantaneous KPI: P / ((U × η) / t).
    pub watts_per_utility_rate: f64,
    /// Cycle KPI: (U × η) / E.
    pub utility_per_kwh: f64,
    /// Inverse cycle KPI: E / (U × η).
    pub kwh_per_utility: f64,
}

/// Measured OFF vs ON comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasuredEfficiencyComparison {
    pub off: MeasuredEfficiency,
    pub on: MeasuredEfficiency,
    /// Positive when ON improves useful work per Watt.
    pub gain_utility_per_watt_pct: f64,
    /// Positive when ON reduces energy per useful output.
    pub gain_kwh_per_utility_pct: f64,
    /// Positive when ON reduces instantaneous Watts per useful-rate.
    pub gain_watts_per_utility_rate_pct: f64,
}

// ─── Constants ───────────────────────────────────────────────────────────────

const C_SETUP: f64 = 0.040; // cost of kernel reconfiguration
const C_ROLLBACK: f64 = 0.020; // cost of restoring defaults

// δ_k — "mobilisability" coefficients per resource
const DELTA: [f64; 5] = [0.90, 0.85, 0.95, 0.80, 0.70];

// ─── Core computation ─────────────────────────────────────────────────────────

pub fn compute(
    state: &ResourceState,
    profile: &WorkloadProfile,
    kappa: f64,
    p_active_hint_w: Option<f64>,
) -> FormulaResult {
    let r = resource_vec(state);
    let alpha = profile.alpha;
    let eps = state.epsilon;
    let opportunity = advanced_opportunity(state, profile);
    let sigma_effective = effective_sigma(state, profile);

    // ── Brut gain: 𝒲 · r(t) ────────────────────────────────────────────────
    let brut_base: f64 = alpha.iter().zip(r.iter()).map(|(a, rv)| a * rv).sum();
    let brut = (brut_base * opportunity).clamp(0.0, 1.2);

    // ── Friction: ∏_k (1 − ε_k)^α_k ────────────────────────────────────────
    let friction_base: f64 = alpha
        .iter()
        .zip(eps.iter())
        .map(|(a, e)| (1.0_f64 - e).max(0.0).powf(*a))
        .product();
    let advanced_guard = advanced_guard(state, profile, p_active_hint_w);
    let friction = (friction_base * advanced_guard).clamp(0.0, 1.0);

    // ── Stability brake: e^{−κΣ(t)} ─────────────────────────────────────────
    let brake = (-kappa * sigma_effective).exp();

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
        opportunity,
        advanced_guard,
        sigma_effective,
    }
}

/// Computes a true measured efficiency KPI from useful output and measured power.
pub fn compute_measured_efficiency(input: MeasuredEfficiencyInput) -> Option<MeasuredEfficiency> {
    if !input.useful_output.is_finite()
        || !input.avg_power_watts.is_finite()
        || !input.duration_s.is_finite()
    {
        return None;
    }
    if input.useful_output <= 0.0 || input.avg_power_watts <= 0.0 || input.duration_s <= 0.0 {
        return None;
    }

    let quality_factor = input
        .quality_factor
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let effective_utility = input.useful_output * quality_factor;
    if effective_utility <= 0.0 {
        return None;
    }

    let energy_kwh = input.avg_power_watts * input.duration_s / 3_600_000.0;
    if energy_kwh <= 0.0 {
        return None;
    }

    let utility_rate_per_s = effective_utility / input.duration_s;
    let utility_per_watt = utility_rate_per_s / input.avg_power_watts;
    let watts_per_utility_rate = input.avg_power_watts / utility_rate_per_s;
    let utility_per_kwh = effective_utility / energy_kwh;
    let kwh_per_utility = energy_kwh / effective_utility;

    Some(MeasuredEfficiency {
        useful_output: input.useful_output,
        effective_utility,
        quality_factor,
        avg_power_watts: input.avg_power_watts,
        duration_s: input.duration_s,
        energy_kwh,
        utility_rate_per_s,
        utility_per_watt,
        watts_per_utility_rate,
        utility_per_kwh,
        kwh_per_utility,
    })
}

/// Compares true measured efficiency OFF vs ON.
pub fn compare_measured_efficiency(
    off: MeasuredEfficiency,
    on: MeasuredEfficiency,
) -> MeasuredEfficiencyComparison {
    let gain_utility_per_watt_pct =
        ((on.utility_per_watt - off.utility_per_watt) / off.utility_per_watt) * 100.0;
    let gain_kwh_per_utility_pct =
        ((off.kwh_per_utility - on.kwh_per_utility) / off.kwh_per_utility) * 100.0;
    let gain_watts_per_utility_rate_pct = ((off.watts_per_utility_rate
        - on.watts_per_utility_rate)
        / off.watts_per_utility_rate)
        * 100.0;

    MeasuredEfficiencyComparison {
        off,
        on,
        gain_utility_per_watt_pct,
        gain_kwh_per_utility_pct,
        gain_watts_per_utility_rate_pct,
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

fn advanced_opportunity(state: &ResourceState, profile: &WorkloadProfile) -> f64 {
    let cpu_headroom = state
        .raw
        .cpu_freq_ratio
        .map(|v| (1.0 - v).clamp(0.0, 1.0))
        .unwrap_or((1.0 - state.cpu).clamp(0.0, 1.0));
    let gpu_headroom = state.gpu.map(|v| (1.0 - v).clamp(0.0, 1.0)).unwrap_or(0.45);
    let io_headroom = state
        .io_bandwidth
        .map(|v| (1.0 - v).clamp(0.0, 1.0))
        .unwrap_or(0.45);
    let mem_headroom = state.mem.clamp(0.0, 1.0);
    let headroom_mix = profile.alpha[0] * cpu_headroom
        + profile.alpha[1] * mem_headroom
        + profile.alpha[3] * io_headroom
        + profile.alpha[4] * gpu_headroom;

    let ui_penalty = state
        .raw
        .webview_host_cpu_sum
        .map(|v| (v / 100.0).clamp(0.0, 0.25))
        .unwrap_or(0.0);

    (0.85 + 0.40 * headroom_mix - 0.20 * ui_penalty).clamp(0.70, 1.35)
}

fn advanced_guard(state: &ResourceState, profile: &WorkloadProfile, p_active_hint_w: Option<f64>) -> f64 {
    let cpu_hot = state
        .raw
        .cpu_temp_c
        .map(|t| ((t - 80.0) / 18.0).clamp(0.0, 1.0))
        .unwrap_or(0.0);
    let gpu_hot = state
        .raw
        .gpu_temp_c
        .map(|t| ((t - 76.0) / 16.0).clamp(0.0, 1.0))
        .unwrap_or(0.0);
    let gpu_vram_pressure = match (state.raw.gpu_mem_used_mb, state.raw.gpu_mem_total_mb) {
        (Some(used), Some(total)) if total > 0 => (used as f64 / total as f64).clamp(0.0, 1.0),
        _ => 0.0,
    };
    let load_pressure = state
        .raw
        .load_avg_1m_norm
        .map(|v| ((v - 0.9) / 1.3).clamp(0.0, 1.0))
        .unwrap_or(0.0);
    let phase_penalty =
        (profile.alpha[4] * gpu_vram_pressure + profile.alpha[0] * load_pressure).clamp(0.0, 1.0);

    // Total machine power pressure — only fires for real wall/PDH measurements (not RAPL/estimated partials).
    // Normalized against p_active_hint_w if known (learned from telemetry EMA), otherwise
    // falls back to a generic 120W reference. Pressure = 0 at or below the reference,
    // scaling to 1.0 when power is double the reference.
    let power_pressure = state
        .raw
        .power_watts
        .filter(|_| {
            !state
                .raw
                .power_watts_source
                .as_deref()
                .map(|s| {
                    s.contains("rapl")
                        || s.contains("pd_estimated")
                        || s.contains("usb_pd_measured")
                })
                .unwrap_or(false)
        })
        .map(|w| {
            let p_ref = p_active_hint_w.unwrap_or(120.0).max(20.0);
            ((w - p_ref) / p_ref).clamp(0.0, 1.0)
        })
        .unwrap_or(0.0);
    let faults_pressure = state
        .raw
        .page_faults_per_sec
        .map(|pf| (pf / 20_000.0).clamp(0.0, 1.0))
        .unwrap_or(0.0);

    (1.0
        - 0.30 * cpu_hot
        - 0.25 * gpu_hot
        - 0.18 * phase_penalty
        - 0.12 * power_pressure
        - 0.20 * faults_pressure)
        .clamp(0.45, 1.0)
}

fn effective_sigma(state: &ResourceState, profile: &WorkloadProfile) -> f64 {
    let load_penalty = state
        .raw
        .load_avg_1m_norm
        .map(|v| ((v - 1.0) / 1.5).clamp(0.0, 1.0))
        .unwrap_or(0.0);
    let power_penalty = state
        .raw
        .gpu_power_watts
        .map(|w| (w / 220.0).clamp(0.0, 1.0) * profile.alpha[4])
        .unwrap_or(0.0);
    let runnable_penalty = state
        .raw
        .runnable_tasks
        .map(|n| ((n as f64 - 2.0) / 10.0).clamp(0.0, 1.0) * profile.alpha[0])
        .unwrap_or(0.0);
    (state.sigma + 0.16 * load_penalty + 0.12 * power_penalty + 0.10 * runnable_penalty)
        .clamp(0.0, 1.0)
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
                cpu_max_clock_mhz: None,
                cpu_freq_ratio: None,
                cpu_temp_c: None,
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
                gpu_temp_c: None,
                gpu_power_watts: None,
                gpu_power_source: None,
                gpu_power_confidence: None,
                gpu_mem_used_mb: None,
                gpu_mem_total_mb: None,
                gpu_devices: Vec::new(),
                power_watts: None,
                power_watts_source: None,
                host_power_watts: None,
                host_power_watts_source: None,
                wall_power_watts: None,
                wall_power_watts_source: None,
                psi_cpu: None,
                psi_mem: None,
                load_avg_1m_norm: None,
                runnable_tasks: None,
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
            let result = compute(&state, &profile, 2.0, None);
            assert!(result.pi >= 0.0, "π must be ≥ 0 for {}", profile.name);
            assert!(result.pi <= 1.0, "π must be ≤ 1 for {}", profile.name);
        }
    }

    #[test]
    fn friction_is_in_unit_range() {
        let state = mock_state(0.5, 0.7, 0.3);
        let profile = WorkloadProfile::from_name("es").unwrap();
        let result = compute(&state, &profile, 2.0, None);
        assert!(result.friction >= 0.0 && result.friction <= 1.0);
    }

    #[test]
    fn brake_decreases_with_sigma() {
        let profile = WorkloadProfile::from_name("compile").unwrap();
        let r_low = compute(&mock_state(0.3, 0.8, 0.1), &profile, 2.0, None);
        let r_high = compute(&mock_state(0.3, 0.8, 0.9), &profile, 2.0, None);
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
        let r_short = compute(&state, &short, 2.0, None);
        let r_long = compute(&state, &long, 2.0, None);
        assert!(r_long.dome_gain > r_short.dome_gain);
    }

    #[test]
    fn b_idle_positive_when_resources_available() {
        let state = mock_state(0.3, 0.8, 0.2);
        let profile = WorkloadProfile::from_name("es").unwrap();
        let result = compute(&state, &profile, 2.0, None);
        assert!(
            result.b_idle > 0.0,
            "B_idle must be positive with available resources"
        );
    }

    #[test]
    fn all_profiles_exist() {
        let profiles = WorkloadProfile::all();
        assert_eq!(profiles.len(), 50);
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
        let result = compute(&state, &profile, 0.0, None);
        assert!(
            (result.brake - 1.0).abs() < 1e-10,
            "brake must be 1.0 when κ=0"
        );
    }

    #[test]
    fn dimension_weights_consistent() {
        let state = mock_state(0.5, 0.7, 0.3);
        let profile = WorkloadProfile::from_name("gamer").unwrap();
        let result = compute(&state, &profile, 2.0, None);
        for (i, dw) in result.dimension_weights.iter().enumerate() {
            assert!(*dw >= 0.0, "dimension_weight[{}] must be ≥ 0", i);
        }
    }

    #[test]
    fn measured_efficiency_computes_true_u_over_p_and_e_over_u() {
        let eff = compute_measured_efficiency(MeasuredEfficiencyInput {
            useful_output: 1.0,
            quality_factor: Some(1.0),
            avg_power_watts: 100.0,
            duration_s: 10.0,
        })
        .expect("efficiency");

        assert!((eff.energy_kwh - (1000.0 / 3_600_000.0)).abs() < 1e-12);
        assert!((eff.utility_rate_per_s - 0.1).abs() < 1e-12);
        assert!((eff.utility_per_watt - 0.001).abs() < 1e-12);
        assert!((eff.kwh_per_utility - eff.energy_kwh).abs() < 1e-12);
    }

    #[test]
    fn measured_efficiency_comparison_is_positive_when_on_uses_less_energy() {
        let off = compute_measured_efficiency(MeasuredEfficiencyInput {
            useful_output: 1.0,
            quality_factor: Some(1.0),
            avg_power_watts: 120.0,
            duration_s: 10.0,
        })
        .expect("off");
        let on = compute_measured_efficiency(MeasuredEfficiencyInput {
            useful_output: 1.0,
            quality_factor: Some(1.0),
            avg_power_watts: 90.0,
            duration_s: 8.0,
        })
        .expect("on");

        let cmp = compare_measured_efficiency(off, on);
        assert!(cmp.gain_utility_per_watt_pct > 0.0);
        assert!(cmp.gain_kwh_per_utility_pct > 0.0);
        assert!(cmp.gain_watts_per_utility_rate_pct > 0.0);
    }
}
