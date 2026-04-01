use serde::{Deserialize, Serialize};
use sysinfo::{get_current_pid, ProcessStatus, System};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSample {
    pub pid: u32,
    pub name: String,
    pub cpu_usage_pct: f64,
    pub memory_kb: u64,
    pub disk_read_bytes: u64,
    pub disk_written_bytes: u64,
    pub run_time_s: u64,
    pub status: String,
    pub is_self_process: bool,
    pub is_embedded_webview: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessObservedSummary {
    pub process_count: usize,
    pub top_count: usize,
    pub total_cpu_usage_pct: f64,
    pub total_memory_kb: u64,
    pub webview_process_count: usize,
    pub webview_cpu_usage_pct: f64,
    pub webview_memory_kb: u64,
    pub self_process_count: usize,
    pub self_cpu_usage_pct: f64,
    pub self_memory_kb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessObservedReport {
    pub summary: ProcessObservedSummary,
    pub top_processes: Vec<ProcessSample>,
}

pub fn is_embedded_webview_name(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("msedgewebview")
        || n.contains("webview2")
        || n.contains("webkitnetworkprocess")
        || n.contains("webkit.webcontent")
        || n.contains("webkitwebprocess")
        || (n.contains("webkit") && n.contains("gpu"))
}

fn status_label(status: ProcessStatus) -> String {
    format!("{status:?}").to_lowercase()
}

pub fn collect_observed_report(top_n: usize) -> ProcessObservedReport {
    let mut sys = System::new_all();
    sys.refresh_all();

    let self_pid = get_current_pid().ok();
    let mut samples = Vec::with_capacity(sys.processes().len());
    let mut summary = ProcessObservedSummary::default();

    for (pid, proc_) in sys.processes() {
        let name = proc_.name().to_string();
        let is_self = Some(*pid) == self_pid;
        let is_webview = is_embedded_webview_name(&name);
        let cpu_usage_pct = (proc_.cpu_usage() as f64).max(0.0);
        let memory_kb = proc_.memory() / 1024;
        let du = proc_.disk_usage();
        let sample = ProcessSample {
            pid: pid.as_u32(),
            name,
            cpu_usage_pct,
            memory_kb,
            disk_read_bytes: du.total_read_bytes,
            disk_written_bytes: du.total_written_bytes,
            run_time_s: proc_.run_time(),
            status: status_label(proc_.status()),
            is_self_process: is_self,
            is_embedded_webview: is_webview,
        };
        summary.process_count += 1;
        summary.total_cpu_usage_pct += sample.cpu_usage_pct;
        summary.total_memory_kb = summary.total_memory_kb.saturating_add(sample.memory_kb);
        if sample.is_self_process {
            summary.self_process_count += 1;
            summary.self_cpu_usage_pct += sample.cpu_usage_pct;
            summary.self_memory_kb = summary.self_memory_kb.saturating_add(sample.memory_kb);
        }
        if sample.is_embedded_webview {
            summary.webview_process_count += 1;
            summary.webview_cpu_usage_pct += sample.cpu_usage_pct;
            summary.webview_memory_kb = summary.webview_memory_kb.saturating_add(sample.memory_kb);
        }
        samples.push(sample);
    }

    samples.sort_by(|a, b| {
        b.cpu_usage_pct
            .partial_cmp(&a.cpu_usage_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.memory_kb.cmp(&a.memory_kb))
    });
    samples.truncate(top_n);
    summary.top_count = samples.len();

    ProcessObservedReport {
        summary,
        top_processes: samples,
    }
}
