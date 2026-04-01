use soulkernel_core::audit::{audit_write, default_audit_path, AuditState, SharedAudit};
use soulkernel_core::metrics;
use soulkernel_core::telemetry::{MachineActivity, TelemetryIngestRequest, TelemetryState};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--once") {
        match metrics::collect() {
            Ok(sample) => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&sample).unwrap_or_default()
                );
                return;
            }
            Err(err) => {
                eprintln!("headless metrics failed: {err}");
                std::process::exit(1);
            }
        }
    }

    let audit: SharedAudit = Arc::new(Mutex::new(AuditState {
        path: Some(default_audit_path()),
    }));
    let mut telemetry = TelemetryState::new_default();
    let _ = audit_write(
        &audit,
        "headless",
        "started",
        Some("info"),
        Some(serde_json::json!({ "mode": "continuous" })),
    );

    loop {
        if let Ok(sample) = metrics::collect() {
            let _ = telemetry.ingest(TelemetryIngestRequest {
                ts_ms: None,
                power_watts: sample.raw.power_watts,
                dome_active: false,
                soulram_active: false,
                kpi_gain_median_pct: None,
                cpu_pct: Some(sample.raw.cpu_pct),
                pi: None,
                machine_activity: Some(MachineActivity::Active),
                mem_used_mb: Some(sample.raw.mem_used_mb as f64),
                mem_total_mb: Some(sample.raw.mem_total_mb as f64),
                power_source_tag: sample.raw.power_watts_source.clone(),
            });
            let _ = audit_write(
                &audit,
                "telemetry",
                "sample",
                None,
                Some(serde_json::json!({
                    "cpu_pct": sample.raw.cpu_pct,
                    "mem_pct": sample.mem,
                    "gpu_pct": sample.raw.gpu_pct,
                    "power_watts": sample.raw.power_watts,
                })),
            );
        }

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                let _ = audit_write(&audit, "headless", "stopped", Some("info"), None);
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
        }
    }
}
