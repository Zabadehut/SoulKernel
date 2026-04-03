//! KPI énergétique temps réel — W par unité de CPU utile.
//!
//! KPI(t)   = P(t) / CPU_utile(t)
//! KPI*(t)  = KPI(t) × (1 + λ × Faults / 10 000)
//! J(t)     = α × KPI + β × Faults_norm + γ × RAM_inactive_norm
//!
//! Le but : minimiser les watts par unité de calcul réellement utile.
//! Ni la RAM seule, ni le CPU brut — l'efficacité énergétique du travail fait.

use crate::metrics::ResourceState;
use crate::processes::ProcessObservedReport;
use serde::{Deserialize, Serialize};

// ─── Seuils KPI ──────────────────────────────────────────────────────────────

/// En dessous : système efficace.
pub const KPI_EFFICIENT_THRESHOLD: f64 = 5.0; // W/%
/// En dessous : acceptable. Au-dessus : inefficace.
pub const KPI_MODERATE_THRESHOLD: f64 = 12.0; // W/%

/// Paramètre λ par défaut pour la pénalité de faults.
pub const LAMBDA_DEFAULT: f64 = 0.5;

/// Epsilon : plancher du CPU utile pour éviter /0 et KPI absurde.
/// 5 % correspond à un travail minimal réaliste sur un poste actif.
pub const CPU_USEFUL_EPSILON: f64 = 5.0;

/// Top-N processus utiles retenus pour le calcul bottom-up.
pub const CPU_USEFUL_TOP_N: usize = 5;

/// CPU utile minimum par processus pour être retenu dans le top-N.
pub const CPU_USEFUL_MIN_PCT: f64 = 2.0;

/// Poids par défaut pour J(t).
pub const ALPHA_DEFAULT: f64 = 0.60; // poids puissance/CPU
pub const BETA_DEFAULT: f64 = 0.25; // poids faults mémoire
pub const GAMMA_DEFAULT: f64 = 0.15; // poids RAM inactive

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

/// Classifie un processus par son nom (sans `.exe`, case-insensitive).
/// Aucun hardcoding OS-specific : patterns portables Windows/Linux/macOS.
pub fn classify_by_name(raw_name: &str) -> Option<ProcessClass> {
    let n = raw_name
        .trim_end_matches(".exe")
        .trim_end_matches(".app")
        .to_ascii_lowercase();
    let n = n.as_str();

    // ── Kernel / OS backbone ──────────────────────────────────────────────
    // Windows + Linux + macOS noyau — ni overhead ni utile
    if matches!(
        n,
        "system"
            | "registry"
            | "memory compression"
            | "memcompression"
            | "secure system"
            | "system interrupts"
            | "interruptions système"
            | "hal"
            | "dwm"
            | "csrss"
            | "wininit"
            | "winlogon"
            | "smss"
            | "services"
            | "lsass"
            | "lsaiso"
            | "fontdrvhost"
            | "sihost"
            | "ntoskrnl"
            | "idle"
            // Linux kernel threads
            | "kthreadd"
            | "kworker"
            | "ksoftirqd"
            | "rcu_sched"
            | "migration"
            // macOS launchd / kernel
            | "launchd"
            | "kernel_task"
    ) {
        return Some(ProcessClass::SystemKernel);
    }
    // svchost on Windows — système pur
    if n.starts_with("svchost") {
        return Some(ProcessClass::SystemKernel);
    }
    // Linux/macOS runtime brokers
    if matches!(n, "runtimebroker" | "backgroundtaskhost") {
        return Some(ProcessClass::SystemKernel);
    }

    // ── Overhead critique — sécurité / antivirus ──────────────────────────
    // Peut être dômé (IO priority ↓, affinité réduite), jamais tué.
    if matches!(
        n,
        "msmpeng"               // Windows Defender antimalware
            | "nissrv"          // Network Inspection Service
            | "mpdefendercoreservice"
            | "securityhealthservice"
            | "mpcmdrun"
            | "mpschdutil"
            | "antimalware service executable"
            // Linux equivalents
            | "clamd"
            | "freshclam"
            | "avahi-daemon"
            // macOS
            | "mdworker_shared"
            | "mds_stores"
            | "trustd"
    ) {
        return Some(ProcessClass::OverheadCritical);
    }

    // ── Overhead doux — browser background, indexation, sync ─────────────
    if n.starts_with("msedge") || n.starts_with("msedgewebview") {
        return Some(ProcessClass::OverheadSoft);
    }
    if n.starts_with("chrome") || n.starts_with("chromium") {
        return Some(ProcessClass::OverheadSoft);
    }
    if n.starts_with("firefox") {
        return Some(ProcessClass::OverheadSoft);
    }
    if n.contains("update") || n.contains("updater") || n.contains("autoupdate") {
        return Some(ProcessClass::OverheadSoft);
    }
    if matches!(
        n,
        "searchindexer"
            | "searchhost"
            | "wmiprvse"
            | "onedrive"
            | "onedrive.sync.service"
            | "teams"
            | "ms-teams"
            | "outlook"
            | "olk"
            // Linux
            | "tracker"
            | "tracker-miner-fs"
            | "zeitgeist-daemon"
            // macOS
            | "spotlight"
            | "mds"
    ) {
        return Some(ProcessClass::OverheadSoft);
    }

    // Aucune règle sur le nom → sera classifié par CPU dans compute()
    None
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
    /// Vrai si le KPI se dégrade (ou est déjà inefficace) et mérite une action.
    pub fn should_act(&self) -> bool {
        matches!(self.label, KpiLabel::Inefficient)
            || self.trend.map(|d| d > 1.0).unwrap_or(false)
    }
}

