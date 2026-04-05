use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

/// Machine activity state — used to exclude idle/media periods from dome gain accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MachineActivity {
    /// User is actively working (compilation, DB, game, etc.)
    Active,
    /// Machine is mostly idle (low CPU, low I/O, low GPU)
    Idle,
    /// Passive media consumption (video/film — GPU busy, CPU low)
    Media,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergyPricing {
    pub currency: String,
    pub price_per_kwh: f64,
    pub co2_kg_per_kwh: f64,
}

impl Default for EnergyPricing {
    fn default() -> Self {
        Self {
            currency: "EUR".to_string(),
            price_per_kwh: 0.22,
            co2_kg_per_kwh: 0.05,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetryIngestRequest {
    pub ts_ms: Option<u64>,
    pub power_watts: Option<f64>,
    pub dome_active: bool,
    pub soulram_active: bool,
    pub kpi_gain_median_pct: Option<f64>,
    /// CPU usage percentage at this tick (from sysinfo).
    #[serde(default)]
    pub cpu_pct: Option<f64>,
    /// Instantaneous π(t) computed by the frontend at this tick.
    #[serde(default)]
    pub pi: Option<f64>,
    /// Machine activity state detected by the frontend.
    #[serde(default)]
    pub machine_activity: Option<MachineActivity>,
    /// RAM utilisée (Mo) — pour cumul GREEN IT « RAM·GB·h ».
    #[serde(default)]
    pub mem_used_mb: Option<f64>,
    /// RAM physique totale (Mo).
    #[serde(default)]
    pub mem_total_mb: Option<f64>,
    /// `meross_wall`, `rapl`, etc. — pour libellé source énergie.
    #[serde(default)]
    pub power_source_tag: Option<String>,
    #[serde(default)]
    pub io_read_mb_s: Option<f64>,
    #[serde(default)]
    pub io_write_mb_s: Option<f64>,
    #[serde(default)]
    pub gpu_pct: Option<f64>,
    #[serde(default)]
    pub gpu_power_watts: Option<f64>,
    #[serde(default)]
    pub gpu_temp_c: Option<f64>,
    #[serde(default)]
    pub cpu_temp_c: Option<f64>,
    #[serde(default)]
    pub zram_used_mb: Option<u64>,
    #[serde(default)]
    pub psi_cpu: Option<f64>,
    #[serde(default)]
    pub psi_mem: Option<f64>,
    #[serde(default)]
    pub load_avg_1m_norm: Option<f64>,
    #[serde(default)]
    pub runnable_tasks: Option<u64>,
    #[serde(default)]
    pub on_battery: Option<bool>,
    #[serde(default)]
    pub battery_percent: Option<f64>,
    #[serde(default)]
    pub page_faults_per_sec: Option<f64>,
    #[serde(default)]
    pub webview_host_cpu_sum: Option<f64>,
    #[serde(default)]
    pub webview_host_mem_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySample {
    pub ts_ms: u64,
    pub dt_s: f64,
    pub power_watts: Option<f64>,
    pub dome_active: bool,
    pub soulram_active: bool,
    pub kpi_gain_median_pct: Option<f64>,
    /// CPU % at this tick — used for CPU·h differential when no power meter.
    #[serde(default)]
    pub cpu_pct: Option<f64>,
    /// π(t) at this tick — used for real dome gain integral.
    #[serde(default)]
    pub pi: Option<f64>,
    /// Machine activity state at this tick.
    #[serde(default)]
    pub machine_activity: Option<MachineActivity>,
    /// Ratio RAM utilisée (0..1), dérivé de used/total à l’ingest.
    #[serde(default)]
    pub mem_used_ratio: Option<f64>,
    #[serde(default)]
    pub mem_total_mb: Option<f64>,
    /// Rempli à l’ingest (ex. `meross_wall`, `rapl`).
    #[serde(default)]
    pub power_source_tag: Option<String>,
    #[serde(default)]
    pub io_read_mb_s: Option<f64>,
    #[serde(default)]
    pub io_write_mb_s: Option<f64>,
    #[serde(default)]
    pub gpu_pct: Option<f64>,
    #[serde(default)]
    pub gpu_power_watts: Option<f64>,
    #[serde(default)]
    pub gpu_temp_c: Option<f64>,
    #[serde(default)]
    pub cpu_temp_c: Option<f64>,
    #[serde(default)]
    pub zram_used_mb: Option<u64>,
    #[serde(default)]
    pub psi_cpu: Option<f64>,
    #[serde(default)]
    pub psi_mem: Option<f64>,
    #[serde(default)]
    pub load_avg_1m_norm: Option<f64>,
    #[serde(default)]
    pub runnable_tasks: Option<u64>,
    #[serde(default)]
    pub on_battery: Option<bool>,
    #[serde(default)]
    pub battery_percent: Option<f64>,
    #[serde(default)]
    pub page_faults_per_sec: Option<f64>,
    #[serde(default)]
    pub webview_host_cpu_sum: Option<f64>,
    #[serde(default)]
    pub webview_host_mem_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WindowSummary {
    pub samples: usize,
    pub duration_h: f64,
    pub avg_power_w: Option<f64>,
    /// Average machine power while dome is active (W).
    #[serde(default)]
    pub avg_power_dome_on_w: Option<f64>,
    /// Average machine power while dome is inactive (W).
    #[serde(default)]
    pub avg_power_dome_off_w: Option<f64>,
    /// Energy delta saved by dome: (avg_off − avg_on) × dome_on_duration / 3_600_000 (kWh).
    #[serde(default)]
    pub energy_saved_kwh: Option<f64>,
    pub has_power_data: bool,
    pub energy_kwh: f64,
    pub cost: f64,
    pub co2_kg: f64,
    pub dome_active_ratio: f64,
    pub passive_clean_h: f64,
    pub kpi_gain_median_pct: Option<f64>,
    /// CPU·h différentielles = baseline CPU dôme OFF vs mesure dôme ON, entrées `cpu_pct` réelles.
    #[serde(rename = "cpu_hours_differential", alias = "cpu_hours_saved", default)]
    pub cpu_hours_differential: f64,
    /// Real dome gain integral Σ(π_i × dt_i) for dome-active samples.
    pub dome_gain_integral: f64,
    /// ∫ max(0, ratio_baseline − ratio_dome) × (total_GB) dt / 3600 — équivalent « gigaoctet-heures » de pression RAM évitée.
    #[serde(
        rename = "mem_gb_hours_differential",
        alias = "mem_gb_hours_saved",
        default
    )]
    pub mem_gb_hours_differential: f64,
    /// Ratio of idle samples in the window.
    pub idle_ratio: f64,
    /// Ratio of media samples in the window.
    pub media_ratio: f64,
}

/// Cumulative lifetime gains since first launch. Persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifetimeGains {
    /// Timestamp (ms) of the very first sample ever ingested.
    pub first_launch_ts: u64,
    /// Total number of dome activations (transitions OFF→ON).
    pub total_dome_activations: u64,
    /// Total hours the dome was active.
    pub total_dome_hours: f64,
    /// CPU·hours differential (measured differential, always available).
    #[serde(
        rename = "total_cpu_hours_differential",
        alias = "total_cpu_hours_saved",
        default
    )]
    pub total_cpu_hours_differential: f64,
    /// Énergie intégrée depuis le capteur de puissance (kWh). Zéro si pas de watts réels.
    pub total_energy_kwh: f64,
    /// kg CO₂ équivalent = `total_energy_kwh` × facteur (empreinte liée au kWh mesuré, pas un «gain évité» sans baseline énergétique).
    #[serde(
        rename = "total_co2_measured_kg",
        alias = "total_co2_avoided_kg",
        default
    )]
    pub total_co2_measured_kg: f64,
    /// Coût cumulé = `total_energy_kwh` × prix (idem : pas des euros «économisés» par le dôme sans référence).
    #[serde(
        rename = "total_energy_cost_measured",
        alias = "total_cost_saved",
        default
    )]
    pub total_energy_cost_measured: f64,
    /// ∫ π(t)·dt sur les ticks dôme ACTIF (π issu de la formule ; entrées r(t) mesurées côté OS).
    pub total_dome_gain_integral: f64,
    /// Median KPI gain % across all measurements.
    pub avg_kpi_gain_pct: Option<f64>,
    /// Total samples ever ingested.
    pub total_samples: u64,
    /// Whether real power data (RAPL/battery) has ever been seen.
    pub has_real_power: bool,
    /// Total hours the machine was idle while monitored.
    pub total_idle_hours: f64,
    /// Total hours spent in media consumption (video/film).
    pub total_media_hours: f64,
    /// Cumul RAM·GB·h (pression mémoire × temps, même principe que CPU·h).
    #[serde(
        rename = "total_mem_gb_hours_differential",
        alias = "total_mem_gb_hours_saved",
        default
    )]
    pub total_mem_gb_hours_differential: f64,
    /// Heures cumulées d’échantillonnage avec SoulRAM actif et dôme inactif (Δt réels entre ticks télémétrie).
    #[serde(default)]
    pub soulram_active_hours: f64,
}

