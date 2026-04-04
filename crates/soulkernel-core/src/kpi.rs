//! KPI énergétique temps réel — W par unité de CPU utile.
//!
//! KPI(t)   = P(t) / CPU_utile(t)
//! KPI*(t)  = KPI(t) × (1 + λ × Faults / 10 000)
//! J(t)     = α × KPI + β × Faults_norm + γ × RAM_inactive_norm
//!
//! Le but : minimiser les watts par unité de calcul réellement utile.
//! Ni la RAM seule, ni le CPU brut — l'efficacité énergétique du travail fait.

use crate::device_profile::{DeviceProfile, ProcessRuleClass};
use crate::metrics::ResourceState;
use crate::processes::ProcessObservedReport;
use serde::{Deserialize, Serialize};

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum KpiLabel {
    Efficient,
    Moderate,
    Inefficient,
    #[default]
    Unknown, // pas de source de puissance
}

impl KpiLabel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Efficient => "EFFICACE",
            Self::Moderate => "MODÉRÉ",
            Self::Inefficient => "INEFFICACE",
            Self::Unknown => "—",
        }
    }
}

/// Classification d'un processus pour le calcul du CPU overhead.
/// Ne dépend pas de l'OS — basé sur des patterns de noms portables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessClass {
    /// Travail utile à l'utilisateur (jeu, compilation, rendu...).
    Useful,
    /// Overhead critique — sécurité OS. Peut être dômé mais pas tué.
    OverheadCritical,
    /// Overhead doux — navigateur background, mises à jour, index.
    OverheadSoft,
    /// Processus OS noyau. Ne compte ni comme utile ni comme overhead.
    SystemKernel,
    /// CPU < seuil minimal — considéré idle.
    Idle,
}

impl ProcessClass {
    pub fn is_overhead(self) -> bool {
        matches!(self, Self::OverheadCritical | Self::OverheadSoft)
    }
    pub fn is_system(self) -> bool {
        matches!(self, Self::SystemKernel)
    }
}

