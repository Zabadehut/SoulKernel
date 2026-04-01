use crate::state::LiteViewModel;
use rfd::FileDialog;
use serde::Serialize;

#[derive(Serialize)]
struct LiteExport<'a> {
    exported_at_ms: u64,
    workload: &'a str,
    dome_active: bool,
    soulram_active: bool,
    policy_mode: &'a str,
    kappa: f64,
    sigma_max: f64,
    eta: f64,
    metrics: &'a soulkernel_core::metrics::ResourceState,
    formula: &'a soulkernel_core::formula::FormulaResult,
    telemetry: &'a soulkernel_core::telemetry::TelemetrySummary,
    processes: &'a soulkernel_core::processes::ProcessObservedReport,
    audit_path: &'a str,
    last_actions: &'a [String],
}

pub fn export_snapshot(vm: &LiteViewModel) -> Result<String, String> {
    let path = FileDialog::new()
        .set_file_name("soulkernel-lite-export.json")
        .save_file()
        .ok_or_else(|| "export annulé".to_string())?;

    let payload = LiteExport {
        exported_at_ms: vm.now_ms,
        workload: &vm.selected_workload,
        dome_active: vm.dome_active,
        soulram_active: vm.soulram_active,
        policy_mode: vm.policy_mode.as_name(),
        kappa: vm.kappa,
        sigma_max: vm.sigma_max,
        eta: vm.eta,
        metrics: &vm.metrics,
        formula: &vm.formula,
        telemetry: &vm.telemetry,
        processes: &vm.process_report,
        audit_path: &vm.audit_path,
        last_actions: &vm.last_actions,
    };

    let bytes = serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}
