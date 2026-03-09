use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;

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
    pub samples: Vec<BenchmarkSample>,
    pub summary: BenchmarkSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkTuningAdvice {
    pub sample_size: usize,
    pub recommended_kappa: f64,
    pub recommended_sigma_max: f64,
    pub recommended_eta: f64,
    pub expected_gain_median_pct: Option<f64>,
    pub expected_gain_p95_pct: Option<f64>,
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

fn percentile(values: &[u64], p: u8) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let idx = (((p as usize) * sorted.len()).div_ceil(100)).saturating_sub(1);
    sorted
        .get(idx.min(sorted.len().saturating_sub(1)))
        .map(|v| *v as f64)
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

    let median_off_ms = percentile(&off, 50);
    let median_on_ms = percentile(&on, 50);
    let p95_off_ms = percentile(&off, 95);
    let p95_on_ms = percentile(&on, 95);

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

    BenchmarkSummary {
        samples_off_ok: off.len(),
        samples_on_ok: on.len(),
        median_off_ms,
        median_on_ms,
        p95_off_ms,
        p95_on_ms,
        gain_median_pct,
        gain_p95_pct,
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
    let mut gain_median = 0.0;
    let mut gain_p95 = 0.0;
    let mut composite_score = 0.0;
    for session in &top {
        let gain_pct = session.summary.gain_median_pct.unwrap_or(0.0);
        let gain_p95_pct = session.summary.gain_p95_pct.unwrap_or(0.0);
        let score = session_composite_score(session);
        let weight = score.max(0.25);
        weight_sum += weight;
        kappa += session.kappa * weight;
        sigma_max += session.sigma_max * weight;
        eta += session.eta * weight;
        gain_median += gain_pct * weight;
        gain_p95 += gain_p95_pct * weight;
        composite_score += score * weight;
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
        expected_gain_median_pct: Some(gain_median / weight_sum),
        expected_gain_p95_pct: Some(gain_p95 / weight_sum),
        confidence,
        composite_score: composite_score / weight_sum,
        basis: "composite_score(median+p95+sample_size)".to_string(),
    })
}

fn session_composite_score(session: &BenchmarkSession) -> f64 {
    let median_gain = session.summary.gain_median_pct.unwrap_or(0.0);
    let p95_gain = session.summary.gain_p95_pct.unwrap_or(0.0);
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

    (median_term + p95_term - stability_penalty) * sample_factor
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
