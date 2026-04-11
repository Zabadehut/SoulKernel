use flate2::read::GzDecoder;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SERVER_NAME: &str = "soulkernel-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
const ACTIVE_FRESHNESS_MS: u64 = 30_000;
const DEFAULT_SAMPLE_LIMIT: usize = 120;
const MAX_SAMPLE_LIMIT: usize = 2_000;

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    while let Some(message) = read_message(&mut reader)? {
        if let Some(response) = handle_message(message) {
            write_message(&mut writer, &response)?;
            writer.flush()?;
        }
    }

    Ok(())
}

fn handle_message(message: Value) -> Option<Value> {
    let id = message.get("id").cloned();
    let method = message.get("method").and_then(Value::as_str)?;

    match method {
        "initialize" => Some(ok_response(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": SERVER_VERSION
                }
            }),
        )),
        "notifications/initialized" => None,
        "ping" => Some(ok_response(id, json!({}))),
        "tools/list" => Some(ok_response(id, json!({ "tools": tools_manifest() }))),
        "tools/call"              => Some(handle_tool_call(id, message.get("params"))),
        _ => id.map(|req_id| error_response(req_id, -32601, format!("Method not found: {method}"))),
    }
}

fn handle_tool_call(id: Option<Value>, params: Option<&Value>) -> Value {
    let Some(req_id) = id else {
        return error_response(Value::Null, -32600, "Missing id".to_string());
    };

    let name = params
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let args = params
        .and_then(|p| p.get("arguments"))
        .cloned()
        .unwrap_or_else(|| json!({}));

    let result = match name {
        "get_live_report"             => tool_get_live_report(&args),
        "get_metric_snapshot"         => tool_get_metric_snapshot(&args),
        "get_timeline_samples"        => tool_get_timeline_samples(&args),
        "get_observability_status"    => tool_get_observability_status(&args),
        "get_project_bridge_status"   => tool_get_project_bridge_status(&args),
        "get_supervisor_launch_config"=> tool_get_supervisor_launch_config(&args),
        // ── Profiling pipeline ──────────────────────────────────────────────
        "run_rust_profiler"           => tool_run_rust_profiler(&args),
        "run_memlab"                  => tool_run_memlab(&args),
        "get_profiling_reports"       => tool_get_profiling_reports(&args),
        "get_profiling_summary"       => tool_get_profiling_summary(&args),
        other => Err(format!("Unknown tool: {other}")),
    };

    match result {
        Ok(payload) => ok_response(
            Some(req_id),
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
                }],
                "structuredContent": payload,
                "isError": false
            }),
        ),
        Err(err) => ok_response(
            Some(req_id),
            json!({
                "content": [{
                    "type": "text",
                    "text": err
                }],
                "isError": true
            }),
        ),
    }
}

fn tool_get_live_report(args: &Value) -> Result<Value, String> {
    let include_archives = args
        .get("include_archives")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let state = ObservabilityStore::discover()?;
    let report = state
        .latest_sample(include_archives)?
        .ok_or_else(|| "No live report found in observability files".to_string())?;
    Ok(report)
}

fn tool_get_metric_snapshot(args: &Value) -> Result<Value, String> {
    let include_archives = args
        .get("include_archives")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let state = ObservabilityStore::discover()?;
    let report = state
        .latest_sample(include_archives)?
        .ok_or_else(|| "No live report found in observability files".to_string())?;
    Ok(build_metric_snapshot(&state, &report))
}

fn tool_get_timeline_samples(args: &Value) -> Result<Value, String> {
    let include_archives = args
        .get("include_archives")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let since_ms = args.get("since_ms").and_then(Value::as_u64);
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(DEFAULT_SAMPLE_LIMIT)
        .min(MAX_SAMPLE_LIMIT);
    let state = ObservabilityStore::discover()?;
    let mut samples = state.read_all_samples(include_archives)?;
    if let Some(since_ms) = since_ms {
        samples.retain(|sample| sample_ts(sample) >= since_ms);
    }
    if samples.len() > limit {
        let drain = samples.len() - limit;
        samples.drain(0..drain);
    }
    Ok(json!({
        "path": state.active_path,
        "archives": state.archives,
        "count": samples.len(),
        "samples": samples
    }))
}

