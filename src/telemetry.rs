use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WindowSummary {
    pub samples: usize,
    pub duration_h: f64,
    pub avg_power_w: Option<f64>,
    pub has_power_data: bool,
    pub energy_kwh: f64,
    pub cost: f64,
    pub co2_kg: f64,
    pub dome_active_ratio: f64,
    pub passive_clean_h: f64,
    pub kpi_gain_median_pct: Option<f64>,
    /// CPU·hours saved = ∫(cpu_baseline − cpu_dome) × dt / 3600.
    /// Computed from measured CPU differential (dome ON vs OFF). Always real data.
    pub cpu_hours_saved: f64,
    /// Real dome gain integral Σ(π_i × dt_i) for dome-active samples.
    pub dome_gain_integral: f64,
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
    /// CPU·hours saved (measured differential, always available).
    pub total_cpu_hours_saved: f64,
    /// Total energy consumed while monitored (kWh). Only when power data available.
    pub total_energy_kwh: f64,
    /// CO₂ avoided (kg). Computed from energy × pricing factor.
    pub total_co2_avoided_kg: f64,
    /// Cost saved (currency). Computed from energy × pricing factor.
    pub total_cost_saved: f64,
    /// Cumulative real dome gain integral Σ(π_i × dt_i) − n × (C_setup + C_rb).
    pub total_dome_gain_integral: f64,
    /// Median KPI gain % across all measurements.
    pub avg_kpi_gain_pct: Option<f64>,
    /// Total samples ever ingested.
    pub total_samples: u64,
    /// Whether real power data (RAPL/battery) has ever been seen.
    pub has_real_power: bool,
}

impl Default for LifetimeGains {
    fn default() -> Self {
        Self {
            first_launch_ts: 0,
            total_dome_activations: 0,
            total_dome_hours: 0.0,
            total_cpu_hours_saved: 0.0,
            total_energy_kwh: 0.0,
            total_co2_avoided_kg: 0.0,
            total_cost_saved: 0.0,
            total_dome_gain_integral: 0.0,
            avg_kpi_gain_pct: None,
            total_samples: 0,
            has_real_power: false,
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
    /// Running KPI gains for lifetime median.
    kpi_gains_all: Vec<f64>,
    retention_ms: u64,
}

impl TelemetryState {
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

        let sample = TelemetrySample {
            ts_ms: now_ms,
            dt_s,
            power_watts: req.power_watts.filter(|v| v.is_finite() && *v >= 0.0),
            dome_active: req.dome_active,
            soulram_active: req.soulram_active,
            kpi_gain_median_pct: req.kpi_gain_median_pct.filter(|v| v.is_finite()),
            cpu_pct: req.cpu_pct.filter(|v| v.is_finite() && *v >= 0.0),
            pi: req.pi.filter(|v| v.is_finite()),
        };

        // ── Update lifetime gains ─────────────────────────────────────────
        self.update_lifetime(&sample);

        self.ring.push_back(sample.clone());
        self.prune(now_ms);
        self.append_sample(&sample)?;

        // Persist lifetime every 20 samples to avoid excessive I/O.
        if self.lifetime.total_samples % 20 == 0 {
            let _ = self.save_lifetime();
        }
        Ok(())
    }

    pub fn summary(&self, now_ms: u64) -> TelemetrySummary {
        let data_real_power = self.ring.iter().any(|s| s.power_watts.is_some());
        let power_source = if data_real_power {
            // Distinguish RAPL vs battery by checking if we ever had power
            // without a battery path — on Linux RAPL is the primary source.
            if self.lifetime.has_real_power {
                "rapl".to_string()
            } else {
                "battery".to_string()
            }
        } else {
            "cpu_differential".to_string()
        };
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

        // CPU baseline: average CPU% during dome-OFF periods in this window.
        let mut cpu_off_sum = 0.0;
        let mut cpu_off_dt = 0.0;

        // First pass: compute dome-OFF CPU baseline.
        for s in self.ring.iter().filter(|s| s.ts_ms >= start_ms) {
            if !s.dome_active {
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

        // Second pass: aggregate all metrics.
        for s in self.ring.iter().filter(|s| s.ts_ms >= start_ms) {
            out.samples += 1;
            out.duration_h += s.dt_s / 3600.0;
            if s.dome_active {
                active_dt += s.dt_s;
                // CPU·h saved = (baseline% − dome%) × dt / 3600 / 100
                if let Some(cpu) = s.cpu_pct {
                    let delta_pct = (cpu_baseline_pct - cpu).max(0.0);
                    out.cpu_hours_saved += delta_pct * s.dt_s / 360_000.0;
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
            }
        }

        out.avg_power_w = if weighted_w_dt > 0.0 {
            Some(weighted_w_sum / weighted_w_dt)
        } else {
            None
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
        // First launch timestamp.
        if self.lifetime.first_launch_ts == 0 {
            self.lifetime.first_launch_ts = s.ts_ms;
        }
        self.lifetime.total_samples += 1;

        // Detect dome activation (OFF → ON transition).
        if s.dome_active && !self.last_dome_active {
            self.lifetime.total_dome_activations += 1;
        }
        self.last_dome_active = s.dome_active;

        if s.dome_active {
            self.lifetime.total_dome_hours += s.dt_s / 3600.0;

            // CPU·h saved: compare against running baseline.
            if let Some(cpu) = s.cpu_pct {
                let baseline = if self.cpu_baseline_dt > 0.0 {
                    self.cpu_baseline_acc / self.cpu_baseline_dt
                } else {
                    0.0
                };
                let delta_pct = (baseline - cpu).max(0.0);
                self.lifetime.total_cpu_hours_saved += delta_pct * s.dt_s / 360_000.0;
            }

            // Real π integral.
            if let Some(pi) = s.pi {
                self.lifetime.total_dome_gain_integral += pi * s.dt_s;
            }
        } else {
            // Dome OFF: accumulate CPU baseline.
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
        }

        // Energy (only when real power data present).
        if let Some(w) = s.power_watts {
            self.lifetime.has_real_power = true;
            self.lifetime.total_energy_kwh += (w * s.dt_s) / 3_600_000.0;
            self.lifetime.total_co2_avoided_kg =
                self.lifetime.total_energy_kwh * self.pricing.co2_kg_per_kwh;
            self.lifetime.total_cost_saved =
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
