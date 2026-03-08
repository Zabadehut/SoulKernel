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