fn tool_get_observability_status(_args: &Value) -> Result<Value, String> {
    let state = ObservabilityStore::discover()?;
    let latest = state.latest_sample(true)?;
    let latest_ts = latest.as_ref().map(sample_ts);
    let latest_snapshot = latest
        .as_ref()
        .map(|sample| build_metric_snapshot(&state, sample))
        .unwrap_or_else(|| json!(null));
    Ok(json!({
        "active_path": state.active_path,
        "active_exists": state.active_exists,
        "active_size_bytes": state.active_size_bytes,
        "active_modified_ms": state.active_modified_ms,
        "archives": state.archives,
        "archive_count": state.archive_count,
        "latest_sample_ts_ms": latest_ts,
        "is_fresh": latest_ts.map(|ts| now_ms().saturating_sub(ts) <= ACTIVE_FRESHNESS_MS).unwrap_or(false),
        "rotation_bytes": 16 * 1024 * 1024_u64,
        "latest_snapshot": latest_snapshot
    }))
}

fn tool_get_project_bridge_status(_args: &Value) -> Result<Value, String> {
    let observability = ObservabilityStore::discover()?;
    let latest = observability.latest_sample(true)?;
    let latest_ts = latest.as_ref().map(sample_ts);
    let supervisor = SupervisorProject::discover();
    let latest_snapshot = latest
        .as_ref()
        .map(|sample| build_metric_snapshot(&observability, sample))
        .unwrap_or_else(|| json!(null));

    Ok(json!({
        "soulkernel": {
            "workspace_root": workspace_root().to_string_lossy(),
            "observability_path": observability.active_path,
            "observability_exists": observability.active_exists,
            "archive_count": observability.archive_count,
            "latest_sample_ts_ms": latest_ts,
            "is_fresh": latest_ts.map(|ts| now_ms().saturating_sub(ts) <= ACTIVE_FRESHNESS_MS).unwrap_or(false),
        },
        "supervisor": supervisor,
        "bridge_ready": supervisor.exists && observability.active_exists,
        "latest_snapshot": latest_snapshot,
    }))
}

fn tool_get_supervisor_launch_config(args: &Value) -> Result<Value, String> {
    let observability = ObservabilityStore::discover()?;
    let supervisor = SupervisorProject::discover();
    let telemetry_dir = PathBuf::from(&observability.active_path)
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| "Unable to derive telemetry directory".to_string())?;
    let port = args
        .get("port")
        .and_then(Value::as_u64)
        .unwrap_or(8787);

    let mut commands = BTreeMap::new();
    if supervisor.exists {
        let root = supervisor.root.clone().unwrap_or_default();
        commands.insert(
            "docker".to_string(),
            format!(
                "cd {root} && SOULKERNEL_TELEMETRY_DIR=\"{}\" SOULKERNEL_DASHBOARD_PORT={port} docker compose up --build -d",
                telemetry_dir.to_string_lossy()
            ),
        );
        commands.insert(
            "node".to_string(),
            format!(
                "cd {root} && PORT={port} SOULKERNEL_OBSERVABILITY_PATH=\"{}\" npm run dev",
                observability.active_path
            ),
        );
    }

    Ok(json!({
        "supervisor": supervisor,
        "telemetry_dir": telemetry_dir,
        "observability_path": observability.active_path,
        "recommended_port": port,
        "env": {
            "SOULKERNEL_TELEMETRY_DIR": telemetry_dir,
            "SOULKERNEL_OBSERVABILITY_PATH": observability.active_path,
            "SOULKERNEL_DASHBOARD_PORT": port,
            "PORT": port,
        },
        "commands": commands,
        "notes": [
            "Mode recommandé: le superviseur lit le dossier telemetry du client en lecture seule.",
            "Pour un serveur distant, synchronisez ou montez le dossier telemetry du client vers la machine de supervision."
        ]
    }))
}

