use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiProbeResult {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub duration_ms: u64,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout_tail: String,
    pub stderr_tail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRequest {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    pub runs_per_state: usize,
    pub workload: String,
    pub kappa: f64,
    pub sigma_max: f64,
    pub eta: f64,
    #[serde(default)]
    pub target_pid: Option<u32>,
    #[serde(default)]
    pub policy_mode: Option<String>,
    #[serde(default)]
    pub soulram_percent: Option<u8>,
    #[serde(default = "default_settle_ms")]
    pub settle_ms: u64,
}

fn default_settle_ms() -> u64 {
    1200
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkPhase {
    Off,
    On,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSample {
    pub idx: usize,
    pub phase: BenchmarkPhase,
    pub ts: String,
    pub duration_ms: u64,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub dome_active: bool,
    pub workload: String,
    pub kappa: f64,
    pub sigma_max: f64,
    pub eta: f64,
    pub sigma_before: Option<f64>,
    pub sigma_after: Option<f64>,
    pub cpu_before_pct: Option<f64>,
    pub cpu_after_pct: Option<f64>,
    pub mem_before_gb: Option<f64>,
    pub mem_after_gb: Option<f64>,
    #[serde(default)]
    pub gpu_before_pct: Option<f64>,
    #[serde(default)]
    pub gpu_after_pct: Option<f64>,
    #[serde(default)]
    pub io_before_mb_s: Option<f64>,
    #[serde(default)]
    pub io_after_mb_s: Option<f64>,
    #[serde(default)]
    pub power_before_watts: Option<f64>,
    #[serde(default)]
    pub power_after_watts: Option<f64>,
    #[serde(default)]
    pub cpu_temp_before_c: Option<f64>,
    #[serde(default)]
    pub cpu_temp_after_c: Option<f64>,
    #[serde(default)]
    pub gpu_temp_before_c: Option<f64>,
    #[serde(default)]
    pub gpu_temp_after_c: Option<f64>,
    #[serde(default)]
    pub sigma_effective_before: Option<f64>,
    #[serde(default)]
    pub sigma_effective_after: Option<f64>,
    pub stdout_tail: String,
    pub stderr_tail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSummary {
    pub samples_off_ok: usize,
    pub samples_on_ok: usize,
    pub median_off_ms: Option<f64>,
    pub median_on_ms: Option<f64>,
    pub p95_off_ms: Option<f64>,
    pub p95_on_ms: Option<f64>,
    pub gain_median_pct: Option<f64>,
    pub gain_p95_pct: Option<f64>,
    /// Médiane RAM utilisée (Go) après sonde : (med_OFF − med_ON) / med_OFF × 100 ; positif = moins de RAM en phase ON.
    #[serde(default)]
    pub gain_mem_median_pct: Option<f64>,
    /// Idem pour GPU % après sonde (si capteur disponible).
    #[serde(default)]
    pub gain_gpu_median_pct: Option<f64>,
    /// Idem pour CPU % après sonde (seuil minimal pour éviter le bruit près de 0 %).
    #[serde(default)]
    pub gain_cpu_median_pct: Option<f64>,
    /// Baisse médiane de puissance totale (positive = mieux).
    #[serde(default)]
    pub gain_power_median_pct: Option<f64>,
    /// Baisse médiane de sigma effectif (positive = moins de stress).
    #[serde(default)]
    pub gain_sigma_median_pct: Option<f64>,
    /// Baisse médiane des températures CPU/GPU (positive = mieux).
    #[serde(default)]
    pub gain_cpu_temp_median_pct: Option<f64>,
    #[serde(default)]
    pub gain_gpu_temp_median_pct: Option<f64>,
    /// Score composite pour gain net sans régression ressource.
    #[serde(default)]
    pub efficiency_score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSession {
    pub started_at: String,
    pub finished_at: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub runs_per_state: usize,
    pub settle_ms: u64,
    pub workload: String,
    pub kappa: f64,
    pub sigma_max: f64,
    pub eta: f64,
    pub target_pid: Option<u32>,
    pub policy_mode: Option<String>,
    #[serde(default)]
    pub soulram_percent: Option<u8>,
    pub samples: Vec<BenchmarkSample>,
    pub summary: BenchmarkSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkTuningAdvice {
    pub sample_size: usize,
    pub recommended_kappa: f64,
    pub recommended_sigma_max: f64,
    pub recommended_eta: f64,
    pub recommended_policy_mode: String,
    pub recommended_soulram_percent: u8,
    pub expected_gain_median_pct: Option<f64>,
    pub expected_gain_p95_pct: Option<f64>,
    pub expected_efficiency_score: Option<f64>,
    pub confidence: f64,
    pub composite_score: f64,
    pub basis: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkHistoryResponse {
    pub sessions: Vec<BenchmarkSession>,
    pub last_summary: Option<BenchmarkSummary>,
    pub advice: Option<BenchmarkTuningAdvice>,
    pub top_sessions: Vec<RankedBenchmarkSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedBenchmarkSession {
    pub rank: usize,
    pub composite_score: f64,
    pub started_at: String,
    pub workload: String,
    pub runs_per_state: usize,
    pub kappa: f64,
    pub sigma_max: f64,
    pub eta: f64,
    pub gain_median_pct: Option<f64>,
    pub gain_p95_pct: Option<f64>,
}

pub struct BenchmarkState {
    path: PathBuf,
    sessions: Vec<BenchmarkSession>,
}

/// Sonde intégrée : échantillonne les métriques OS pendant `duration_ms` (pas de processus externe).
/// Utile pour comparer dôme OFF/ON sur charge « système » (CPU, RAM, I/O, σ, etc.).
pub async fn execute_system_probe(duration_ms: u64) -> Result<KpiProbeResult, String> {
    let duration_ms = duration_ms.clamp(500, 120_000);
    let start = Instant::now();
    let mut cpu_sum = 0.0_f64;
    let mut mem_sum = 0.0_f64;
    let mut io_sum = 0.0_f64;
    let mut io_n = 0_u32;
    let mut n = 0_u64;
    while start.elapsed().as_millis() < u128::from(duration_ms) {
        match crate::metrics::collect() {
            Ok(m) => {
                cpu_sum += m.raw.cpu_pct;
                mem_sum += m.mem;
                if let (Some(r), Some(w)) = (m.raw.io_read_mb_s, m.raw.io_write_mb_s) {
                    io_sum += r + w;
                    io_n += 1;
                }
                n += 1;
            }
            Err(_) => {}
        }
        tokio::time::sleep(Duration::from_millis(220)).await;
    }
    let elapsed = start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let cpu_avg = if n > 0 { cpu_sum / n as f64 } else { 0.0 };
    let mem_avg = if n > 0 { mem_sum / n as f64 } else { 0.0 };
    let io_avg = if io_n > 0 { io_sum / f64::from(io_n) } else { 0.0 };
    let summary = format!(
        "OS {}ms | CPU~{:.1}% RAM~{:.0}% | I/O~{:.1}MB/s | n={}",
        elapsed,
        cpu_avg,
        mem_avg * 100.0,
        io_avg,
        n
    );
    Ok(KpiProbeResult {
        command: "system".to_string(),
        args: vec![duration_ms.to_string()],
        cwd: None,
        duration_ms: elapsed,
        success: true,
        exit_code: Some(0),
        stdout_tail: tail_text(summary.as_bytes(), 600),
        stderr_tail: String::new(),
    })
}

fn tail_text(buf: &[u8], max_chars: usize) -> String {
    let s = String::from_utf8_lossy(buf);
    let v: Vec<char> = s.chars().collect();
    let start = v.len().saturating_sub(max_chars);
    v[start..].iter().collect::<String>()
}

pub async fn execute_probe(
    command: String,
    args: Vec<String>,
    cwd: Option<String>,
) -> Result<KpiProbeResult, String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err("kpi command is empty".to_string());
    }

    let mut cmd = tokio::process::Command::new(trimmed);
    cmd.args(args.clone());
    if let Some(c) = cwd.as_ref() {
        let ctrim = c.trim();
        if !ctrim.is_empty() {
            cmd.current_dir(ctrim);
        }
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW — évite le flash console par sonde
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let start = Instant::now();
    let out = cmd.output().await.map_err(|e| e.to_string())?;
    let duration_ms = start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

    Ok(KpiProbeResult {
        command: trimmed.to_string(),
        args,
        cwd,
        duration_ms,
        success: out.status.success(),
        exit_code: out.status.code(),
        stdout_tail: tail_text(&out.stdout, 600),
        stderr_tail: tail_text(&out.stderr, 600),
    })
}

fn median_sorted_u64(sorted: &[u64]) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let n = sorted.len();
    if n % 2 == 1 {
        Some(sorted[n / 2] as f64)
    } else {
        Some((sorted[n / 2 - 1] + sorted[n / 2]) as f64 / 2.0)
    }
}

/// Percentile « nearest rank » : rang = ceil(p/100 × n), index = rang − 1.
fn percentile_nearest_rank_sorted(sorted: &[u64], p: u8) -> Option<f64> {
    if sorted.is_empty() || p == 0 {
        return None;
    }
    let n = sorted.len();
    let rank = ((p as f64 / 100.0) * n as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(n - 1);
    Some(sorted[idx] as f64)
}

fn median_f64(values: &[f64]) -> Option<f64> {
    let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        Some(v[n / 2])
    } else {
        Some((v[n / 2 - 1] + v[n / 2]) / 2.0)
    }
}

fn values_after_by_phase(
    samples: &[BenchmarkSample],
    phase: BenchmarkPhase,
    pick: impl Fn(&BenchmarkSample) -> Option<f64>,
) -> Vec<f64> {
    samples
        .iter()
        .filter(|s| s.phase == phase && s.success)
        .filter_map(pick)
        .filter(|x| x.is_finite())
        .collect()
}

/// Gain quand une baisse de la métrique est « mieux » (RAM, % GPU, % CPU après sonde).
fn gain_pct_lower_is_better(off_vals: &[f64], on_vals: &[f64], min_positive: f64) -> Option<f64> {
    let off_m = median_f64(off_vals)?;
    let on_m = median_f64(on_vals)?;
    if off_m < min_positive {
        return None;
    }
    Some(((off_m - on_m) / off_m) * 100.0)
}

pub fn compute_summary(samples: &[BenchmarkSample]) -> BenchmarkSummary {
    let off: Vec<u64> = samples
        .iter()
        .filter(|s| s.phase == BenchmarkPhase::Off && s.success)
        .map(|s| s.duration_ms)
        .collect();
    let on: Vec<u64> = samples
        .iter()
        .filter(|s| s.phase == BenchmarkPhase::On && s.success)
        .map(|s| s.duration_ms)
        .collect();

    let mut off_sorted = off.clone();
    off_sorted.sort_unstable();
    let mut on_sorted = on.clone();
    on_sorted.sort_unstable();

    let median_off_ms = median_sorted_u64(&off_sorted);
    let median_on_ms = median_sorted_u64(&on_sorted);
    let p95_off_ms = percentile_nearest_rank_sorted(&off_sorted, 95);
    let p95_on_ms = percentile_nearest_rank_sorted(&on_sorted, 95);

    let gain_median_pct = median_off_ms.and_then(|off_ms| {
        median_on_ms.and_then(|on_ms| {
            if off_ms > 0.0 {
                Some(((off_ms - on_ms) / off_ms) * 100.0)
            } else {
                None
            }
        })
    });
    let gain_p95_pct = p95_off_ms.and_then(|off_ms| {
        p95_on_ms.and_then(|on_ms| {
            if off_ms > 0.0 {
                Some(((off_ms - on_ms) / off_ms) * 100.0)
            } else {
                None
            }
        })
    });

    let mem_off = values_after_by_phase(samples, BenchmarkPhase::Off, |s| s.mem_after_gb);
    let mem_on = values_after_by_phase(samples, BenchmarkPhase::On, |s| s.mem_after_gb);
    let gain_mem_median_pct = gain_pct_lower_is_better(&mem_off, &mem_on, 0.05);

    let gpu_off = values_after_by_phase(samples, BenchmarkPhase::Off, |s| s.gpu_after_pct);
    let gpu_on = values_after_by_phase(samples, BenchmarkPhase::On, |s| s.gpu_after_pct);
    let gain_gpu_median_pct = gain_pct_lower_is_better(&gpu_off, &gpu_on, 1.0);

    let cpu_off = values_after_by_phase(samples, BenchmarkPhase::Off, |s| s.cpu_after_pct);
    let cpu_on = values_after_by_phase(samples, BenchmarkPhase::On, |s| s.cpu_after_pct);
    let gain_cpu_median_pct = gain_pct_lower_is_better(&cpu_off, &cpu_on, 2.0);

    let power_off = values_after_by_phase(samples, BenchmarkPhase::Off, |s| s.power_after_watts);
    let power_on = values_after_by_phase(samples, BenchmarkPhase::On, |s| s.power_after_watts);
    let gain_power_median_pct = gain_pct_lower_is_better(&power_off, &power_on, 1.0);

    let sigma_off =
        values_after_by_phase(samples, BenchmarkPhase::Off, |s| s.sigma_effective_after);
    let sigma_on = values_after_by_phase(samples, BenchmarkPhase::On, |s| s.sigma_effective_after);
    let gain_sigma_median_pct = gain_pct_lower_is_better(&sigma_off, &sigma_on, 0.05);

    let cpu_temp_off = values_after_by_phase(samples, BenchmarkPhase::Off, |s| s.cpu_temp_after_c);
    let cpu_temp_on = values_after_by_phase(samples, BenchmarkPhase::On, |s| s.cpu_temp_after_c);
    let gain_cpu_temp_median_pct = gain_pct_lower_is_better(&cpu_temp_off, &cpu_temp_on, 1.0);

    let gpu_temp_off = values_after_by_phase(samples, BenchmarkPhase::Off, |s| s.gpu_temp_after_c);
    let gpu_temp_on = values_after_by_phase(samples, BenchmarkPhase::On, |s| s.gpu_temp_after_c);
    let gain_gpu_temp_median_pct = gain_pct_lower_is_better(&gpu_temp_off, &gpu_temp_on, 1.0);

    let efficiency_score = Some(
        gain_median_pct.unwrap_or(0.0) * 0.45
            + gain_p95_pct.unwrap_or(0.0) * 0.20
            + gain_mem_median_pct.unwrap_or(0.0) * 0.08
            + gain_gpu_median_pct.unwrap_or(0.0) * 0.07
            + gain_cpu_median_pct.unwrap_or(0.0) * 0.07
            + gain_power_median_pct.unwrap_or(0.0) * 0.08
            + gain_sigma_median_pct.unwrap_or(0.0) * 0.05,
    );

    BenchmarkSummary {
        samples_off_ok: off.len(),
        samples_on_ok: on.len(),
        median_off_ms,
        median_on_ms,
        p95_off_ms,
        p95_on_ms,
        gain_median_pct,
        gain_p95_pct,
        gain_mem_median_pct,
        gain_gpu_median_pct,
        gain_cpu_median_pct,
        gain_power_median_pct,
        gain_sigma_median_pct,
        gain_cpu_temp_median_pct,
        gain_gpu_temp_median_pct,
        efficiency_score,
    }
}

impl BenchmarkState {
    pub fn new(path: PathBuf) -> Self {
        let mut state = Self {
            path,
            sessions: Vec::new(),
        };
        let _ = state.load_existing();
        state
    }

    pub fn record_session(&mut self, session: BenchmarkSession) -> Result<(), String> {
        self.sessions.insert(0, session.clone());
        if self.sessions.len() > 200 {
            self.sessions.truncate(200);
        }
        self.append_session(&session)
    }

    pub fn clear(&mut self) -> Result<(), String> {
        self.sessions.clear();
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&self.path, "").map_err(|e| e.to_string())
    }

    pub fn history(
        &self,
        command: Option<&str>,
        args: Option<&[String]>,
        cwd: Option<&str>,
        workload: Option<&str>,
    ) -> BenchmarkHistoryResponse {
        let sessions = self
            .sessions
            .iter()
            .filter(|s| matches_signature(s, command, args, cwd, workload))
            .cloned()
            .collect::<Vec<_>>();
        let last_summary = sessions.first().map(|s| s.summary.clone());
        let advice = compute_tuning_advice(&sessions);
        let top_sessions = ranked_top_sessions(&sessions, 5);
        BenchmarkHistoryResponse {
            sessions,
            last_summary,
            advice,
            top_sessions,
        }
    }

    fn append_session(&self, session: &BenchmarkSession) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| e.to_string())?;
        let line = serde_json::to_string(session).map_err(|e| e.to_string())?;
        writeln!(f, "{line}").map_err(|e| e.to_string())
    }

    fn load_existing(&mut self) -> Result<(), String> {
        let file = match std::fs::File::open(&self.path) {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };
        let reader = BufReader::new(file);
        let mut sessions = Vec::new();
        for line in reader.lines() {
            let line = line.map_err(|e| e.to_string())?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(session) = serde_json::from_str::<BenchmarkSession>(&line) {
                sessions.push(session);
            }
        }
        self.sessions = sessions.into_iter().rev().take(200).collect();
        Ok(())
    }
}

fn matches_signature(
    session: &BenchmarkSession,
    command: Option<&str>,
    args: Option<&[String]>,
    cwd: Option<&str>,
    workload: Option<&str>,
) -> bool {
    if let Some(v) = command {
        if session.command != v {
            return false;
        }
    }
    if let Some(v) = args {
        if session.args.as_slice() != v {
            return false;
        }
    }
    if let Some(v) = cwd {
        if session.cwd.as_deref().unwrap_or("") != v {
            return false;
        }
    }
    if let Some(v) = workload {
        if session.workload != v {
            return false;
        }
    }
    true
}

pub fn compute_tuning_advice(sessions: &[BenchmarkSession]) -> Option<BenchmarkTuningAdvice> {
    let mut candidates = sessions
        .iter()
        .filter(|s| {
            s.summary.samples_off_ok > 0
                && s.summary.samples_on_ok > 0
                && s.summary.gain_median_pct.unwrap_or(f64::MIN).is_finite()
        })
        .collect::<Vec<_>>();
    if candidates.len() < 2 {
        return None;
    }

    candidates.sort_by(|a, b| {
        session_composite_score(b)
            .partial_cmp(&session_composite_score(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let top = candidates.into_iter().take(5).collect::<Vec<_>>();
    let mut weight_sum = 0.0;
    let mut kappa = 0.0;
    let mut sigma_max = 0.0;
    let mut eta = 0.0;
    let mut soulram_percent = 0.0;
    let mut gain_median = 0.0;
    let mut gain_p95 = 0.0;
    let mut efficiency = 0.0;
    let mut composite_score = 0.0;
    let mut policy_safe_weight = 0.0;
    for session in &top {
        let gain_pct = session.summary.gain_median_pct.unwrap_or(0.0);
        let gain_p95_pct = session.summary.gain_p95_pct.unwrap_or(0.0);
        let eff = session.summary.efficiency_score.unwrap_or(0.0);
        let score = session_composite_score(session);
        let weight = score.max(0.25);
        weight_sum += weight;
        kappa += session.kappa * weight;
        sigma_max += session.sigma_max * weight;
        eta += session.eta * weight;
        soulram_percent += session.soulram_percent.unwrap_or(20) as f64 * weight;
        gain_median += gain_pct * weight;
        gain_p95 += gain_p95_pct * weight;
        efficiency += eff * weight;
        composite_score += score * weight;
        if session.policy_mode.as_deref() == Some("safe") {
            policy_safe_weight += weight;
        }
    }
    if weight_sum <= 0.0 {
        return None;
    }

    let confidence = ((top.len() as f64 / 5.0) * sample_confidence(&top)).clamp(0.2, 1.0);
    Some(BenchmarkTuningAdvice {
        sample_size: top.len(),
        recommended_kappa: (kappa / weight_sum * 10.0).round() / 10.0,
        recommended_sigma_max: (sigma_max / weight_sum * 100.0).round() / 100.0,
        recommended_eta: (eta / weight_sum * 100.0).round() / 100.0,
        recommended_policy_mode: if policy_safe_weight >= weight_sum * 0.45 {
            "safe".to_string()
        } else {
            "privileged".to_string()
        },
        recommended_soulram_percent: ((soulram_percent / weight_sum).round() as i64).clamp(10, 60)
            as u8,
        expected_gain_median_pct: Some(gain_median / weight_sum),
        expected_gain_p95_pct: Some(gain_p95 / weight_sum),
        expected_efficiency_score: Some(efficiency / weight_sum),
        confidence,
        composite_score: composite_score / weight_sum,
        basis: "composite_score(median+p95+efficiency+sample_size)".to_string(),
    })
}

fn session_composite_score(session: &BenchmarkSession) -> f64 {
    let median_gain = session.summary.gain_median_pct.unwrap_or(0.0);
    let p95_gain = session.summary.gain_p95_pct.unwrap_or(0.0);
    let efficiency = session.summary.efficiency_score.unwrap_or(0.0);
    let sample_pairs = session
        .summary
        .samples_off_ok
        .min(session.summary.samples_on_ok) as f64;
    let sample_factor = sample_pairs.sqrt().clamp(1.0, 4.5);

    let median_term = median_gain.max(-25.0) * 0.65;
    let p95_term = p95_gain.max(-30.0) * 0.35;
    let stability_penalty = if p95_gain + 1.5 < median_gain {
        (median_gain - p95_gain) * 0.15
    } else {
        0.0
    };

    (median_term + p95_term + efficiency * 0.35 - stability_penalty) * sample_factor
}

fn sample_confidence(top: &[&BenchmarkSession]) -> f64 {
    if top.is_empty() {
        return 0.0;
    }
    let avg_pairs = top
        .iter()
        .map(|s| s.summary.samples_off_ok.min(s.summary.samples_on_ok) as f64)
        .sum::<f64>()
        / top.len() as f64;
    (avg_pairs / 10.0).clamp(0.35, 1.0)
}

fn ranked_top_sessions(sessions: &[BenchmarkSession], limit: usize) -> Vec<RankedBenchmarkSession> {
    let mut ranked = sessions
        .iter()
        .filter(|s| s.summary.samples_off_ok > 0 && s.summary.samples_on_ok > 0)
        .map(|s| RankedBenchmarkSession {
            rank: 0,
            composite_score: session_composite_score(s),
            started_at: s.started_at.clone(),
            workload: s.workload.clone(),
            runs_per_state: s.runs_per_state,
            kappa: s.kappa,
            sigma_max: s.sigma_max,
            eta: s.eta,
            gain_median_pct: s.summary.gain_median_pct,
            gain_p95_pct: s.summary.gain_p95_pct,
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| {
        b.composite_score
            .partial_cmp(&a.composite_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ranked.truncate(limit);
    for (idx, item) in ranked.iter_mut().enumerate() {
        item.rank = idx + 1;
    }
    ranked
}