pub fn classify_by_name(profile: &DeviceProfile, raw_name: &str) -> Option<ProcessClass> {
    match profile.classify_process_name(raw_name) {
        Some(ProcessRuleClass::SystemKernel) => Some(ProcessClass::SystemKernel),
        Some(ProcessRuleClass::OverheadCritical) => Some(ProcessClass::OverheadCritical),
        Some(ProcessRuleClass::OverheadSoft) => Some(ProcessClass::OverheadSoft),
        None => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KpiSnapshot {
    /// P(t) / CPU_utile (W/%) — None si pas de source de puissance.
    pub kpi_basic: Option<f64>,
    /// KPI × (1 + λ × Faults/10000) — pénalisé thrashing mémoire.
    pub kpi_penalized: Option<f64>,
    /// Fonction objectif J(t) normalisée [0,1].
    pub objective_j: Option<f64>,

    pub cpu_total_pct: f64,
    /// CPU utile = somme des top-N processus utiles (bottom-up, ≥ ε).
    pub cpu_useful_pct: f64,
    /// CPU overhead critique (antivirus…) + doux (browser bg…) + self.
    pub cpu_overhead_pct: f64,
    /// CPU système noyau (svchost, dwm…) — exclu du calcul KPI.
    pub cpu_system_pct: f64,
    /// CPU du propre processus SoulKernel (per-core, peut dépasser 100%).
    pub cpu_self_pct: f64,
    /// True si SoulKernel lui-même consomme plus de 50% CPU (auto-sabotage).
    pub self_overload: bool,

    pub lambda: f64,
    pub label: KpiLabel,
    /// Δ KPI* par rapport à la mesure précédente (positif = dégradation).
    pub trend: Option<f64>,
}

impl KpiSnapshot {
    /// Vrai si le KPI est inefficace et mérite une action.
    /// Note: on n'agit PAS sur le trend seul (ex: MODÉRÉ + trend montant) pour éviter
    /// la boucle dome → page faults → rollback → repeat.
    pub fn should_act_with_profile(&self, _profile: &DeviceProfile) -> bool {
        matches!(self.label, KpiLabel::Inefficient)
    }
}

/// Calcule le KPI à partir des métriques live, du rapport processus et du profil appareil.
/// Le profil définit les seuils bottom-up (epsilon, top_n, min_pct) — universels par classe.
pub fn compute(
    metrics: &ResourceState,
    processes: &ProcessObservedReport,
    profile: &DeviceProfile,
    lambda: f64,
    alpha: f64,
    beta: f64,
    gamma: f64,
    prev_kpi_penalized: Option<f64>,
) -> KpiSnapshot {
    let power = metrics
        .raw
        .host_power_watts
        .or(metrics.raw.wall_power_watts)
        .or_else(|| {
            // Tente la source externe via wall_power_watts_source
            metrics.raw.wall_power_watts
        });

    let cpu_total = metrics.raw.cpu_pct;
    let mut cpu_overhead = 0.0f64;
    let mut cpu_system = 0.0f64;
    let mut cpu_self = 0.0f64;

    // Bottom-up : accumule le CPU des processus utiles (top-N, seuil ≥ CPU_USEFUL_MIN_PCT).
    // Robuste aux spikes d'overhead : on somme ce qui est utile au lieu de soustraire l'overhead
    // d'un total normalisé différemment (sysinfo : process = per-core, total = système).
    let mut cpu_useful_candidates: Vec<f64> = Vec::new();

    for proc_ in &processes.top_processes {
        if proc_.is_self_process || proc_.is_embedded_webview {
            cpu_self += proc_.cpu_usage_pct;
            continue;
        }
        match classify_by_name(profile, &proc_.name) {
            Some(ProcessClass::SystemKernel) => {
                cpu_system += proc_.cpu_usage_pct;
            }
            Some(c) if c.is_overhead() => {
                cpu_overhead += proc_.cpu_usage_pct;
            }
            _ => {
                // Utile ou Idle — candidat pour le calcul bottom-up.
                if proc_.cpu_usage_pct >= profile.cpu_useful_min_pct {
                    cpu_useful_candidates.push(proc_.cpu_usage_pct);
                }
            }
        }
    }
    // SoulKernel compte dans l'overhead, pas dans le travail utile.
    cpu_overhead += cpu_self;

    // CPU utile bottom-up : somme des top-N processus utiles identifiés.
    // Si aucun n'atteint le seuil (machine idle ou tout est overhead), repli sur
    // cpu_total avec un plancher ε pour éviter KPI absurde.
    cpu_useful_candidates.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let cpu_useful_bottomup: f64 = cpu_useful_candidates
        .iter()
        .take(profile.cpu_useful_top_n)
        .sum();

    let cpu_useful = if cpu_useful_bottomup > profile.cpu_useful_epsilon {
        cpu_useful_bottomup
    } else {
        // Repli : machine idle ou overhead total. On utilise cpu_total comme proxy.
        cpu_total.max(profile.cpu_useful_epsilon)
    };

    let self_overload = cpu_self > profile.kpi_self_overload_pct;

    // KPI de base (W/%)
    let kpi_basic = power.map(|p| p / cpu_useful);

    // Pénalité faults mémoire
    let faults = metrics.raw.page_faults_per_sec.unwrap_or(0.0);
    let fault_penalty = 1.0 + lambda * (faults / profile.kpi_fault_penalty_divisor);
    let kpi_penalized = kpi_basic.map(|k| (k * fault_penalty).max(0.0));

    // J(t) normalisé [0, 1]
    // Chaque composante est ramenée à [0,1] avec des plafonds raisonnables.
    let objective_j = kpi_penalized.map(|kpi| {
        let kpi_norm = (kpi / profile.kpi_norm_max).clamp(0.0, 1.0);
        let faults_norm = (faults / profile.kpi_faults_norm_max).clamp(0.0, 1.0);
        let ram_inactive = if metrics.raw.mem_total_mb > 0 {
            1.0 - (metrics.raw.mem_used_mb as f64 / metrics.raw.mem_total_mb as f64)
        } else {
            0.0
        };
        (alpha * kpi_norm + beta * faults_norm + gamma * ram_inactive).clamp(0.0, 1.0)
    });

    let label = match kpi_penalized {
        None => KpiLabel::Unknown,
        Some(k) if k < profile.kpi_efficient_threshold => KpiLabel::Efficient,
        Some(k) if k < profile.kpi_moderate_threshold => KpiLabel::Moderate,
        Some(_) => KpiLabel::Inefficient,
    };

    let trend = match (prev_kpi_penalized, kpi_penalized) {
        (Some(prev), Some(curr)) => Some(curr - prev),
        _ => None,
    };

    KpiSnapshot {
        kpi_basic,
        kpi_penalized,
        objective_j,
        cpu_total_pct: cpu_total,
        cpu_useful_pct: cpu_useful,
        cpu_overhead_pct: cpu_overhead,
        cpu_system_pct: cpu_system,
        cpu_self_pct: cpu_self,
        self_overload,
        lambda,
        label,
        trend,
    }
}

/// Entrée de l'historique delta-KPI pour la boucle d'apprentissage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiActionRecord {
    pub ts_ms: u64,
    pub action: String,
    pub kpi_before: Option<f64>,
    pub kpi_after: Option<f64>,
    /// kpi_after - kpi_before (positif = l'action a dégradé, négatif = amélioré).
    pub delta_kpi: Option<f64>,
    /// Positif si l'action a amélioré le KPI.
    pub rewarded: bool,
}

impl KpiActionRecord {
    pub fn new(ts_ms: u64, action: impl Into<String>, kpi_before: Option<f64>) -> Self {
        Self {
            ts_ms,
            action: action.into(),
            kpi_before,
            kpi_after: None,
            delta_kpi: None,
            rewarded: false,
        }
    }

    /// Ferme l'enregistrement avec le KPI mesuré après l'action.
    pub fn close(&mut self, kpi_after: Option<f64>) {
        self.kpi_after = kpi_after;
        self.delta_kpi = match (self.kpi_before, kpi_after) {
            (Some(before), Some(after)) => Some(after - before),
            _ => None,
        };
        // Récompense si le KPI s'est amélioré (baissé) ou est resté stable.
        self.rewarded = self
            .delta_kpi
            .map(|d| d <= 0.0)
            .unwrap_or(false);
    }
}

/// Historique glissant — les N dernières actions avec leur impact sur le KPI.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KpiLearningMemory {
    pub records: Vec<KpiActionRecord>,
    /// Action en attente de fermeture (before capturé, after pas encore).
    pub pending: Option<KpiActionRecord>,
}