// ═══════════════════════════════════════════════════════════════════════════
// ── Profiling pipeline tools ────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════

/// Chemin vers la racine du workspace depuis l'emplacement du binaire MCP.
fn profiling_workspace_root() -> PathBuf {
    std::env::var_os("SOULKERNEL_WORKSPACE")
        .map(PathBuf::from)
        .or_else(|| {
            // Remonte depuis target/ jusqu'au dossier contenant Cargo.toml
            std::env::current_exe().ok().and_then(|exe| {
                exe.ancestors()
                    .find(|p| p.join("Cargo.toml").exists())
                    .map(PathBuf::from)
            })
        })
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn profiling_reports_dir() -> PathBuf {
    profiling_workspace_root().join("profiling-reports")
}

fn memlab_dir() -> PathBuf {
    profiling_workspace_root().join("tools").join("memlab")
}

/// Exécute une commande, retourne (exit_code, stdout, stderr).
fn run_cmd(
    program: &str,
    args: &[&str],
    cwd: &Path,
    timeout_s: u64,
) -> (i32, String, String) {
    use std::process::Command;
    use std::time::Instant;

    let deadline = Instant::now() + Duration::from_secs(timeout_s);

    let mut child = match Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (-1, String::new(), e.to_string()),
    };

    // Attente bornée par timeout
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    return (-2, String::new(), format!("timeout after {timeout_s}s"));
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            Err(e) => return (-1, String::new(), e.to_string()),
        }
    }

    let out = child.wait_with_output().unwrap_or_else(|_| {
        std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    });

    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

// ── tool: run_rust_profiler ───────────────────────────────────────────────

fn tool_run_rust_profiler(args: &Value) -> Result<Value, String> {
    let mode = args
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("heap");
    let features = match mode {
        "heap" => "dhat-heap",
        "cpu"  => "pprof-cpu",
        "both" => "dhat-heap,pprof-cpu",
        other  => return Err(format!("mode invalide: {other} (heap|cpu|both)")),
    };
    let timeout_s = args
        .get("timeout_s")
        .and_then(Value::as_u64)
        .unwrap_or(120);

    let workspace = profiling_workspace_root();
    let (code, stdout, stderr) = run_cmd(
        "cargo",
        &[
            "run",
            "-p", "soulkernel-profiler",
            "--features", features,
            "--",
            mode,
        ],
        &workspace,
        timeout_s,
    );

    let reports_dir = profiling_reports_dir();
    let meta = read_profiler_meta(&reports_dir);

    Ok(json!({
        "mode": mode,
        "features": features,
        "exit_code": code,
        "stdout": stdout.lines().rev().take(20).collect::<Vec<_>>(),
        "stderr_tail": stderr.lines().rev().take(10).collect::<Vec<_>>(),
        "reports_dir": reports_dir.to_string_lossy(),
        "meta": meta,
        "available_reports": list_report_files(&reports_dir),
    }))
}

fn read_profiler_meta(dir: &Path) -> Value {
    let path = dir.join("profiler-meta.json");
    if let Ok(content) = std::fs::read_to_string(&path) {
        serde_json::from_str(&content).unwrap_or(Value::Null)
    } else {
        Value::Null
    }
}

fn list_report_files(dir: &Path) -> Vec<Value> {
    if !dir.exists() { return Vec::new(); }
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| {
            let p = e.path();
            let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            json!({
                "name": p.file_name().unwrap_or_default().to_string_lossy(),
                "path": p.to_string_lossy(),
                "size_bytes": size,
            })
        })
        .collect()
}

// ── tool: run_memlab ─────────────────────────────────────────────────────

