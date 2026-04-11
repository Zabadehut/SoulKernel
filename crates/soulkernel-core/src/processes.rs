use serde::{Deserialize, Serialize};
use sysinfo::{get_current_pid, CpuRefreshKind, ProcessRefreshKind, ProcessStatus, RefreshKind, System};

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
    /// Number of bridge/helper processes named "python" or "python3" that are not the current
    /// process. More than 1 indicates a likely bridge accumulation leak.
    pub bridge_python_count: usize,
    /// True if the Windows Memory Compression process was detected.
    pub memory_compression_active: bool,
}

/// Aggregated view of all processes sharing the same executable name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessGroup {
    pub name: String,
    pub instance_count: usize,
    pub total_cpu_pct: f64,
    pub total_memory_kb: u64,
    /// PID of the instance with the highest CPU usage (dome target candidate).
    pub top_pid: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessObservedReport {
    pub summary: ProcessObservedSummary,
    pub top_processes: Vec<ProcessSample>,
    /// Processes grouped by executable name, sorted by aggregate CPU descending.
    pub groups: Vec<ProcessGroup>,
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

/// Returns true if a process status string indicates the process has exited / is a zombie.
fn is_dead_status(status: &str) -> bool {
    // sysinfo formats ProcessStatus via Debug — covers "Dead", "Zombie", "Unknown(n)" variants
    // as well as any future platform-specific terminated states.
    status == "dead"
        || status == "zombie"
        || status.starts_with("unknown(")
        || status == "stopped"
}

/// Normalise a process name for grouping — strip .exe suffix and lowercase.
fn group_key(name: &str) -> String {
    name.trim_end_matches(".exe").to_ascii_lowercase()
}

pub fn collect_observed_report(top_n: usize) -> ProcessObservedReport {
    let mut sys = System::new_with_specifics(
        RefreshKind::new()
            .with_cpu(CpuRefreshKind::new().with_cpu_usage())
            .with_processes(
                ProcessRefreshKind::new()
                    .with_memory()
                    .with_cpu()
                    .with_disk_usage(),
            ),
    );
    sys.refresh_processes();
    // sys.cpus() peut retourner une liste vide sur Windows si le CPU list n'a pas été
    // explicitement rafraîchi (sysinfo 0.30 ne le peuple pas via with_cpu_usage seul).
    // available_parallelism() est l'API OS la plus fiable pour le nombre de threads logiques.
    let logical_cores = {
        let n = sys.cpus().len();
        if n > 0 {
            n
        } else {
            std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
        }
    } as f64;

    let self_pid = get_current_pid().ok();
    let mut all_samples: Vec<ProcessSample> = Vec::with_capacity(sys.processes().len());
    let mut summary = ProcessObservedSummary::default();

    for (pid, proc_) in sys.processes() {
        let name = proc_.name().to_string();
        let status = status_label(proc_.status());
        let is_self = Some(*pid) == self_pid;
        let is_webview = is_embedded_webview_name(&name);
        // sysinfo process CPU is per-core and can exceed 100 on multi-core machines.
        // Normalize it to the same 0..100 system-wide scale used by metrics.raw.cpu_pct.
        let cpu_usage_pct = ((proc_.cpu_usage() as f64).max(0.0) / logical_cores).max(0.0);
        let memory_kb = proc_.memory() / 1024;
        let du = proc_.disk_usage();

        // Count all processes (including dead) for accurate totals.
        summary.process_count += 1;
        summary.total_cpu_usage_pct += cpu_usage_pct;
        summary.total_memory_kb = summary.total_memory_kb.saturating_add(memory_kb);

        // Bridge python leak detection — count non-self python instances.
        let lname = name.to_ascii_lowercase();
        if !is_self && (lname == "python.exe" || lname == "python3" || lname == "python") {
            summary.bridge_python_count += 1;
        }

        // Windows Memory Compression process.
        if lname == "memory compression" || lname == "memcompression" {
            summary.memory_compression_active = true;
        }

        let sample = ProcessSample {
            pid: pid.as_u32(),
            name,
            cpu_usage_pct,
            memory_kb,
            disk_read_bytes: du.total_read_bytes,
            disk_written_bytes: du.total_written_bytes,
            run_time_s: proc_.run_time(),
            status,
            is_self_process: is_self,
            is_embedded_webview: is_webview,
        };

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
        all_samples.push(sample);
    }

    // Build groups from all live (non-dead) samples.
    let mut group_map: std::collections::HashMap<String, ProcessGroup> =
        std::collections::HashMap::new();
    for s in &all_samples {
        if is_dead_status(&s.status) {
            continue;
        }
        let key = group_key(&s.name);
        let entry = group_map.entry(key).or_insert_with(|| ProcessGroup {
            name: s.name.clone(),
            instance_count: 0,
            total_cpu_pct: 0.0,
            total_memory_kb: 0,
            top_pid: s.pid,
        });
        entry.instance_count += 1;
        entry.total_cpu_pct += s.cpu_usage_pct;
        entry.total_memory_kb = entry.total_memory_kb.saturating_add(s.memory_kb);
        if s.cpu_usage_pct > 0.0 {
            // Track highest-CPU PID as the primary dome target for this group.
            // We do a simple last-wins for equal CPU — good enough.
            let current_top_cpu = all_samples
                .iter()
                .find(|p| p.pid == entry.top_pid)
                .map(|p| p.cpu_usage_pct)
                .unwrap_or(0.0);
            if s.cpu_usage_pct >= current_top_cpu {
                entry.top_pid = s.pid;
            }
        }
    }
    let mut groups: Vec<ProcessGroup> = group_map.into_values().collect();
    groups.sort_by(|a, b| {
        b.total_cpu_pct
            .partial_cmp(&a.total_cpu_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.total_memory_kb.cmp(&a.total_memory_kb))
    });

    // Top-N for the flat list: live processes first, sorted by CPU then RAM.
    all_samples.sort_by(|a, b| {
        // Put dead processes last regardless.
        let a_dead = is_dead_status(&a.status);
        let b_dead = is_dead_status(&b.status);
        match (a_dead, b_dead) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => b
                .cpu_usage_pct
                .partial_cmp(&a.cpu_usage_pct)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.memory_kb.cmp(&a.memory_kb)),
        }
    });
    // Only keep top_n live processes in the flat list.
    let top_processes: Vec<ProcessSample> = all_samples
        .into_iter()
        .filter(|s| !is_dead_status(&s.status))
        .take(top_n)
        .collect();
    summary.top_count = top_processes.len();

    ProcessObservedReport {
        summary,
        top_processes,
        groups,
    }
}