impl KpiLearningMemory {
    const MAX_RECORDS: usize = 50;

    /// Ouvre un enregistrement pour une action qui vient d'être déclenchée.
    pub fn open(&mut self, ts_ms: u64, action: &str, kpi_now: Option<f64>) {
        // Ferme l'éventuel pending précédent sans after (action abandonnée).
        if let Some(mut prev) = self.pending.take() {
            prev.close(None);
            self.push(prev);
        }
        self.pending = Some(KpiActionRecord::new(ts_ms, action, kpi_now));
    }

    /// Ferme le pending avec le KPI mesuré après stabilisation.
    pub fn close_pending(&mut self, kpi_after: Option<f64>) {
        if let Some(mut rec) = self.pending.take() {
            rec.close(kpi_after);
            self.push(rec);
        }
    }

    fn push(&mut self, rec: KpiActionRecord) {
        self.records.push(rec);
        if self.records.len() > Self::MAX_RECORDS {
            self.records.remove(0);
        }
    }

    /// Ratio de récompenses sur les N dernières actions.
    pub fn reward_ratio(&self) -> f64 {
        if self.records.is_empty() {
            return 0.5;
        }
        let rewarded = self.records.iter().filter(|r| r.rewarded).count();
        rewarded as f64 / self.records.len() as f64
    }

    /// KPI moyen observé après les actions récompensées.
    pub fn avg_kpi_gain(&self) -> Option<f64> {
        let gains: Vec<f64> = self
            .records
            .iter()
            .filter_map(|r| r.delta_kpi.filter(|&d| d < 0.0))
            .collect();
        if gains.is_empty() {
            None
        } else {
            Some(gains.iter().sum::<f64>() / gains.len() as f64)
        }
    }
}
