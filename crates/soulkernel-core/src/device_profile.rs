//! Profil appareil — définit ce qui est "normal", "dangereux" et ce qu'on peut couper.
//!
//! Architecture : noyau universel + profil adaptatif.
//! Le noyau (kpi::compute, orchestrator) est générique.
//! Le profil porte tout ce qui est spécifique à un type d'appareil.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessRuleClass {
    SystemKernel,
    OverheadCritical,
    OverheadSoft,
}

// ─── Classe d'appareil ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceClass {
    /// PC / laptop / workstation. Actions : dome + SoulRAM.
    Pc,
    /// Serveur (headless, plus de processus utiles, seuils différents).
    Server,
    /// Télévision / box multimédia. Actions : coupe veille.
    Tv,
    /// Appareil critique (frigo, médical, avion). Monitor only — jamais de coupure.
    CriticalAppliance,
    /// Profil personnalisé.
    Custom,
}

impl DeviceClass {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pc => "PC / Laptop",
            Self::Server => "Serveur",
            Self::Tv => "TV / Box",
            Self::CriticalAppliance => "Appareil critique",
            Self::Custom => "Personnalisé",
        }
    }
}

// ─── Profil ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProfile {
    pub id: &'static str,
    pub label: &'static str,
    pub device_class: DeviceClass,

    /// SoulKernel peut prendre des actions (dome, trim).
    /// False = monitoring uniquement — jamais de coupure.
    pub can_act: bool,

    // ── Paramètres KPI bottom-up ─────────────────────────────────────────
    /// Plancher CPU_utile (%) pour éviter KPI absurde sur spike d'overhead.
    pub cpu_useful_epsilon: f64,
    /// Nombre max de processus utiles retenus pour la somme bottom-up.
    pub cpu_useful_top_n: usize,
    /// CPU minimum (%) pour qu'un processus compte comme "utile".
    pub cpu_useful_min_pct: f64,

    // ── Seuils KPI / décision ────────────────────────────────────────────
    pub kpi_efficient_threshold: f64,
    pub kpi_moderate_threshold: f64,
    pub kpi_lambda_default: f64,
    pub kpi_alpha: f64,
    pub kpi_beta: f64,
    pub kpi_gamma: f64,
    pub kpi_fault_penalty_divisor: f64,
    pub kpi_faults_norm_max: f64,
    pub kpi_norm_max: f64,
    pub kpi_self_overload_pct: f64,
    pub kpi_trend_degrade_threshold: f64,
    pub kpi_post_action_rollback_ratio: f64,

    // ── Automatisation ───────────────────────────────────────────────────
    pub auto_dome_cooldown_s: u64,
    pub auto_dome_guard_min: f64,
    pub auto_dome_rollback_trend_max: f64,
    pub soulram_idle_sigma_min: f64,
    pub soulram_burst_cooldown_s: u64,
    pub soulram_sustain_cooldown_s: u64,

    // ── Polling / refresh ────────────────────────────────────────────────
    pub lite_refresh_min_s: u64,
    pub process_refresh_s: u64,
    pub inventory_refresh_s: u64,

    // ── Baselines puissance (optionnel — None = appris depuis l'historique) ─
    pub p_idle_hint_w: Option<f64>,
    pub p_active_hint_w: Option<f64>,
}

impl DeviceProfile {
    // ── Presets ──────────────────────────────────────────────────────────────

    pub fn pc() -> Self {
        Self {
            id: "pc",
            label: "PC / Laptop",
            device_class: DeviceClass::Pc,
            can_act: true,
            cpu_useful_epsilon: 5.0,
            cpu_useful_top_n: 5,
            cpu_useful_min_pct: 2.0,
            kpi_efficient_threshold: 5.0,
            kpi_moderate_threshold: 12.0,
            kpi_lambda_default: 0.5,
            kpi_alpha: 0.60,
            kpi_beta: 0.25,
            kpi_gamma: 0.15,
            kpi_fault_penalty_divisor: 10_000.0,
            kpi_faults_norm_max: 30_000.0,
            kpi_norm_max: 20.0,
            kpi_self_overload_pct: 50.0,
            kpi_trend_degrade_threshold: 1.0,
            kpi_post_action_rollback_ratio: 1.20,
            auto_dome_cooldown_s: 30,
            auto_dome_guard_min: 0.85,
            auto_dome_rollback_trend_max: 0.0,
            soulram_idle_sigma_min: 0.30,
            soulram_burst_cooldown_s: 180,
            soulram_sustain_cooldown_s: 900,
            lite_refresh_min_s: 5,
            process_refresh_s: 20,
            inventory_refresh_s: 60,
            p_idle_hint_w: None,
            p_active_hint_w: None,
        }
    }