fn tool_run_memlab(args: &Value) -> Result<Value, String> {
    let scenario = args
        .get("scenario")
        .and_then(Value::as_str)
        .unwrap_or("all");
    let dashboard_url = args
        .get("dashboard_url")
        .and_then(Value::as_str)
        .unwrap_or("http://localhost:8787");
    let app_url = args
        .get("app_url")
        .and_then(Value::as_str)
        .unwrap_or("http://localhost:1420");
    let timeout_s = args
        .get("timeout_s")
        .and_then(Value::as_u64)
        .unwrap_or(300);

    let memlab = memlab_dir();
    if !memlab.exists() {
        return Err(format!(
            "Répertoire MemLab introuvable : {}",
            memlab.display()
        ));
    }

    // npm install si node_modules absent
    if !memlab.join("node_modules").exists() {
        let (code, _, stderr) = run_cmd("npm", &["install"], &memlab, 120);
        if code != 0 {
            return Err(format!("npm install échoué (code {code}): {stderr}"));
        }
    }

    let (code, stdout, stderr) = run_cmd(
        "bash",
        &["run.sh", scenario],
        &memlab,
        timeout_s,
    );

    // Lecture du résumé JSON généré par run.sh
    let summary_path = memlab.join("memlab-reports").join("summary.json");
    let summary = if summary_path.exists() {
        std::fs::read_to_string(&summary_path)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .unwrap_or(Value::Null)
    } else {
        Value::Null
    };

    Ok(json!({
        "scenario": scenario,
        "exit_code": code,
        "stdout_tail": stdout.lines().rev().take(30).collect::<Vec<_>>(),
        "stderr_tail": stderr.lines().rev().take(10).collect::<Vec<_>>(),
        "summary": summary,
        "reports_dir": memlab.join("memlab-reports").to_string_lossy(),
        "urls_tested": {
            "dashboard": dashboard_url,
            "app": app_url,
        }
    }))
}

// ── tool: get_profiling_reports ──────────────────────────────────────────

fn tool_get_profiling_reports(_args: &Value) -> Result<Value, String> {
    let rust_dir = profiling_reports_dir();
    let memlab_dir = memlab_dir().join("memlab-reports");

    let rust_files = list_report_files(&rust_dir);
    let meta = read_profiler_meta(&rust_dir);

    // Résumé MemLab
    let memlab_summary = {
        let path = memlab_dir.join("summary.json");
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        }
    };

    // Lecture dhat-heap.json (extrait les top allocations)
    let dhat_summary = {
        let p = rust_dir.join("dhat-heap.json");
        if p.exists() {
            std::fs::read_to_string(&p)
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                .map(|v| {
                    json!({
                        "total_bytes": v.get("totalBytes"),
                        "total_blocks": v.get("totalBlocks"),
                        "peak_bytes": v.get("peakBytes"),
                        "at_t_gmax_bytes": v.get("atTGmaxBytes"),
                    })
                })
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        }
    };

    Ok(json!({
        "rust": {
            "reports_dir": rust_dir.to_string_lossy(),
            "files": rust_files,
            "meta": meta,
            "dhat_summary": dhat_summary,
            "flamegraph_exists": rust_dir.join("flamegraph.svg").exists(),
            "proto_exists": rust_dir.join("profile.pb").exists(),
        },
        "frontend": {
            "memlab_dir": memlab_dir.to_string_lossy(),
            "summary": memlab_summary,
        }
    }))
}

// ── tool: get_profiling_summary ──────────────────────────────────────────