/// Calcule le KPI à partir des métriques live et du rapport processus.
pub fn compute(
    metrics: &ResourceState,
    processes: &ProcessObservedReport,
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
        match classify_by_name(&proc_.name) {
            Some(ProcessClass::SystemKernel) => {
                cpu_system += proc_.cpu_usage_pct;
            }
            Some(c) if c.is_overhead() => {
                cpu_overhead += proc_.cpu_usage_pct;
            }
            _ => {
                // Utile ou Idle — candidat pour le calcul bottom-up.
                if proc_.cpu_usage_pct >= CPU_USEFUL_MIN_PCT {
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
        .take(CPU_USEFUL_TOP_N)
        .sum();

    let cpu_useful = if cpu_useful_bottomup > CPU_USEFUL_EPSILON {
        cpu_useful_bottomup
    } else {
        // Repli : machine idle ou overhead total. On utilise cpu_total comme proxy.
        cpu_total.max(CPU_USEFUL_EPSILON)
    };

    let self_overload = cpu_self > 50.0;

    // KPI de base (W/%)
    let kpi_basic = power.map(|p| p / cpu_useful);

    // Pénalité faults mémoire
    let faults = metrics.raw.page_faults_per_sec.unwrap_or(0.0);
    let fault_penalty = 1.0 + lambda * (faults / 10_000.0);
    let kpi_penalized = kpi_basic.map(|k| (k * fault_penalty).max(0.0));

    // J(t) normalisé [0, 1]
    // Chaque composante est ramenée à [0,1] avec des plafonds raisonnables.
    let objective_j = kpi_penalized.map(|kpi| {
        let kpi_norm = (kpi / 20.0).clamp(0.0, 1.0); // 20 W/% = pire cas pratique
        let faults_norm = (faults / 30_000.0).clamp(0.0, 1.0);
        let ram_inactive = if metrics.raw.mem_total_mb > 0 {
            1.0 - (metrics.raw.mem_used_mb as f64 / metrics.raw.mem_total_mb as f64)
        } else {
            0.0
        };
        (alpha * kpi_norm + beta * faults_norm + gamma * ram_inactive).clamp(0.0, 1.0)
    });

    let label = match kpi_penalized {
        None => KpiLabel::Unknown,
        Some(k) if k < KPI_EFFICIENT_THRESHOLD => KpiLabel::Efficient,
        Some(k) if k < KPI_MODERATE_THRESHOLD => KpiLabel::Moderate,
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
