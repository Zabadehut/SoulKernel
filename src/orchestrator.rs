//! orchestrator.rs — Performance Dome activation & rollback
//!
//! Translates the optimal profile P* into real kernel writes.
//! Gradient ascent: r(t+Δt) = r(t) + η · ∇_r π(t) · 1_{Σ < Σmax}

use crate::{formula::WorkloadProfile, metrics::ResourceState, platform};
use anyhow::Result;
use serde::{Deserialize, Serialize};

// ─── Result types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomeResult {
    pub activated: bool,
    pub pi: f64,
    pub dome_gain: f64,
    pub b_idle: f64,
    pub message: String,
    /// List of actual kernel actions taken (for the log panel)
    pub actions: Vec<String>,
    /// Number of kernel actions that succeeded.
    pub actions_ok: usize,
    /// Total number of kernel actions attempted.
    pub actions_total: usize,
}

// ─── Activation ───────────────────────────────────────────────────────────────

pub async fn activate(
    profile: &WorkloadProfile,
    eta: f64,
    baseline: &ResourceState,
    policy: crate::platform::PolicyMode,
    target_pid: Option<u32>,
) -> Result<DomeResult> {
    let mut actions: Vec<String> = Vec::new();

    let f = crate::formula::compute(baseline, profile, 2.0);

    let results = platform::apply_dome_profile(profile, eta, baseline, policy, target_pid).await;

    let mut ok_count = 0usize;
    let total_count = results.len();
    for (action, ok) in results {
        if ok {
            ok_count += 1;
            actions.push(format!("✓ {}", action));
        } else {
            actions.push(format!("✗ {}", action));
        }
    }

    let msg = if ok_count == 0 && total_count > 0 {
        format!("Dome actif (toutes actions refusées) · π={:.4}", f.pi)
    } else if f.rentable {
        format!("Dome ACTIVÉ · π={:.4} · 𝒟={:.4}", f.pi, f.dome_gain)
    } else {
        format!("Dome actif (gain marginal) · π={:.4}", f.pi)
    };

    Ok(DomeResult {
        activated: true,
        pi: f.pi,
        dome_gain: f.dome_gain,
        b_idle: f.b_idle,
        message: msg,
        actions,
        actions_ok: ok_count,
        actions_total: total_count,
    })
}

// ─── Rollback ────────────────────────────────────────────────────────────────

pub async fn rollback(
    snapshot: Option<ResourceState>,
    target_pid: Option<u32>,
) -> Result<Vec<String>> {
    let actions = platform::rollback_dome_profile(snapshot, target_pid).await;
    Ok(actions
        .into_iter()
        .map(|(a, ok)| {
            if ok {
                format!("✓ {}", a)
            } else {
                format!("✗ {}", a)
            }
        })
        .collect())
}