fn tool_get_profiling_summary(_args: &Value) -> Result<Value, String> {
    let rust_dir = profiling_reports_dir();
    let memlab_reports = memlab_dir().join("memlab-reports");

    let heap_exists = rust_dir.join("dhat-heap.json").exists();
    let flamegraph_exists = rust_dir.join("flamegraph.svg").exists();
    let memlab_summary_exists = memlab_reports.join("summary.json").exists();

    let dhat_peak = if heap_exists {
        std::fs::read_to_string(rust_dir.join("dhat-heap.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|v| v.get("peakBytes").and_then(Value::as_u64))
    } else {
        None
    };

    let memlab_leaks: Option<u64> = if memlab_summary_exists {
        std::fs::read_to_string(memlab_reports.join("summary.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|v| {
                let scenarios = v.get("scenarios")?.as_object()?;
                Some(
                    scenarios
                        .values()
                        .filter_map(|s| s.get("leak_count").and_then(Value::as_u64))
                        .sum(),
                )
            })
    } else {
        None
    };

    // Commandes utiles affichées dans la réponse MCP
    let workspace = profiling_workspace_root();
    let commands = json!({
        "heap_profile": format!(
            "cargo run -p soulkernel-profiler --features dhat-heap -- heap"
        ),
        "cpu_profile": format!(
            "cargo run -p soulkernel-profiler --features pprof-cpu -- cpu"
        ),
        "both_profiles": format!(
            "cargo run -p soulkernel-profiler --features dhat-heap,pprof-cpu -- both"
        ),
        "memlab_dashboard": "cd tools/memlab && npm run check:dashboard",
        "memlab_app": "cd tools/memlab && npm run check:app",
        "memlab_all": "cd tools/memlab && npm run check:all",
        "view_flamegraph": format!(
            "open {}", workspace.join("profiling-reports/flamegraph.svg").display()
        ),
        "view_dhat": format!(
            "npx dh {} {}", // dhat viewer: npx dh <heap.json>
            workspace.join("profiling-reports/dhat-heap.json").display(),
            ""
        ),
    });

    Ok(json!({
        "rust_profiling": {
            "heap_profile_available": heap_exists,
            "cpu_flamegraph_available": flamegraph_exists,
            "peak_heap_bytes": dhat_peak,
            "peak_heap_mb": dhat_peak.map(|b| b as f64 / 1_048_576.0),
            "reports_dir": rust_dir.to_string_lossy(),
        },
        "frontend_profiling": {
            "memlab_summary_available": memlab_summary_exists,
            "total_leaks_detected": memlab_leaks,
            "scenarios_dir": memlab_dir().join("scenarios").to_string_lossy(),
        },
        "quick_commands": commands,
        "notes": [
            "heap  : dhat-rs — profil d'allocations Rust (callstack + taille par site)",
            "cpu   : pprof   — flamegraph CPU, compatible pprof tool et speedscope",
            "memlab: scenarios/live-dashboard.ts (port 8787) + app-shell.ts (port 1420)",
        ]
    }))
}

// ─────────────────────────────────────────────────────────────────────────────

fn build_metric_snapshot(state: &ObservabilityStore, report: &Value) -> Value {
    let raw = report.pointer("/report/metrics/raw").cloned().unwrap_or(Value::Null);
    let kpi = report.get("kpi").cloned().unwrap_or(Value::Null);
    let telemetry = report.pointer("/report/telemetry").cloned().unwrap_or(Value::Null);
    let external = report.get("external_power").cloned().unwrap_or(Value::Null);
    let report_node = report.get("report").cloned().unwrap_or(Value::Null);

    json!({
        "exported_at": report.pointer("/report/exported_at"),
        "exported_at_ms": report.pointer("/report/exported_at_ms"),
        "workload": report.pointer("/report/workload"),
        "dome_active": report.pointer("/report/dome_active"),
        "soulram_active": report.pointer("/report/soulram_active"),
        "target_pid": report.pointer("/report/target_pid"),
        "raw_metrics": {
            "cpu_pct": raw.get("cpu_pct"),
            "mem_used_mb": raw.get("mem_used_mb"),
            "mem_total_mb": raw.get("mem_total_mb"),
            "gpu_pct": raw.get("gpu_pct"),
            "gpu_power_watts": raw.get("gpu_power_watts"),
            "power_watts": raw.get("power_watts"),
            "wall_power_watts": raw.get("wall_power_watts"),
            "io_read_mb_s": raw.get("io_read_mb_s"),
            "io_write_mb_s": raw.get("io_write_mb_s"),
            "page_faults_per_sec": raw.get("page_faults_per_sec"),
        },
        "kpi": {
            "label": kpi.get("label"),
            "kpi_basic_w_per_pct": kpi.get("kpi_basic_w_per_pct"),
            "kpi_penalized_w_per_pct": kpi.get("kpi_penalized_w_per_pct"),
            "cpu_total_pct": kpi.get("cpu_total_pct"),
            "cpu_useful_pct": kpi.get("cpu_useful_pct"),
            "cpu_overhead_pct": kpi.get("cpu_overhead_pct"),
            "cpu_self_pct": kpi.get("cpu_self_pct"),
            "trend": kpi.get("trend"),
        },
        "telemetry": {
            "power_source": telemetry.get("power_source"),
            "live_power_w": telemetry.get("live_power_w"),
            "total_energy_kwh": telemetry.pointer("/total/energy_kwh"),
            "total_cost": telemetry.pointer("/total/cost"),
            "total_co2_kg": telemetry.pointer("/total/co2_kg"),
        },
        "external_power": {
            "source_tag": external.get("source_tag"),
            "last_watts_label": external.get("last_watts_label"),
            "freshness": external.get("freshness"),
            "bridge_state": external.get("bridge_state"),
        },
        "observability": {
            "active_path": state.active_path,
            "archive_count": state.archive_count,
            "active_exists": state.active_exists,
        },
        "report": report_node
    })
}

fn tools_manifest() -> Vec<Value> {
    vec![
        json!({
            "name": "get_live_report",
            "description": "Return the complete latest SoulKernel live report from observability files while soulkernel-lite is running.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "include_archives": {
                        "type": "boolean",
                        "description": "Also scan rotated .jsonl.gz archives when searching the latest report."
                    }
                }
            }
        }),
        json!({
            "name": "get_metric_snapshot",
            "description": "Return a condensed live metric snapshot extracted from the latest complete report.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "include_archives": {
                        "type": "boolean"
                    }
                }
            }
        }),
        json!({
            "name": "get_timeline_samples",
            "description": "Return recent observability samples from the active .jsonl file and optional rotated .jsonl.gz archives.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "include_archives": {
                        "type": "boolean"
                    },
                    "since_ms": {
                        "type": "integer",
                        "description": "Only keep samples newer than this UNIX timestamp in milliseconds."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of samples to return."
                    }
                }
            }
        }),
        json!({
            "name": "get_observability_status",
            "description": "Return observability file paths, archive rotation status, freshness, and the latest condensed snapshot.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "get_project_bridge_status",
            "description": "Return the linkage status between the SoulKernel client workspace and the sibling SoulKernel-Supervisor project.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "get_supervisor_launch_config",
            "description": "Return launch commands and environment variables to start SoulKernel-Supervisor against this client's observability files.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "port": {
                        "type": "integer",
                        "description": "Dashboard port to expose."
                    }
                }
            }
        }),
        // ── Profiling pipeline ───────────────────────────────────────────────
        json!({
            "name": "run_rust_profiler",
            "description": "Build and run the soulkernel-profiler harness. \
                Mode 'heap' runs dhat-heap (memory allocations callstack), \
                'cpu' runs pprof flamegraph, 'both' runs both. \
                Outputs are written to profiling-reports/ at the workspace root.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["heap", "cpu", "both"],
                        "description": "Profiling mode. Default: heap."
                    },
                    "timeout_s": {
                        "type": "integer",
                        "description": "Max seconds to wait for the profiler to finish. Default: 120."
                    }
                }
            }
        }),
        json!({
            "name": "run_memlab",
            "description": "Run MemLab memory leak detection on the SoulKernel front-end. \
                Scenario 'dashboard' targets the live-dashboard (localhost:8787), \
                'app' targets the Svelte dev server (localhost:1420), \
                'all' runs both. Requires the target server(s) to be running.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scenario": {
                        "type": "string",
                        "enum": ["dashboard", "app", "all"],
                        "description": "Which scenario to run. Default: all."
                    },
                    "dashboard_url": {
                        "type": "string",
                        "description": "Override live-dashboard URL. Default: http://localhost:8787."
                    },
                    "app_url": {
                        "type": "string",
                        "description": "Override Svelte app URL. Default: http://localhost:1420."
                    },
                    "timeout_s": {
                        "type": "integer",
                        "description": "Max seconds for the full MemLab run. Default: 300."
                    }
                }
            }
        }),
        json!({
            "name": "get_profiling_reports",
            "description": "List all available profiling report files (dhat-heap.json, flamegraph.svg, \
                profile.pb, memlab summary) with sizes and extracted key metrics.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "get_profiling_summary",
            "description": "Return a high-level summary of all profiling results: \
                peak heap bytes (dhat), flamegraph availability (pprof), \
                and total MemLab leaks detected. Also returns ready-to-run CLI commands.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
    ]
}