    pub fn server() -> Self {
        Self {
            id: "server",
            label: "Serveur",
            device_class: DeviceClass::Server,
            can_act: true,
            // Serveur : plus de processus utiles simultanés, seuils plus hauts.
            cpu_useful_epsilon: 10.0,
            cpu_useful_top_n: 10,
            cpu_useful_min_pct: 1.0,
            kpi_efficient_threshold: 6.0,
            kpi_moderate_threshold: 14.0,
            kpi_lambda_default: 0.35,
            kpi_alpha: 0.65,
            kpi_beta: 0.20,
            kpi_gamma: 0.15,
            kpi_fault_penalty_divisor: 20_000.0,
            kpi_faults_norm_max: 60_000.0,
            kpi_norm_max: 25.0,
            kpi_self_overload_pct: 70.0,
            kpi_trend_degrade_threshold: 1.5,
            kpi_post_action_rollback_ratio: 1.25,
            auto_dome_cooldown_s: 45,
            auto_dome_guard_min: 0.80,
            auto_dome_rollback_trend_max: 0.0,
            soulram_idle_sigma_min: 0.20,
            soulram_burst_cooldown_s: 180,
            soulram_sustain_cooldown_s: 900,
            lite_refresh_min_s: 5,
            process_refresh_s: 15,
            inventory_refresh_s: 120,
            p_idle_hint_w: None,
            p_active_hint_w: None,
        }
    }

    pub fn tv() -> Self {
        Self {
            id: "tv",
            label: "TV / Box",
            device_class: DeviceClass::Tv,
            can_act: true,
            // TV : peu de processus, seuil de CPU utile plus haut.
            cpu_useful_epsilon: 5.0,
            cpu_useful_top_n: 3,
            cpu_useful_min_pct: 5.0,
            kpi_efficient_threshold: 4.0,
            kpi_moderate_threshold: 10.0,
            kpi_lambda_default: 0.25,
            kpi_alpha: 0.70,
            kpi_beta: 0.10,
            kpi_gamma: 0.20,
            kpi_fault_penalty_divisor: 15_000.0,
            kpi_faults_norm_max: 20_000.0,
            kpi_norm_max: 15.0,
            kpi_self_overload_pct: 40.0,
            kpi_trend_degrade_threshold: 0.7,
            kpi_post_action_rollback_ratio: 1.15,
            auto_dome_cooldown_s: 20,
            auto_dome_guard_min: 0.90,
            auto_dome_rollback_trend_max: 0.0,
            soulram_idle_sigma_min: 0.35,
            soulram_burst_cooldown_s: 180,
            soulram_sustain_cooldown_s: 900,
            lite_refresh_min_s: 5,
            process_refresh_s: 30,
            inventory_refresh_s: 120,
            p_idle_hint_w: Some(8.0),
            p_active_hint_w: Some(80.0),
        }
    }