impl Default for LifetimeGains {
    fn default() -> Self {
        Self {
            first_launch_ts: 0,
            total_dome_activations: 0,
            total_dome_hours: 0.0,
            total_cpu_hours_differential: 0.0,
            total_energy_kwh: 0.0,
            total_co2_measured_kg: 0.0,
            total_energy_cost_measured: 0.0,
            total_dome_gain_integral: 0.0,
            avg_kpi_gain_pct: None,
            total_samples: 0,
            has_real_power: false,
            total_idle_hours: 0.0,
            total_media_hours: 0.0,
            total_mem_gb_hours_differential: 0.0,
            soulram_active_hours: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySummary {
    pub pricing: EnergyPricing,
    pub total: WindowSummary,
    pub hour: WindowSummary,
    pub day: WindowSummary,
    pub week: WindowSummary,
    pub month: WindowSummary,
    pub year: WindowSummary,
    pub live_power_w: Option<f64>,
    pub data_real_power: bool,
    /// "rapl", "battery", "cpu_differential" — source of energy/efficiency data.
    pub power_source: String,
    /// Cumulative lifetime gains since first launch.
    pub lifetime: LifetimeGains,
}

/// Synthèse des gains — structure partagée entre soulkernel-lite et le backend Tauri.
/// Calculée depuis `TelemetrySummary::to_gains_summary(...)`.
/// Positionnée en tête du rapport JSON pour une lecture immédiate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GainsSummary {
    // ── Période ──────────────────────────────────────────────────────────────
    pub first_launch_ts_ms: u64,
    pub monitored_hours: f64,
    pub total_samples: u64,

    // ── Dôme ─────────────────────────────────────────────────────────────────
    pub dome_activations_lifetime: u64,
    pub dome_active_hours_lifetime: f64,
    pub cpu_hours_differential_lifetime: f64,
    pub mem_gb_hours_differential_lifetime: f64,

    // ── SoulRAM ───────────────────────────────────────────────────────────────
    pub soulram_active_hours_lifetime: f64,

    // ── KPI efficacité ────────────────────────────────────────────────────────
    pub kpi_reward_ratio_session_pct: f64,
    pub kpi_avg_delta_session_w_per_pct: Option<f64>,
    pub kpi_avg_gain_lifetime_pct: Option<f64>,

    // ── Session courante ──────────────────────────────────────────────────────
    pub session_avg_power_w: Option<f64>,
    pub session_dome_on_avg_power_w: Option<f64>,
    pub session_dome_off_avg_power_w: Option<f64>,
    pub session_energy_saved_kwh: Option<f64>,
    pub session_cost_saved: Option<f64>,
    pub session_cost_currency: String,

    // ── Énergie lifetime (capteur réel) ───────────────────────────────────────
    pub has_real_power: bool,
    pub total_energy_kwh_lifetime: f64,
    pub total_cost_lifetime: f64,
    pub total_cost_currency: String,
    pub total_co2_kg_lifetime: f64,

    // ── Estimation sans capteur ───────────────────────────────────────────────
    pub estimated_kwh_no_sensor: f64,
    pub estimated_cost_no_sensor: f64,
    pub estimated_cost_currency: String,

    // ── Interprétation lisible ────────────────────────────────────────────────
    pub interpretation: String,
    pub caveats: Vec<String>,
}

impl TelemetrySummary {
    /// Calcule la synthèse des gains à partir de la télémétrie et des métriques KPI optionnelles.
    /// `kpi_reward_ratio_pct` et `kpi_avg_delta` viennent de `KpiLearningMemory` dans
    /// soulkernel-lite ; le backend Tauri peut passer `(0.0, None)` tant qu'il n'expose pas
    /// la mémoire KPI.
    pub fn to_gains_summary(
        &self,
        kpi_reward_ratio_pct: f64,
        kpi_avg_delta_w_per_pct: Option<f64>,
    ) -> GainsSummary {
        let lt = &self.lifetime;
        let t = &self.total;
        let pricing = &self.pricing;

        let monitored_hours =
            lt.total_idle_hours + lt.total_dome_hours + lt.soulram_active_hours;
        let session_energy_saved_kwh = t.energy_saved_kwh.filter(|&v| v > 0.0);
        let session_cost_saved =
            session_energy_saved_kwh.map(|kwh| kwh * pricing.price_per_kwh);
        let estimated_kwh_no_sensor = lt.total_cpu_hours_differential * 0.5;
        let estimated_cost_no_sensor = estimated_kwh_no_sensor * pricing.price_per_kwh;

        let mut lines: Vec<String> = Vec::new();
        if monitored_hours > 0.01 {
            lines.push(format!(
                "SoulKernel a monitoré {:.0}h depuis le premier lancement ({} échantillons).",
                monitored_hours, lt.total_samples
            ));
        }
        if lt.total_dome_activations > 0 {
            lines.push(format!(
                "Dôme : {} activation(s), {:.1}h actif.",
                lt.total_dome_activations, lt.total_dome_hours
            ));
            if lt.total_cpu_hours_differential > 0.001 {
                lines.push(format!(
                    "CPU économisé (diff. dôme vs baseline) : {:.4} CPU·h.",
                    lt.total_cpu_hours_differential
                ));
            }
            if lt.total_mem_gb_hours_differential > 0.001 {
                lines.push(format!(
                    "RAM libérée (diff.) : {:.4} GB·h.",
                    lt.total_mem_gb_hours_differential
                ));
            }
        } else {
            lines.push("Dôme : aucune activation enregistrée.".to_string());
        }
        if lt.soulram_active_hours > 0.01 {
            lines.push(format!("SoulRAM : {:.1}h actif.", lt.soulram_active_hours));
        }
        if let Some(delta) = kpi_avg_delta_w_per_pct {
            lines.push(format!(
                "KPI : amélioration médiane {:.2} W/% par cycle (session), taux de réussite {:.0}%.",
                delta, kpi_reward_ratio_pct
            ));
        }
        if lt.has_real_power {
            lines.push(format!(
                "Énergie mesurée (capteur réel) : {:.4} kWh · {:.4} {} · {:.4} kg CO₂.",
                lt.total_energy_kwh,
                lt.total_energy_cost_measured,
                pricing.currency,
                lt.total_co2_measured_kg,
            ));
            if let Some(saved) = session_energy_saved_kwh {
                lines.push(format!(
                    "Économie dôme cette session : ~{:.5} kWh (~{:.4} {}).",
                    saved,
                    saved * pricing.price_per_kwh,
                    pricing.currency,
                ));
            }
        } else {
            lines.push(
                "Pas de capteur de puissance : kWh et euros non mesurables directement."
                    .to_string(),
            );
            if estimated_kwh_no_sensor > 0.0 {
                lines.push(format!(
                    "Estimation conservative (0.5 W/% × CPU·h diff) : ~{:.5} kWh, ~{:.4} {}.",
                    estimated_kwh_no_sensor, estimated_cost_no_sensor, pricing.currency,
                ));
            }
        }

        let mut caveats: Vec<String> = Vec::new();
        if !lt.has_real_power {
            caveats.push(
                "Pas de capteur de puissance (RAPL / PDH / Meross) : l'énergie et le coût sont des estimations conservatives.".to_string(),
            );
        }
        if lt.total_cpu_hours_differential == 0.0 && lt.total_dome_activations > 0 {
            caveats.push(
                "CPU·h différentiel = 0 : la baseline CPU (10 min dôme OFF) n'est peut-être pas encore établie.".to_string(),
            );
        }
        if lt.total_dome_hours < 0.1 {
            caveats.push(
                "Dôme actif moins de 6 min au total : pas assez de données pour des statistiques fiables.".to_string(),
            );
        }

        GainsSummary {
            first_launch_ts_ms: lt.first_launch_ts,
            monitored_hours,
            total_samples: lt.total_samples,
            dome_activations_lifetime: lt.total_dome_activations,
            dome_active_hours_lifetime: lt.total_dome_hours,
            cpu_hours_differential_lifetime: lt.total_cpu_hours_differential,
            mem_gb_hours_differential_lifetime: lt.total_mem_gb_hours_differential,
            soulram_active_hours_lifetime: lt.soulram_active_hours,
            kpi_reward_ratio_session_pct: kpi_reward_ratio_pct,
            kpi_avg_delta_session_w_per_pct: kpi_avg_delta_w_per_pct,
            kpi_avg_gain_lifetime_pct: lt.avg_kpi_gain_pct,
            session_avg_power_w: t.avg_power_w,
            session_dome_on_avg_power_w: t.avg_power_dome_on_w,
            session_dome_off_avg_power_w: t.avg_power_dome_off_w,
            session_energy_saved_kwh,
            session_cost_saved,
            session_cost_currency: pricing.currency.clone(),
            has_real_power: lt.has_real_power,
            total_energy_kwh_lifetime: lt.total_energy_kwh,
            total_cost_lifetime: lt.total_energy_cost_measured,
            total_cost_currency: pricing.currency.clone(),
            total_co2_kg_lifetime: lt.total_co2_measured_kg,
            estimated_kwh_no_sensor,
            estimated_cost_no_sensor,
            estimated_cost_currency: pricing.currency.clone(),
            interpretation: lines.join(" "),
            caveats,
        }
    }
}

pub struct TelemetryState {
    path: PathBuf,
    pricing_path: PathBuf,
    lifetime_path: PathBuf,
    pricing: EnergyPricing,
    lifetime: LifetimeGains,
    ring: VecDeque<TelemetrySample>,
    last_ts_ms: Option<u64>,
    last_dome_active: bool,
    /// Running average CPU% when dome is OFF (baseline).
    cpu_baseline_acc: f64,
    cpu_baseline_dt: f64,
    /// Baseline ratio RAM utilisée (0..1) quand dôme OFF + ACTIF.
    mem_baseline_acc: f64,
    mem_baseline_dt: f64,
    /// Running KPI gains for lifetime median.
    kpi_gains_all: Vec<f64>,
    retention_ms: u64,
}

impl TelemetryState {
    pub fn new_default() -> Self {
        Self::new(
            default_telemetry_path(),
            default_telemetry_pricing_path(),
            default_telemetry_lifetime_path(),
        )
    }

    pub fn new(path: PathBuf, pricing_path: PathBuf, lifetime_path: PathBuf) -> Self {
        let pricing = load_pricing(&pricing_path).unwrap_or_default();
        let lifetime = load_lifetime(&lifetime_path).unwrap_or_default();
        let mut s = Self {
            path,
            pricing_path,
            lifetime_path,
            pricing,
            lifetime,
            ring: VecDeque::new(),
            last_ts_ms: None,
            last_dome_active: false,
            cpu_baseline_acc: 0.0,
            cpu_baseline_dt: 0.0,
            mem_baseline_acc: 0.0,
            mem_baseline_dt: 0.0,
            kpi_gains_all: Vec::new(),
            retention_ms: 370 * 24 * 3600 * 1000,
        };
        let _ = s.load_existing();
        s
    }

    pub fn lifetime(&self) -> LifetimeGains {
        self.lifetime.clone()
    }

    pub fn pricing(&self) -> EnergyPricing {
        self.pricing.clone()
    }

    pub fn set_pricing(&mut self, mut p: EnergyPricing) -> Result<(), String> {
        if p.currency.trim().is_empty() {
            p.currency = "EUR".to_string();
        }
        if !(p.price_per_kwh.is_finite() && p.price_per_kwh >= 0.0) {
            return Err("invalid price_per_kwh".to_string());
        }
        if !(p.co2_kg_per_kwh.is_finite() && p.co2_kg_per_kwh >= 0.0) {
            return Err("invalid co2_kg_per_kwh".to_string());
        }
        self.pricing = p.clone();
        if let Some(parent) = self.pricing_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(
            &self.pricing_path,
            serde_json::to_vec_pretty(&p).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn ingest(&mut self, req: TelemetryIngestRequest) -> Result<(), String> {
        let now_ms = req.ts_ms.unwrap_or_else(now_ms);
        let dt_s = match self.last_ts_ms {
            Some(prev) if now_ms > prev => ((now_ms - prev) as f64 / 1000.0).clamp(0.1, 30.0),
            _ => 1.0,
        };
        self.last_ts_ms = Some(now_ms);

        let (mem_used_ratio, mem_total_mb_stored) = match (req.mem_used_mb, req.mem_total_mb) {
            (Some(u), Some(t)) if t > 0.0 && u.is_finite() && t.is_finite() => {
                let r = (u / t).clamp(0.0, 1.0);
                (Some(r), Some(t))
            }
            _ => (None, None),
        };

        let sample = TelemetrySample {
            ts_ms: now_ms,
            dt_s,
            power_watts: req.power_watts.filter(|v| v.is_finite() && *v >= 0.0),
            dome_active: req.dome_active,
            soulram_active: req.soulram_active,
            kpi_gain_median_pct: req.kpi_gain_median_pct.filter(|v| v.is_finite()),
            cpu_pct: req.cpu_pct.filter(|v| v.is_finite() && *v >= 0.0),
            pi: req.pi.filter(|v| v.is_finite()),
            machine_activity: req.machine_activity,
            mem_used_ratio,
            mem_total_mb: mem_total_mb_stored,
            power_source_tag: req
                .power_source_tag
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            io_read_mb_s: req.io_read_mb_s.filter(|v| v.is_finite() && *v >= 0.0),
            io_write_mb_s: req.io_write_mb_s.filter(|v| v.is_finite() && *v >= 0.0),
            gpu_pct: req.gpu_pct.filter(|v| v.is_finite() && *v >= 0.0),
            gpu_power_watts: req.gpu_power_watts.filter(|v| v.is_finite() && *v >= 0.0),
            gpu_temp_c: req.gpu_temp_c.filter(|v| v.is_finite()),
            cpu_temp_c: req.cpu_temp_c.filter(|v| v.is_finite()),
            zram_used_mb: req.zram_used_mb,
            psi_cpu: req.psi_cpu.filter(|v| v.is_finite() && *v >= 0.0),
            psi_mem: req.psi_mem.filter(|v| v.is_finite() && *v >= 0.0),
            load_avg_1m_norm: req.load_avg_1m_norm.filter(|v| v.is_finite() && *v >= 0.0),
            runnable_tasks: req.runnable_tasks,
            on_battery: req.on_battery,
            battery_percent: req.battery_percent.filter(|v| v.is_finite() && *v >= 0.0),
            page_faults_per_sec: req
                .page_faults_per_sec
                .filter(|v| v.is_finite() && *v >= 0.0),
            webview_host_cpu_sum: req
                .webview_host_cpu_sum
                .filter(|v| v.is_finite() && *v >= 0.0),
            webview_host_mem_mb: req.webview_host_mem_mb,
        };

        // ── Update lifetime gains ─────────────────────────────────────────
        self.update_lifetime(&sample);

        self.ring.push_back(sample.clone());
        self.prune(now_ms);
        self.append_sample(&sample)?;

        // Persist lifetime every 20 samples to avoid excessive I/O.
        if self.lifetime.total_samples.is_multiple_of(20) {
            let _ = self.save_lifetime();
        }
        Ok(())
    }

    pub fn summary(&self, now_ms: u64) -> TelemetrySummary {
        let data_real_power = self.ring.iter().any(|s| s.power_watts.is_some());
        let power_source = self
            .ring
            .back()
            .and_then(|s| s.power_source_tag.clone())
            .unwrap_or_else(|| {
                if data_real_power {
                    if self.lifetime.has_real_power {
                        "rapl".to_string()
                    } else {
                        "battery".to_string()
                    }
                } else {
                    "cpu_differential".to_string()
                }
            });
        TelemetrySummary {
            pricing: self.pricing.clone(),
            total: self.window_summary(now_ms, None),
            hour: self.window_summary(now_ms, Some(3600 * 1000)),
            day: self.window_summary(now_ms, Some(24 * 3600 * 1000)),
            week: self.window_summary(now_ms, Some(7 * 24 * 3600 * 1000)),
            month: self.window_summary(now_ms, Some(30 * 24 * 3600 * 1000)),
            year: self.window_summary(now_ms, Some(365 * 24 * 3600 * 1000)),
            live_power_w: self.ring.back().and_then(|s| s.power_watts),
            data_real_power,
            power_source,
            lifetime: self.lifetime.clone(),
        }
    }

    fn window_summary(&self, now_ms: u64, window_ms: Option<u64>) -> WindowSummary {
        let start_ms = window_ms.map(|w| now_ms.saturating_sub(w)).unwrap_or(0);
        let mut out = WindowSummary::default();
        let mut weighted_w_sum = 0.0;
        let mut weighted_w_dt = 0.0;
        let mut active_dt = 0.0;
        let mut passive_clean_dt = 0.0;
        let mut gains = Vec::new();
        let mut idle_dt = 0.0;
        let mut media_dt = 0.0;

        // CPU baseline: average CPU% during dome-OFF active periods in this window.
        let mut cpu_off_sum = 0.0;
        let mut cpu_off_dt = 0.0;

        // First pass: compute dome-OFF CPU baseline.
        for s in self.ring.iter().filter(|s| s.ts_ms >= start_ms) {
            let activity = s.machine_activity.unwrap_or(MachineActivity::Active);
            if !s.dome_active && activity == MachineActivity::Active {
                if let Some(cpu) = s.cpu_pct {
                    cpu_off_sum += cpu * s.dt_s;
                    cpu_off_dt += s.dt_s;
                }
            }
        }
        let cpu_baseline_pct = if cpu_off_dt > 0.0 {
            cpu_off_sum / cpu_off_dt
        } else {
            0.0
        };

        let mut mem_off_sum = 0.0;
        let mut mem_off_dt = 0.0;
        for s in self.ring.iter().filter(|s| s.ts_ms >= start_ms) {
            let activity = s.machine_activity.unwrap_or(MachineActivity::Active);
            if !s.dome_active && activity == MachineActivity::Active {
                if let Some(r) = s.mem_used_ratio {
                    mem_off_sum += r * s.dt_s;
                    mem_off_dt += s.dt_s;
                }
            }
        }
        let mem_baseline_ratio = if mem_off_dt > 0.0 {
            mem_off_sum / mem_off_dt
        } else {
            0.0
        };

        // Second pass: aggregate all metrics.
        let mut power_on_sum = 0.0_f64;
        let mut power_on_dt = 0.0_f64;
        let mut power_off_sum = 0.0_f64;
        let mut power_off_dt = 0.0_f64;
        for s in self.ring.iter().filter(|s| s.ts_ms >= start_ms) {
            let activity = s.machine_activity.unwrap_or(MachineActivity::Active);
            out.samples += 1;
            out.duration_h += s.dt_s / 3600.0;

            match activity {
                MachineActivity::Idle => idle_dt += s.dt_s,
                MachineActivity::Media => media_dt += s.dt_s,
                MachineActivity::Active => {}
            }

            if s.dome_active && activity == MachineActivity::Active {
                active_dt += s.dt_s;
                // CPU·h saved = (baseline% − dome%) × dt / 3600 / 100
                if let Some(cpu) = s.cpu_pct {
                    let delta_pct = (cpu_baseline_pct - cpu).max(0.0);
                    out.cpu_hours_differential += delta_pct * s.dt_s / 360_000.0;
                }
                if let (Some(r), Some(tmb)) = (s.mem_used_ratio, s.mem_total_mb) {
                    let gb = tmb / 1024.0;
                    if gb > 0.01 && mem_off_dt > 0.0 {
                        let delta_gb = (mem_baseline_ratio - r).max(0.0) * gb;
                        out.mem_gb_hours_differential += delta_gb * s.dt_s / 3600.0;
                    }
                }
                // Real π integral.
                if let Some(pi) = s.pi {
                    out.dome_gain_integral += pi * s.dt_s;
                }
            }
            if !s.dome_active && s.soulram_active {
                passive_clean_dt += s.dt_s;
            }
            if let Some(g) = s.kpi_gain_median_pct {
                gains.push(g);
            }
            if let Some(w) = s.power_watts {
                out.has_power_data = true;
                weighted_w_sum += w * s.dt_s;
                weighted_w_dt += s.dt_s;
                out.energy_kwh += (w * s.dt_s) / 3_600_000.0;
                if s.dome_active {
                    power_on_sum += w * s.dt_s;
                    power_on_dt += s.dt_s;
                } else {
                    power_off_sum += w * s.dt_s;
                    power_off_dt += s.dt_s;
                }
            }
        }

        out.avg_power_w = if weighted_w_dt > 0.0 {
            Some(weighted_w_sum / weighted_w_dt)
        } else {
            None
        };
        out.avg_power_dome_on_w = if power_on_dt > 0.0 {
            Some(power_on_sum / power_on_dt)
        } else {
            None
        };
        out.avg_power_dome_off_w = if power_off_dt > 0.0 {
            Some(power_off_sum / power_off_dt)
        } else {
            None
        };
        out.energy_saved_kwh = match (out.avg_power_dome_on_w, out.avg_power_dome_off_w) {
            (Some(on_w), Some(off_w)) if off_w > on_w => {
                Some((off_w - on_w) * power_on_dt / 3_600_000.0)
            }
            _ => None,
        };
        out.dome_active_ratio = if out.duration_h > 0.0 {
            (active_dt / (out.duration_h * 3600.0)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        out.passive_clean_h = passive_clean_dt / 3600.0;
        out.cost = out.energy_kwh * self.pricing.price_per_kwh;
        out.co2_kg = out.energy_kwh * self.pricing.co2_kg_per_kwh;
        out.kpi_gain_median_pct = median(&gains);
        let total_dt = out.duration_h * 3600.0;
        out.idle_ratio = if total_dt > 0.0 {
            (idle_dt / total_dt).clamp(0.0, 1.0)
        } else {
            0.0
        };
        out.media_ratio = if total_dt > 0.0 {
            (media_dt / total_dt).clamp(0.0, 1.0)
        } else {
            0.0
        };
        out
    }

    fn append_sample(&self, s: &TelemetrySample) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| e.to_string())?;
        let line = serde_json::to_string(s).map_err(|e| e.to_string())?;
        writeln!(file, "{line}").map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Incrementally update lifetime counters from a single new sample.
    fn update_lifetime(&mut self, s: &TelemetrySample) {
        let activity = s.machine_activity.unwrap_or(MachineActivity::Active);
        // First launch timestamp.
        if self.lifetime.first_launch_ts == 0 {
            self.lifetime.first_launch_ts = s.ts_ms;
        }
        self.lifetime.total_samples += 1;
        // Track idle/media hours.
        match activity {
            MachineActivity::Idle => self.lifetime.total_idle_hours += s.dt_s / 3600.0,
            MachineActivity::Media => self.lifetime.total_media_hours += s.dt_s / 3600.0,
            MachineActivity::Active => {}
        }

        // Detect dome activation (OFF → ON transition).
        if s.dome_active && !self.last_dome_active {
            self.lifetime.total_dome_activations += 1;
        }
        self.last_dome_active = s.dome_active;

        if !s.dome_active && s.soulram_active {
            self.lifetime.soulram_active_hours += s.dt_s / 3600.0;
        }

        if s.dome_active {
            self.lifetime.total_dome_hours += s.dt_s / 3600.0;

            // Only count dome savings during Active periods — idle/media gains are not real.
            if activity == MachineActivity::Active {
                // CPU·h saved: compare against running baseline.
                if let Some(cpu) = s.cpu_pct {
                    let baseline = if self.cpu_baseline_dt > 0.0 {
                        self.cpu_baseline_acc / self.cpu_baseline_dt
                    } else {
                        0.0
                    };
                    let delta_pct = (baseline - cpu).max(0.0);
                    self.lifetime.total_cpu_hours_differential += delta_pct * s.dt_s / 360_000.0;
                }

                // Real π integral.
                if let Some(pi) = s.pi {
                    self.lifetime.total_dome_gain_integral += pi * s.dt_s;
                }

                if let (Some(r), Some(tmb)) = (s.mem_used_ratio, s.mem_total_mb) {
                    let gb = tmb / 1024.0;
                    if gb > 0.01 {
                        let baseline = if self.mem_baseline_dt > 0.0 {
                            self.mem_baseline_acc / self.mem_baseline_dt
                        } else {
                            0.0
                        };
                        if baseline > 0.0 {
                            let delta_gb = (baseline - r).max(0.0) * gb;
                            self.lifetime.total_mem_gb_hours_differential +=
                                delta_gb * s.dt_s / 3600.0;
                        }
                    }
                }
            }
        } else if activity == MachineActivity::Active {
            // Dome OFF + Active: accumulate CPU baseline.
            if let Some(cpu) = s.cpu_pct {
                self.cpu_baseline_acc += cpu * s.dt_s;
                self.cpu_baseline_dt += s.dt_s;
                // Sliding decay: keep ~10 min of baseline data relevant.
                if self.cpu_baseline_dt > 600.0 {
                    let ratio = 600.0 / self.cpu_baseline_dt;
                    self.cpu_baseline_acc *= ratio;
                    self.cpu_baseline_dt = 600.0;
                }
            }
            if let Some(r) = s.mem_used_ratio {
                self.mem_baseline_acc += r * s.dt_s;
                self.mem_baseline_dt += s.dt_s;
                if self.mem_baseline_dt > 600.0 {
                    let ratio = 600.0 / self.mem_baseline_dt;
                    self.mem_baseline_acc *= ratio;
                    self.mem_baseline_dt = 600.0;
                }
            }
        }

        // Energy (only when real power data present).
        if let Some(w) = s.power_watts {
            self.lifetime.has_real_power = true;
            self.lifetime.total_energy_kwh += (w * s.dt_s) / 3_600_000.0;
            self.lifetime.total_co2_measured_kg =
                self.lifetime.total_energy_kwh * self.pricing.co2_kg_per_kwh;
            self.lifetime.total_energy_cost_measured =
                self.lifetime.total_energy_kwh * self.pricing.price_per_kwh;
        }

        // KPI gains running median.
        if let Some(g) = s.kpi_gain_median_pct {
            self.kpi_gains_all.push(g);
            self.lifetime.avg_kpi_gain_pct = median(&self.kpi_gains_all);
        }
    }

    fn save_lifetime(&self) -> Result<(), String> {
        if let Some(parent) = self.lifetime_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(
            &self.lifetime_path,
            serde_json::to_vec_pretty(&self.lifetime).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn load_existing(&mut self) -> Result<(), String> {
        let file = match std::fs::File::open(&self.path) {
            Ok(f) => f,
            Err(_) => return Ok(()),
        };
        let reader = BufReader::new(file);
        for line in reader.lines().map_while(Result::ok) {
            if let Ok(sample) = serde_json::from_str::<TelemetrySample>(&line) {
                self.last_ts_ms = Some(
                    self.last_ts_ms
                        .map_or(sample.ts_ms, |p| p.max(sample.ts_ms)),
                );
                self.ring.push_back(sample);
            }
        }
        self.prune(now_ms());
        Ok(())
    }

    fn prune(&mut self, now_ms: u64) {
        let min_ts = now_ms.saturating_sub(self.retention_ms);
        while let Some(front) = self.ring.front() {
            if front.ts_ms < min_ts {
                self.ring.pop_front();
            } else {
                break;
            }
        }
    }
}

pub fn default_telemetry_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata)
            .join("SoulKernel")
            .join("telemetry")
            .join("energy_samples.jsonl");
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(xdg)
                .join("SoulKernel")
                .join("telemetry")
                .join("energy_samples.jsonl");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("SoulKernel")
                .join("telemetry")
                .join("energy_samples.jsonl");
        }
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("soulkernel_energy_samples.jsonl")
}

pub fn default_telemetry_pricing_path() -> PathBuf {
    default_telemetry_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("energy_pricing.json")
}

pub fn default_telemetry_lifetime_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata)
            .join("SoulKernel")
            .join("telemetry")
            .join("lifetime_gains.json");
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(xdg)
                .join("SoulKernel")
                .join("telemetry")
                .join("lifetime_gains.json");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("SoulKernel")
                .join("telemetry")
                .join("lifetime_gains.json");
        }
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("soulkernel_lifetime_gains.json")
}

pub fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn median(v: &[f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    let mut arr = v.to_vec();
    arr.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(arr[arr.len() / 2])
}

fn load_pricing(path: &PathBuf) -> Option<EnergyPricing> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice::<EnergyPricing>(&bytes).ok()
}

fn load_lifetime(path: &PathBuf) -> Option<LifetimeGains> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice::<LifetimeGains>(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("soulkernel-{name}-{uniq}.jsonl"))
    }

    fn new_state() -> TelemetryState {
        let base = temp_path("telemetry");
        let pricing = base.with_extension("pricing.json");
        let lifetime = base.with_extension("lifetime.json");
        TelemetryState::new(base, pricing, lifetime)
    }

    fn ingest_sample(
        state: &mut TelemetryState,
        ts_ms: u64,
        dome_active: bool,
        cpu_pct: f64,
        mem_used_mb: f64,
        power_watts: Option<f64>,
        machine_activity: MachineActivity,
    ) {
        state
            .ingest(TelemetryIngestRequest {
                ts_ms: Some(ts_ms),
                power_watts,
                dome_active,
                cpu_pct: Some(cpu_pct),
                pi: Some(1.0),
                machine_activity: Some(machine_activity),
                mem_used_mb: Some(mem_used_mb),
                mem_total_mb: Some(8_192.0),
                power_source_tag: power_watts.map(|_| "meross_wall".to_string()),
                ..Default::default()
            })
            .unwrap();
    }

    #[test]
    fn window_summary_computes_energy_and_differentials() {
        let mut state = new_state();
        let base = 1_000_000;
        ingest_sample(
            &mut state,
            base,
            false,
            60.0,
            4_096.0,
            Some(120.0),
            MachineActivity::Active,
        );
        ingest_sample(
            &mut state,
            base + 5_000,
            true,
            30.0,
            2_048.0,
            Some(60.0),
            MachineActivity::Active,
        );

        let summary = state.summary(base + 5_000);
        // Sample 1: dt=1s (first sample default), 120W → 120 Ws
        // Sample 2: dt=5s, 60W → 300 Ws  — total = 420 Ws = 420/3_600_000 kWh
        let expected_kwh = 420.0 / 3_600_000.0;
        assert!(summary.total.has_power_data);
        assert_eq!(summary.power_source, "meross_wall");
        assert!((summary.total.energy_kwh - expected_kwh).abs() < 1e-9);
        assert!((summary.total.cpu_hours_differential - (30.0 * 5.0 / 360_000.0)).abs() < 1e-9);
        assert!((summary.total.mem_gb_hours_differential - (2.0 * 5.0 / 3600.0)).abs() < 1e-9);
        assert!((summary.lifetime.total_energy_kwh - expected_kwh).abs() < 1e-9);
        assert!(summary.lifetime.total_cpu_hours_differential > 0.0);
        assert!(summary.lifetime.total_mem_gb_hours_differential > 0.0);
        // Dome-split power: off=120W (1s), on=60W (5s).
        assert!((summary.total.avg_power_dome_off_w.unwrap() - 120.0).abs() < 1e-9);
        assert!((summary.total.avg_power_dome_on_w.unwrap() - 60.0).abs() < 1e-9);
        // energy_saved_kwh = (120-60) * 5 / 3_600_000 = 300/3_600_000
        let expected_saved = 300.0 / 3_600_000.0;
        assert!((summary.total.energy_saved_kwh.unwrap() - expected_saved).abs() < 1e-12);
    }

    #[test]
    fn idle_and_media_do_not_count_as_dome_differential_gain() {
        let mut state = new_state();
        let base = 2_000_000;
        ingest_sample(
            &mut state,
            base,
            false,
            50.0,
            4_096.0,
            None,
            MachineActivity::Active,
        );
        ingest_sample(
            &mut state,
            base + 5_000,
            true,
            5.0,
            2_048.0,
            None,
            MachineActivity::Idle,
        );
        ingest_sample(
            &mut state,
            base + 10_000,
            true,
            4.0,
            2_048.0,
            None,
            MachineActivity::Media,
        );

        let summary = state.summary(base + 10_000);
        assert_eq!(summary.total.cpu_hours_differential, 0.0);
        assert_eq!(summary.total.mem_gb_hours_differential, 0.0);
        assert!(summary.total.idle_ratio > 0.0);
        assert!(summary.total.media_ratio > 0.0);
        assert_eq!(summary.lifetime.total_cpu_hours_differential, 0.0);
        assert_eq!(summary.lifetime.total_mem_gb_hours_differential, 0.0);
    }

    #[test]
    fn deserialize_legacy_lifetime_names() {
        let path = temp_path("legacy-lifetime");
        let payload = r#"{
          "first_launch_ts": 1,
          "total_dome_activations": 2,
          "total_dome_hours": 3.5,
          "total_cpu_hours_saved": 1.25,
          "total_energy_kwh": 0.75,
          "total_co2_avoided_kg": 0.02,
          "total_cost_saved": 0.11,
          "total_dome_gain_integral": 9.0,
          "avg_kpi_gain_pct": 4.0,
          "total_samples": 10,
          "has_real_power": true,
          "total_idle_hours": 0.5,
          "total_media_hours": 0.25,
          "total_mem_gb_hours_saved": 2.5,
          "soulram_active_hours": 1.0
        }"#;
        fs::write(&path, payload).unwrap();
        let loaded = load_lifetime(&path).unwrap();
        assert_eq!(loaded.total_cpu_hours_differential, 1.25);
        assert_eq!(loaded.total_mem_gb_hours_differential, 2.5);
        assert_eq!(loaded.total_co2_measured_kg, 0.02);
        assert_eq!(loaded.total_energy_cost_measured, 0.11);
        let _ = fs::remove_file(path);
    }
}