#[derive(Serialize)]
struct ObservabilityStore {
    active_path: String,
    active_exists: bool,
    active_size_bytes: u64,
    active_modified_ms: Option<u64>,
    archives: Vec<String>,
    archive_count: usize,
}

#[derive(Serialize)]
struct SupervisorProject {
    exists: bool,
    root: Option<String>,
    package_json_exists: bool,
    dockerfile_exists: bool,
    compose_exists: bool,
    readme_exists: bool,
    package_name: Option<String>,
    scripts: BTreeMap<String, String>,
}

impl SupervisorProject {
    fn discover() -> Self {
        let root = default_supervisor_root();
        let package_path = root.join("package.json");
        let dockerfile_path = root.join("Dockerfile");
        let compose_path = root.join("docker-compose.yml");
        let readme_path = root.join("README.md");
        let package_json_exists = package_path.exists();
        let dockerfile_exists = dockerfile_path.exists();
        let compose_exists = compose_path.exists();
        let readme_exists = readme_path.exists();
        let exists = root.exists() && package_json_exists;
        let mut package_name = None;
        let mut scripts = BTreeMap::new();
        if let Ok(content) = std::fs::read_to_string(&package_path) {
            if let Ok(value) = serde_json::from_str::<Value>(&content) {
                package_name = value
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|s| s.to_string());
                if let Some(obj) = value.get("scripts").and_then(Value::as_object) {
                    for (key, value) in obj {
                        if let Some(script) = value.as_str() {
                            scripts.insert(key.clone(), script.to_string());
                        }
                    }
                }
            }
        }
        Self {
            exists,
            root: root.exists().then(|| root.to_string_lossy().into_owned()),
            package_json_exists,
            dockerfile_exists,
            compose_exists,
            readme_exists,
            package_name,
            scripts,
        }
    }
}

