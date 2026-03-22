//! Moteur de décision mémoire adaptatif (anti-thrashing, Burst/Sustain, scoring π).
//!
//! Ne scanne pas la RAM page par page : hystérésis sur la pression, cooldowns sur les
//! mutations agressives (working set, trim global, zRAM, drop_caches), et ajustement
//! léger du facteur mémoire dans π pour refléter le risque de coût différé.

use crate::formula::WorkloadProfile;
use crate::metrics::ResourceState;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// ─── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryDomeMode {
    /// Actions fortes plus fréquentes si la pression est confirmée.
    Burst,
    /// Cooldowns plus longs, seuils d’hystérésis plus prudents.
    Sustain,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryActivationPlan {
    pub apply_working_set: bool,
    pub apply_disable_compression: bool,
    pub apply_zram_resize: bool,
    pub apply_drop_caches: bool,
    pub notes: Vec<String>,
}

// ─── Constants ─────────────────────────────────────────────────────────────────

const PRESSURE_ENTER: f64 = 0.78; // 1 - mem disponible (approx via 1 - state.mem)
const PRESSURE_EXIT: f64 = 0.68;

const SUSTAIN_PRESSURE_SECS: u64 = 8;
const BURST_PRESSURE_SECS: u64 = 2;

const WS_COOLDOWN_BURST: Duration = Duration::from_secs(120);
const WS_COOLDOWN_SUSTAIN: Duration = Duration::from_secs(360);

const GLOBAL_TRIM_COOLDOWN_BURST: Duration = Duration::from_secs(180);
const GLOBAL_TRIM_COOLDOWN_SUSTAIN: Duration = Duration::from_secs(900);

const COMPRESSION_TOGGLE_COOLDOWN: Duration = Duration::from_secs(120);

const LINUX_AGGR_COOLDOWN_BURST: Duration = Duration::from_secs(90);
const LINUX_AGGR_COOLDOWN_SUSTAIN: Duration = Duration::from_secs(300);

/// Au-delà de ce seuil (PDH Windows), on augmente la prudence π / garde-fous.
const PAGE_FAULTS_HEAVY: f64 = 8000.0;

// ─── Internal state ───────────────────────────────────────────────────────────

struct MemoryPolicyState {
    high_pressure_since: Option<Instant>,
    last_ws_adjust: Option<Instant>,
    last_global_trim: Option<Instant>,
    last_compression_toggle: Option<Instant>,
    last_linux_aggressive: Option<Instant>,
}

impl Default for MemoryPolicyState {
    fn default() -> Self {
        Self {
            high_pressure_since: None,
            last_ws_adjust: None,
            last_global_trim: None,
            last_compression_toggle: None,
            last_linux_aggressive: None,
        }
    }
}

fn global_state() -> &'static Mutex<MemoryPolicyState> {
    static S: OnceLock<Mutex<MemoryPolicyState>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(MemoryPolicyState::default()))
}

pub fn dome_mode_for_profile(profile: &WorkloadProfile) -> MemoryDomeMode {
    match profile.name.as_str() {
        "gamer" | "es" => MemoryDomeMode::Burst,
        _ => MemoryDomeMode::Sustain,
    }
}

fn pressure_required(mode: MemoryDomeMode) -> Duration {
    match mode {
        MemoryDomeMode::Burst => Duration::from_secs(BURST_PRESSURE_SECS),
        MemoryDomeMode::Sustain => Duration::from_secs(SUSTAIN_PRESSURE_SECS),
    }
}

fn ws_cooldown(mode: MemoryDomeMode) -> Duration {
    match mode {
        MemoryDomeMode::Burst => WS_COOLDOWN_BURST,
        MemoryDomeMode::Sustain => WS_COOLDOWN_SUSTAIN,
    }
}

fn global_trim_cooldown(mode: MemoryDomeMode) -> Duration {
    match mode {
        MemoryDomeMode::Burst => GLOBAL_TRIM_COOLDOWN_BURST,
        MemoryDomeMode::Sustain => GLOBAL_TRIM_COOLDOWN_SUSTAIN,
    }
}

fn linux_aggr_cooldown(mode: MemoryDomeMode) -> Duration {
    match mode {
        MemoryDomeMode::Burst => LINUX_AGGR_COOLDOWN_BURST,
        MemoryDomeMode::Sustain => LINUX_AGGR_COOLDOWN_SUSTAIN,
    }
}

