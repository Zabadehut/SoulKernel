//! SoulKernel profiler harness.
//!
//! Modes :
//!   heap  — dhat heap profiler  (feature `dhat-heap`)
//!   cpu   — pprof CPU flamegraph (feature `pprof-cpu`)
//!   both  — les deux
//!
//! Les rapports sont écrits dans `{workspace}/profiling-reports/`.

#![allow(unused_imports, unused_variables, dead_code)]

// ── dhat global allocator (doit être déclaré avant main) ──────────────────
#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use serde_json::json;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

// ── Chemin de sortie ──────────────────────────────────────────────────────

fn reports_dir() -> PathBuf {
    // Remonte jusqu'à la racine workspace depuis target/
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    // exe = …/target/debug/soulkernel-profiler
    // workspace = parent(parent(parent(exe)))
    let workspace = exe
        .ancestors()
        .find(|p| p.join("Cargo.toml").exists())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    workspace.join("profiling-reports")
}

// ── Workload de référence ─────────────────────────────────────────────────
//
// Exercice les chemins d'allocation chauds de soulkernel-core :
//   - formula::compute          (tous les profils)
//   - kpi::compute              (avec métriques mock)
//   - workload_catalog           (chargement de tous les profils)
//
// On n'appelle pas metrics::collect() ni processes::collect_observed_report()
// ici car ils nécessitent un vrai OS / sysinfo.

fn exercise_formula(iters: usize) {
    use soulkernel_core::formula;
    use soulkernel_core::metrics::{RawMetrics, ResourceState};

    let profiles = formula::WorkloadProfile::all();
    let state = ResourceState {
        cpu: 0.45,
        mem: 0.60,
        compression: Some(0.5),
        io_bandwidth: Some(0.3),
        gpu: Some(0.2),
        sigma: 0.35,
        epsilon: [0.05, 0.08, 0.03, 0.04, 0.02],
        raw: RawMetrics {
            cpu_pct: 45.0,
            cpu_clock_mhz: Some(3200.0),
            cpu_max_clock_mhz: Some(4800.0),
            cpu_freq_ratio: Some(0.67),
            cpu_temp_c: Some(65.0),
            mem_used_mb: 12_000,
            mem_total_mb: 32_000,
            ram_clock_mhz: Some(3600.0),
            swap_used_mb: 0,
            swap_total_mb: 16_000,
            zram_used_mb: None,
            io_read_mb_s: Some(50.0),
            io_write_mb_s: Some(20.0),
            gpu_pct: Some(20.0),
            gpu_core_clock_mhz: None,
            gpu_mem_clock_mhz: None,
            gpu_temp_c: Some(55.0),
            gpu_power_watts: Some(48.0),
            gpu_power_source: None,
            gpu_power_confidence: None,
            gpu_mem_used_mb: Some(4096),
            gpu_mem_total_mb: Some(12288),
            gpu_devices: Vec::new(),
            power_watts: Some(180.0),
            power_watts_source: Some("mock_wall".to_string()),
            host_power_watts: Some(180.0),
            host_power_watts_source: Some("mock_host".to_string()),
            wall_power_watts: Some(195.0),
            wall_power_watts_source: Some("mock_meross".to_string()),
            psi_cpu: None,
            psi_mem: None,
            load_avg_1m_norm: Some(0.72),
            runnable_tasks: Some(4),
            on_battery: Some(false),
            battery_percent: None,
            page_faults_per_sec: Some(1200.0),
            platform: "profiler-mock".into(),
            webview_host_cpu_sum: None,
            webview_host_mem_mb: None,
        },
    };

    for _ in 0..iters {
        for profile in &profiles {
            let _r = formula::compute(&state, profile, 2.0, Some(180.0));
        }
    }
}

fn exercise_kpi(iters: usize) {
    use soulkernel_core::device_profile::DeviceProfile;
    use soulkernel_core::kpi;
    use soulkernel_core::metrics::{RawMetrics, ResourceState};
    use soulkernel_core::processes::{ProcessObservedReport, ProcessSample};

    let profile = DeviceProfile::pc();
    let state = ResourceState {
        cpu: 0.45,
        mem: 0.60,
        compression: None,
        io_bandwidth: None,
        gpu: None,
        sigma: 0.35,
        epsilon: [0.05, 0.08, 0.03, 0.04, 0.02],
        raw: RawMetrics {
            cpu_pct: 45.0,
            cpu_clock_mhz: None,
            cpu_max_clock_mhz: None,
            cpu_freq_ratio: None,
            cpu_temp_c: None,
            mem_used_mb: 12_000,
            mem_total_mb: 32_000,
            ram_clock_mhz: None,
            swap_used_mb: 0,
            swap_total_mb: 16_000,
            zram_used_mb: None,
            io_read_mb_s: None,
            io_write_mb_s: None,
            gpu_pct: None,
            gpu_core_clock_mhz: None,
            gpu_mem_clock_mhz: None,
            gpu_temp_c: None,
            gpu_power_watts: None,
            gpu_power_source: None,
            gpu_power_confidence: None,
            gpu_mem_used_mb: None,
            gpu_mem_total_mb: None,
            gpu_devices: Vec::new(),
            power_watts: Some(180.0),
            power_watts_source: Some("mock".to_string()),
            host_power_watts: Some(180.0),
            host_power_watts_source: None,
            wall_power_watts: Some(195.0),
            wall_power_watts_source: None,
            psi_cpu: None,
            psi_mem: None,
            load_avg_1m_norm: None,
            runnable_tasks: None,
            on_battery: None,
            battery_percent: None,
            page_faults_per_sec: Some(1200.0),
            platform: "profiler-mock".into(),
            webview_host_cpu_sum: None,
            webview_host_mem_mb: None,
        },
    };

    let fake_processes = ProcessObservedReport {
        top_processes: vec![
            ProcessSample {
                pid: 1,
                name: "blender.exe".to_string(),
                cpu_usage_pct: 38.0,
                memory_kb: 4_000_000,
                disk_read_bytes: 0,
                disk_written_bytes: 0,
                run_time_s: 300,
                status: "run".to_string(),
                is_self_process: false,
                is_embedded_webview: false,
            },
            ProcessSample {
                pid: 2,
                name: "soulkernel.exe".to_string(),
                cpu_usage_pct: 3.0,
                memory_kb: 120_000,
                disk_read_bytes: 0,
                disk_written_bytes: 0,
                run_time_s: 600,
                status: "run".to_string(),
                is_self_process: true,
                is_embedded_webview: false,
            },
        ],
        summary: Default::default(),
        groups: Vec::new(),
    };

    let mut prev: Option<f64> = None;
    for _ in 0..iters {
        let snap = kpi::compute(
            &state,
            &fake_processes,
            &profile,
            profile.kpi_lambda_default,
            profile.kpi_alpha,
            profile.kpi_beta,
            profile.kpi_gamma,
            prev,
        );
        prev = snap.kpi_penalized;
    }
}