impl ObservabilityStore {
    fn discover() -> Result<Self, String> {
        let path = default_observability_path();
        let meta = std::fs::metadata(&path).ok();
        let active_exists = meta.is_some();
        let active_size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let active_modified_ms = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(system_time_ms);
        let archives = observability_archives(&path)?
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let archive_count = archives.len();
        Ok(Self {
            active_path: path.to_string_lossy().into_owned(),
            active_exists,
            active_size_bytes,
            active_modified_ms,
            archives,
            archive_count,
        })
    }

    fn latest_sample(&self, include_archives: bool) -> Result<Option<Value>, String> {
        let mut latest: Option<Value> = None;
        for sample in self.read_all_samples(include_archives)? {
            let ts = sample_ts(&sample);
            let replace = latest
                .as_ref()
                .map(|current| ts >= sample_ts(current))
                .unwrap_or(true);
            if replace {
                latest = Some(sample);
            }
        }
        Ok(latest)
    }

    fn read_all_samples(&self, include_archives: bool) -> Result<Vec<Value>, String> {
        let active_path = PathBuf::from(&self.active_path);
        let mut samples = Vec::new();
        if active_path.exists() {
            samples.extend(read_jsonl_file(&active_path)?);
        }
        if include_archives {
            for archive in observability_archives(&active_path)? {
                samples.extend(read_gzip_jsonl_file(&archive)?);
            }
        }
        samples.sort_by_key(sample_ts);
        Ok(samples)
    }
}