    /// Appareil critique : monitoring uniquement, aucune action.
    pub fn monitor_only() -> Self {
        Self {
            id: "monitor_only",
            label: "Monitoring seul",
            device_class: DeviceClass::CriticalAppliance,
            can_act: false,
            cpu_useful_epsilon: 5.0,
            cpu_useful_top_n: 5,
            cpu_useful_min_pct: 2.0,
            kpi_efficient_threshold: 5.0,
            kpi_moderate_threshold: 12.0,
            kpi_lambda_default: 0.5,
            kpi_alpha: 0.60,
            kpi_beta: 0.25,
            kpi_gamma: 0.15,
            kpi_fault_penalty_divisor: 10_000.0,
            kpi_faults_norm_max: 30_000.0,
            kpi_norm_max: 20.0,
            kpi_self_overload_pct: 50.0,
            kpi_trend_degrade_threshold: 1.0,
            kpi_post_action_rollback_ratio: 1.20,
            auto_dome_cooldown_s: 30,
            auto_dome_guard_min: 0.85,
            auto_dome_rollback_trend_max: 0.0,
            soulram_idle_sigma_min: 0.30,
            soulram_burst_cooldown_s: 180,
            soulram_sustain_cooldown_s: 900,
            lite_refresh_min_s: 5,
            process_refresh_s: 20,
            inventory_refresh_s: 60,
            p_idle_hint_w: None,
            p_active_hint_w: None,
        }
    }

    /// Liste complète pour le sélecteur UI.
    pub fn list_all() -> Vec<Self> {
        vec![
            Self::pc(),
            Self::server(),
            Self::tv(),
            Self::monitor_only(),
        ]
    }

    pub fn classify_process_name(&self, raw_name: &str) -> Option<ProcessRuleClass> {
        let n = raw_name
            .trim_end_matches(".exe")
            .trim_end_matches(".app")
            .to_ascii_lowercase();
        let n = n.as_str();

        if self.system_kernel_exact().contains(&n)
            || self.system_kernel_prefixes().iter().any(|p| n.starts_with(p))
            || self.system_kernel_contains().iter().all(|needle| n.contains(needle))
        {
            return Some(ProcessRuleClass::SystemKernel);
        }
        if self.overhead_critical_exact().contains(&n)
            || self.overhead_critical_prefixes().iter().any(|p| n.starts_with(p))
        {
            return Some(ProcessRuleClass::OverheadCritical);
        }
        if self.overhead_soft_exact().contains(&n)
            || self.overhead_soft_prefixes().iter().any(|p| n.starts_with(p))
            || self.overhead_soft_contains().iter().any(|needle| n.contains(needle))
        {
            return Some(ProcessRuleClass::OverheadSoft);
        }
        None
    }

    fn system_kernel_exact(&self) -> &'static [&'static str] {
        &[
            "system", "registry", "memory compression", "memcompression", "secure system",
            "system interrupts", "interruptions système", "hal", "dwm", "csrss", "wininit",
            "winlogon", "smss", "services", "lsass", "lsaiso", "fontdrvhost", "sihost",
            "ntoskrnl", "idle", "kthreadd", "kworker", "ksoftirqd", "rcu_sched",
            "migration", "launchd", "kernel_task", "runtimebroker", "backgroundtaskhost",
        ]
    }

    fn system_kernel_prefixes(&self) -> &'static [&'static str] {
        &["svchost"]
    }

    fn system_kernel_contains(&self) -> &'static [&'static str] {
        &["webkit", "gpu"]
    }

    fn overhead_critical_exact(&self) -> &'static [&'static str] {
        &[
            "msmpeng", "nissrv", "mpdefendercoreservice", "securityhealthservice",
            "mpcmdrun", "mpschdutil", "antimalware service executable", "clamd",
            "freshclam", "avahi-daemon", "mdworker_shared", "mds_stores", "trustd",
        ]
    }

    fn overhead_critical_prefixes(&self) -> &'static [&'static str] {
        &[]
    }

    fn overhead_soft_exact(&self) -> &'static [&'static str] {
        &[
            "searchindexer", "searchhost", "wmiprvse", "onedrive",
            "onedrive.sync.service", "teams", "ms-teams", "outlook", "olk",
            "tracker", "tracker-miner-fs", "zeitgeist-daemon", "spotlight", "mds",
        ]
    }

    fn overhead_soft_prefixes(&self) -> &'static [&'static str] {
        &["msedge", "msedgewebview", "chrome", "chromium", "firefox"]
    }

    fn overhead_soft_contains(&self) -> &'static [&'static str] {
        &["update", "updater", "autoupdate"]
    }
}

impl Default for DeviceProfile {
    fn default() -> Self {
        Self::pc()
    }
}