fn exercise_workload_catalog(iters: usize) {
    use soulkernel_core::formula::WorkloadProfile;
    for _ in 0..iters {
        let _all = WorkloadProfile::all();
        let _es = WorkloadProfile::from_name("es");
        let _gamer = WorkloadProfile::from_name("gamer");
        let _ai = WorkloadProfile::from_name("ai");
    }
}

// ── Profils heap ──────────────────────────────────────────────────────────

#[cfg(feature = "dhat-heap")]
fn run_heap_profile(out: &Path) -> PathBuf {
    // Le profiler dhat est déjà actif via l'allocateur global.
    // On crée un Profiler qui capture jusqu'à son drop.
    let report_path = out.join("dhat-heap.json");
    {
        let _prof = dhat::Profiler::builder()
            .file_name(report_path.clone())
            .build();

        // Exercice sur 500 itérations — suffisant pour repérer les fuites
        // et les allocations répétitives sans rendre le profil illisible.
        exercise_formula(500);
        exercise_kpi(500);
        exercise_workload_catalog(200);
    }
    // dhat écrit le fichier au drop du _prof
    report_path
}

#[cfg(not(feature = "dhat-heap"))]
fn run_heap_profile(out: &Path) -> PathBuf {
    eprintln!(
        "⚠  dhat-heap non activé. Relancez avec : cargo run -p soulkernel-profiler --features dhat-heap -- heap"
    );
    out.join("dhat-heap.json")
}

// ── Profil CPU ────────────────────────────────────────────────────────────

#[cfg(feature = "pprof-cpu")]
fn run_cpu_profile(out: &Path) -> PathBuf {
    use pprof::ProfilerGuardBuilder;
    use std::fs::File;

    let svg_path = out.join("flamegraph.svg");
    let proto_path = out.join("profile.pb");

    let guard = ProfilerGuardBuilder::default()
        .frequency(1000)
        .blocklist(&["libc", "libgcc", "pthread"])
        .build()
        .expect("pprof guard");

    // Exercice CPU intensif
    exercise_formula(2000);
    exercise_kpi(2000);
    exercise_workload_catalog(1000);

    if let Ok(report) = guard.report().build() {
        // Flamegraph SVG
        if let Ok(mut f) = File::create(&svg_path) {
            let _ = report.flamegraph(&mut f);
        }
        // Profil protobuf (compatible pprof tool / speedscope)
        if let Ok(mut f) = File::create(&proto_path) {
            if let Ok(profile) = report.pprof() {
                use pprof::protos::Message;
                let mut buf = Vec::new();
                if profile.encode(&mut buf).is_ok() {
                    let _ = f.write_all(&buf);
                }
            }
        }
    }
    svg_path
}

#[cfg(not(feature = "pprof-cpu"))]
fn run_cpu_profile(out: &Path) -> PathBuf {
    eprintln!(
        "⚠  pprof-cpu non activé. Relancez avec : cargo run -p soulkernel-profiler --features pprof-cpu -- cpu"
    );
    out.join("flamegraph.svg")
}

// ── main ──────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("heap");

    let out = reports_dir();
    fs::create_dir_all(&out).expect("create profiling-reports/");

    let t0 = Instant::now();
    let mut report = serde_json::Map::new();
    report.insert(
        "started_at".into(),
        json!(chrono::Local::now().to_rfc3339()),
    );
    report.insert("mode".into(), json!(mode));

    match mode {
        "heap" | "both" => {
            let path = run_heap_profile(&out);
            println!("heap  → {}", path.display());
            report.insert("heap_profile".into(), json!(path.to_string_lossy()));
        }
        _ => {}
    }

    if mode == "cpu" || mode == "both" {
        let path = run_cpu_profile(&out);
        println!("cpu   → {}", path.display());
        report.insert("cpu_flamegraph".into(), json!(path.to_string_lossy()));
    }

    let elapsed_ms = t0.elapsed().as_millis() as u64;
    report.insert("elapsed_ms".into(), json!(elapsed_ms));
    report.insert(
        "reports_dir".into(),
        json!(out.to_string_lossy()),
    );

    // Résumé JSON lisible par le serveur MCP
    let meta_path = out.join("profiler-meta.json");
    if let Ok(mut f) = fs::File::create(&meta_path) {
        let _ = writeln!(
            f,
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Object(report))
                .unwrap_or_default()
        );
    }

    println!("done  → {} ({elapsed_ms} ms)", out.display());
}