fn default_observability_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata)
            .join("SoulKernel")
            .join("telemetry")
            .join("observability_samples.jsonl");
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(xdg)
                .join("SoulKernel")
                .join("telemetry")
                .join("observability_samples.jsonl");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("SoulKernel")
                .join("telemetry")
                .join("observability_samples.jsonl");
        }
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("soulkernel_observability_samples.jsonl")
}

fn workspace_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn default_supervisor_root() -> PathBuf {
    if let Some(root) = std::env::var_os("SOULKERNEL_SUPERVISOR_ROOT") {
        return PathBuf::from(root);
    }
    workspace_root()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("SoulKernel-Supervisor")
}

fn observability_archives(path: &Path) -> Result<Vec<PathBuf>, String> {
    let Some(parent) = path.parent() else {
        return Ok(Vec::new());
    };
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("observability_samples");
    let mut archives = std::fs::read_dir(parent)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|entry| {
            entry
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with(stem) && name.ends_with(".jsonl.gz"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    archives.sort();
    Ok(archives)
}

fn read_jsonl_file(path: &Path) -> Result<Vec<Value>, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let reader = BufReader::new(file);
    parse_jsonl_reader(reader, path)
}

fn read_gzip_jsonl_file(path: &Path) -> Result<Vec<Value>, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let decoder = GzDecoder::new(file);
    let reader = BufReader::new(decoder);
    parse_jsonl_reader(reader, path)
}

fn parse_jsonl_reader<R: BufRead>(reader: R, path: &Path) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| format!("{}: {e}", path.display()))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value =
            serde_json::from_str(trimmed).map_err(|e| format!("{}: {e}", path.display()))?;
        out.push(value);
    }
    Ok(out)
}

fn sample_ts(sample: &Value) -> u64 {
    sample
        .pointer("/report/exported_at_ms")
        .and_then(Value::as_u64)
        .or_else(|| {
            sample
                .pointer("/raw_host_metrics/exported_at_ms")
                .and_then(Value::as_u64)
        })
        .or_else(|| {
            sample
                .pointer("/strict_evidence/exported_at_ms")
                .and_then(Value::as_u64)
        })
        .unwrap_or(0)
}

fn read_message<R: BufRead>(reader: &mut R) -> io::Result<Option<Value>> {
    let mut content_length = None;
    let mut line = String::new();

    loop {
        line.clear();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            let parsed = value.trim().parse::<usize>().map_err(|err| {
                io::Error::new(io::ErrorKind::InvalidData, format!("Invalid Content-Length: {err}"))
            })?;
            content_length = Some(parsed);
        }
    }

    let Some(content_length) = content_length else {
        return Ok(None);
    };

    let mut payload = vec![0_u8; content_length];
    reader.read_exact(&mut payload)?;
    let value = serde_json::from_slice(&payload)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    Ok(Some(value))
}

fn write_message<W: Write>(writer: &mut W, payload: &Value) -> io::Result<()> {
    let bytes =
        serde_json::to_vec(payload).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    write!(writer, "Content-Length: {}\r\n\r\n", bytes.len())?;
    writer.write_all(&bytes)?;
    Ok(())
}

fn ok_response(id: Option<Value>, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result
    })
}

fn error_response(id: Value, code: i64, message: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

fn system_time_ms(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

fn now_ms() -> u64 {
    system_time_ms(SystemTime::now()).unwrap_or_else(|| {
        Duration::from_secs(0)
            .as_millis()
            .try_into()
            .unwrap_or_default()
    })
}
