use crate::state::LiteViewModel;
use rfd::FileDialog;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExternalConfigExport<'a> {
    enabled: bool,
    power_file: Option<&'a str>,
    max_age_ms: Option<u64>,
    meross_email: Option<&'a str>,
    meross_region: Option<&'a str>,
    meross_device_type: Option<&'a str>,
    meross_http_proxy: Option<&'a str>,
    mfa_present: bool,
    python_bin: Option<&'a str>,
    bridge_interval_s: Option<f64>,
    autostart_bridge: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LiteExport<'a> {
    exported_at_ms: u64,
    workload: &'a str,
    dome_active: bool,
    soulram_active: bool,
    policy_mode: &'a str,
    kappa: f64,
    sigma_max: f64,
    eta: f64,
    target_pid: Option<u32>,
    auto_target: bool,
    manual_target_pid: Option<u32>,
    platform_info: &'a soulkernel_core::platform::PlatformInfo,
    soulram_backend: &'a soulkernel_core::platform::SoulRamBackendInfo,
    metrics: &'a soulkernel_core::metrics::ResourceState,
    formula: &'a soulkernel_core::formula::FormulaResult,
    telemetry: &'a soulkernel_core::telemetry::TelemetrySummary,
    processes: &'a soulkernel_core::processes::ProcessObservedReport,
    device_inventory: &'a soulkernel_core::inventory::DeviceInventoryReport,
    external_config: ExternalConfigExport<'a>,
    external_status: &'a soulkernel_core::external_power::ExternalPowerStatus,
    external_bridge_running: bool,
    external_bridge_detail: &'a str,
    benchmark_last_session: &'a Option<soulkernel_core::benchmark::BenchmarkSession>,
    benchmark_history: &'a Option<soulkernel_core::benchmark::BenchmarkHistoryResponse>,
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
        target_pid: vm.target_pid,
        auto_target: vm.auto_target,
        manual_target_pid: vm.manual_target_pid,
        platform_info: &vm.platform_info,
        soulram_backend: &vm.soulram_backend,
        metrics: &vm.metrics,
        formula: &vm.formula,
        telemetry: &vm.telemetry,
        processes: &vm.process_report,
        device_inventory: &vm.device_inventory,
        external_config: ExternalConfigExport {
            enabled: vm.external_config.enabled,
            power_file: vm.external_config.power_file.as_deref(),
            max_age_ms: vm.external_config.max_age_ms,
            meross_email: vm.external_config.meross_email.as_deref(),
            meross_region: vm.external_config.meross_region.as_deref(),
            meross_device_type: vm.external_config.meross_device_type.as_deref(),
            meross_http_proxy: vm.external_config.meross_http_proxy.as_deref(),
            mfa_present: vm
                .external_config
                .meross_mfa_code
                .as_deref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false),
            python_bin: vm.external_config.python_bin.as_deref(),
            bridge_interval_s: vm.external_config.bridge_interval_s,
            autostart_bridge: vm.external_config.autostart_bridge,
        },
        external_status: &vm.external_status,
        external_bridge_running: vm.external_bridge_running,
        external_bridge_detail: &vm.external_bridge_detail,
        benchmark_last_session: &vm.benchmark_last_session,
        benchmark_history: &vm.benchmark_history,
        audit_path: &vm.audit_path,
        last_actions: &vm.last_actions,
    };

    let bytes = serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}