/// Met à jour l’hystérésis pression mémoire ; appeler à chaque collecte métrique.
/// Retourne un facteur [0,1] : 0 = pas de garde-fou supplémentaire, 1 = prudence maximale pour π.
pub fn tick_from_baseline(baseline: &ResourceState) -> f64 {
    let pressure = memory_pressure(baseline);
    let now = Instant::now();
    let mut g = match global_state().lock() {
        Ok(x) => x,
        Err(_) => return 0.0,
    };

    if pressure >= PRESSURE_ENTER {
        if g.high_pressure_since.is_none() {
            g.high_pressure_since = Some(now);
        }
    } else if pressure <= PRESSURE_EXIT {
        g.high_pressure_since = None;
    }

    let pf = baseline.raw.page_faults_per_sec.unwrap_or(0.0);
    let pf_stress = if pf.is_finite() && pf >= PAGE_FAULTS_HEAVY {
        ((pf - PAGE_FAULTS_HEAVY) / PAGE_FAULTS_HEAVY).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let sigma_stress = baseline.sigma.clamp(0.0, 1.0);
    let mem_low = (1.0 - baseline.mem).clamp(0.0, 1.0);

    (0.45 * pf_stress + 0.35 * sigma_stress + 0.2 * mem_low).clamp(0.0, 1.0)
}

fn memory_pressure(s: &ResourceState) -> f64 {
    (1.0 - s.mem).clamp(0.0, 1.0)
}

fn sustained_pressure() -> Option<Duration> {
    let g = global_state().lock().ok()?;
    g.high_pressure_since
        .map(|t| t.elapsed())
}

fn cooldown_blocks(last: Option<Instant>, cooldown: Duration) -> bool {
    last.map(|t| t.elapsed() < cooldown).unwrap_or(false)
}

/// Plan d’actions mémoire pour une activation dôme (après `tick_from_baseline` sur le même état).
pub fn plan_for_dome_activation(baseline: &ResourceState, profile: &WorkloadProfile) -> MemoryActivationPlan {
    let mode = dome_mode_for_profile(profile);
    let sustained = sustained_pressure();
    let need = pressure_required(mode);
    let pressure_ok = sustained.map(|d| d >= need).unwrap_or(false);
    let mem_tight = baseline.mem < 0.22;
    let sigma_high = baseline.sigma > 0.52;

    let mut plan = MemoryActivationPlan::default();
    let g = match global_state().lock() {
        Ok(x) => x,
        Err(_) => {
            plan.notes.push("MemoryPolicy: état interne indisponible".into());
            return plan;
        }
    };

    // Working set (Windows) — cooldown anti-thrashing
    if profile.alpha[1] > 0.2 {
        if cooldown_blocks(g.last_ws_adjust, ws_cooldown(mode)) {
            plan.apply_working_set = false;
            plan
                .notes
                .push("MemoryPolicy: working set différé (cooldown anti-thrashing)".into());
        } else {
            plan.apply_working_set = true;
        }
    } else {
        plan.apply_working_set = false;
    }

    // Compression off (Windows) — adaptatif : pression soutenue + pas de “compression utile” forte
    if profile.alpha[3] > 0.4 {
        let comp = baseline.compression.unwrap_or(0.0);
        let compression_helpful = comp > 0.28 && baseline.mem > 0.14;
        let allow_by_pressure = pressure_ok && (mem_tight || sigma_high);
        let toggle_cooldown = cooldown_blocks(g.last_compression_toggle, COMPRESSION_TOGGLE_COOLDOWN);

        if compression_helpful && !mem_tight {
            plan.apply_disable_compression = false;
            plan.notes.push(
                "MemoryPolicy: compression conservée (ratio élevé, RAM non critique)".into(),
            );
        } else if !allow_by_pressure {
            plan.apply_disable_compression = false;
            plan
                .notes
                .push("MemoryPolicy: disable compression différé (pression non soutenue)".into());
        } else if toggle_cooldown {
            plan.apply_disable_compression = false;
            plan
                .notes
                .push("MemoryPolicy: compression toggle en cooldown".into());
        } else {
            plan.apply_disable_compression = true;
        }
    }

    // Linux zRAM resize
    if profile.alpha[2] > 0.1 {
        if cooldown_blocks(g.last_linux_aggressive, linux_aggr_cooldown(mode)) {
            plan.apply_zram_resize = false;
            plan
                .notes
                .push("MemoryPolicy: zRAM resize différé (cooldown)".into());
        } else {
            plan.apply_zram_resize = true;
        }
    }

    // Linux drop_caches — plus strict : pression mémoire réelle
    if profile.alpha[3] > 0.5 {
        if !pressure_ok || baseline.mem > 0.35 {
            plan.apply_drop_caches = false;
            plan
                .notes
                .push("MemoryPolicy: drop_caches ignoré (pression insuffisante ou RAM OK)".into());
        } else if cooldown_blocks(g.last_linux_aggressive, linux_aggr_cooldown(mode)) {
            plan.apply_drop_caches = false;
            plan
                .notes
                .push("MemoryPolicy: drop_caches différé (cooldown)".into());
        } else {
            plan.apply_drop_caches = true;
        }
    }

    plan
}

pub fn record_working_set_adjustment() {
    if let Ok(mut g) = global_state().lock() {
        g.last_ws_adjust = Some(Instant::now());
    }
}

pub fn record_global_working_set_trim() {
    if let Ok(mut g) = global_state().lock() {
        g.last_global_trim = Some(Instant::now());
        g.last_ws_adjust = Some(Instant::now());
    }
}

pub fn record_compression_toggle() {
    if let Ok(mut g) = global_state().lock() {
        g.last_compression_toggle = Some(Instant::now());
    }
}

pub fn record_linux_aggressive_memory() {
    if let Ok(mut g) = global_state().lock() {
        g.last_linux_aggressive = Some(Instant::now());
    }
}

/// SoulRAM trim global — respecte un cooldown plus long que le dôme seul.
pub fn allow_global_trim(profile_hint: Option<&WorkloadProfile>) -> (bool, Vec<String>) {
    let mode = profile_hint
        .map(dome_mode_for_profile)
        .unwrap_or(MemoryDomeMode::Sustain);
    let mut notes = Vec::new();
    let g = match global_state().lock() {
        Ok(x) => x,
        Err(_) => return (false, vec!["MemoryPolicy: état interne indisponible".into()]),
    };
    let cd = global_trim_cooldown(mode);
    if cooldown_blocks(g.last_global_trim, cd) {
        notes.push(format!(
            "MemoryPolicy: trim global SoulRAM en cooldown ({:?}, mode {:?})",
            cd, mode
        ));
        (false, notes)
    } else {
        (true, notes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::RawMetrics;

    fn state(mem: f64, sigma: f64) -> ResourceState {
        ResourceState {
            cpu: 0.3,
            mem,
            compression: Some(0.1),
            io_bandwidth: Some(0.2),
            gpu: None,
            sigma,
            epsilon: [0.05; 5],
            raw: RawMetrics {
                cpu_pct: 30.0,
                cpu_clock_mhz: None,
                mem_used_mb: 8000,
                mem_total_mb: 16000,
                ram_clock_mhz: None,
                swap_used_mb: 0,
                swap_total_mb: 0,
                zram_used_mb: None,
                io_read_mb_s: None,
                io_write_mb_s: None,
                gpu_pct: None,
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

    fn compile_profile() -> WorkloadProfile {
        WorkloadProfile::from_name("compile").expect("compile")
    }

    #[test]
    fn dome_mode_gamer_burst() {
        let g = WorkloadProfile::from_name("gamer").unwrap();
        assert_eq!(dome_mode_for_profile(&g), MemoryDomeMode::Burst);
    }

    #[test]
    fn dome_mode_compile_sustain() {
        let p = compile_profile();
        assert_eq!(dome_mode_for_profile(&p), MemoryDomeMode::Sustain);
    }

    #[test]
    fn ws_second_activation_blocked_by_cooldown() {
        *global_state().lock().unwrap() = MemoryPolicyState::default();

        let baseline = state(0.5, 0.3);
        let _ = tick_from_baseline(&baseline);
        let mut p = compile_profile();
        p.alpha[1] = 0.25;

        let plan1 = plan_for_dome_activation(&baseline, &p);
        assert!(plan1.apply_working_set);
        record_working_set_adjustment();

        let plan2 = plan_for_dome_activation(&baseline, &p);
        assert!(!plan2.apply_working_set);
    }

    #[test]
    fn compression_preserved_when_helpful() {
        *global_state().lock().unwrap() = MemoryPolicyState::default();

        let mut baseline = state(0.5, 0.8);
        baseline.compression = Some(0.45);
        let _ = tick_from_baseline(&baseline);

        let p = WorkloadProfile::from_name("sqlite").expect("sqlite I/O-heavy profile");

        let plan = plan_for_dome_activation(&baseline, &p);
        assert!(
            !plan.apply_disable_compression,
            "should keep compression when ratio high and RAM not critical"
        );
    }
}
