use soulkernel_core::audit::{default_audit_path, now_ms_local};
use soulkernel_core::formula::{self, FormulaResult, WorkloadProfile};
use soulkernel_core::metrics::{self, ResourceState};
use soulkernel_core::orchestrator;
use soulkernel_core::platform::{self, PlatformInfo, PolicyMode, SoulRamBackendInfo};
use soulkernel_core::processes::{self, ProcessObservedReport};
use soulkernel_core::telemetry::{
    MachineActivity, TelemetryIngestRequest, TelemetryState, TelemetrySummary,
};
use soulkernel_core::workload_catalog::{self, WorkloadSceneDto};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

pub struct LiteViewModel {
    pub now_ms: u64,
    pub metrics: ResourceState,
    pub formula: FormulaResult,
    pub telemetry: TelemetrySummary,
    pub process_report: ProcessObservedReport,
    pub platform_info: PlatformInfo,
    pub soulram_backend: SoulRamBackendInfo,
    pub policy_mode: PolicyMode,
    pub selected_workload: String,
    pub workloads: Vec<WorkloadSceneDto>,
    pub dome_active: bool,
    pub soulram_active: bool,
    pub kappa: f64,
    pub sigma_max: f64,
    pub eta: f64,
    pub soulram_percent: u8,
    pub target_pid: Option<u32>,
    pub audit_path: String,
    pub last_actions: Vec<String>,
}

pub struct LiteState {
    runtime: Runtime,
    telemetry_state: TelemetryState,
    pub vm: LiteViewModel,
    dome_snapshot: Option<ResourceState>,
    last_refresh: Instant,
}

impl LiteState {
    pub fn new() -> Result<Self, String> {
        let runtime = Runtime::new().map_err(|e| e.to_string())?;
        let workloads = workload_catalog::list_scenes_for_ui();
        let selected_workload = workloads
            .first()
            .map(|w| w.id.clone())
            .unwrap_or_else(|| "balanced".to_string());
        let baseline = metrics::collect().map_err(|e| e.to_string())?;
        let profile = WorkloadProfile::from_name(&selected_workload).unwrap_or(WorkloadProfile {
            name: selected_workload.clone(),
            alpha: [0.2, 0.2, 0.2, 0.2, 0.2],
            duration_estimate_s: 60.0,
        });
        let formula = formula::compute(&baseline, &profile, 2.0);
        let now_ms = now_ms_local();
        let mut telemetry_state = TelemetryState::new_default();
        let _ = telemetry_state.ingest(TelemetryIngestRequest {
            ts_ms: Some(now_ms),
            power_watts: baseline.raw.power_watts,
            dome_active: false,
            soulram_active: false,
            kpi_gain_median_pct: None,
            cpu_pct: Some(baseline.raw.cpu_pct),
            pi: Some(formula.pi),
            machine_activity: Some(MachineActivity::Active),
            mem_used_mb: Some(baseline.raw.mem_used_mb as f64),
            mem_total_mb: Some(baseline.raw.mem_total_mb as f64),
            power_source_tag: baseline.raw.power_watts_source.clone(),
        });
        let telemetry = telemetry_state.summary(now_ms);
        let process_report = processes::collect_observed_report(12);
        let platform_info = platform::info();
        let soulram_backend = platform::soulram_backend_info();

        Ok(Self {
            runtime,
            telemetry_state,
            vm: LiteViewModel {
                now_ms,
                metrics: baseline,
                formula,
                telemetry,
                process_report,
                platform_info,
                soulram_backend,
                policy_mode: PolicyMode::Privileged,
                selected_workload,
                workloads,
                dome_active: false,
                soulram_active: false,
                kappa: 2.0,
                sigma_max: 0.75,
                eta: 0.15,
                soulram_percent: 20,
                target_pid: None,
                audit_path: default_audit_path().to_string_lossy().into_owned(),
                last_actions: Vec::new(),
            },
            dome_snapshot: None,
            last_refresh: Instant::now() - Duration::from_secs(10),
        })
    }

    pub fn refresh_if_needed(&mut self) -> Result<bool, String> {
        if self.last_refresh.elapsed() < Duration::from_secs(2) {
            return Ok(false);
        }
        self.last_refresh = Instant::now();
        self.refresh_now()?;
        Ok(true)
    }

    pub fn refresh_now(&mut self) -> Result<(), String> {
        let metrics = metrics::collect().map_err(|e| e.to_string())?;
        let profile = self.selected_profile();
        let formula = formula::compute(&metrics, &profile, self.vm.kappa);
        let now_ms = now_ms_local();
        self.vm.now_ms = now_ms;
        self.vm.metrics = metrics.clone();
        self.vm.formula = formula.clone();
        self.vm.process_report = processes::collect_observed_report(12);
        self.vm.platform_info = platform::info();
        self.vm.soulram_backend = platform::soulram_backend_info();
        let _ = self.telemetry_state.ingest(TelemetryIngestRequest {
            ts_ms: Some(now_ms),
            power_watts: metrics.raw.power_watts,
            dome_active: self.vm.dome_active,
            soulram_active: self.vm.soulram_active,
            kpi_gain_median_pct: None,
            cpu_pct: Some(metrics.raw.cpu_pct),
            pi: Some(formula.pi),
            machine_activity: Some(MachineActivity::Active),
            mem_used_mb: Some(metrics.raw.mem_used_mb as f64),
            mem_total_mb: Some(metrics.raw.mem_total_mb as f64),
            power_source_tag: metrics.raw.power_watts_source.clone(),
        });
        self.vm.telemetry = self.telemetry_state.summary(now_ms);
        self.vm.target_pid = self
            .vm
            .process_report
            .top_processes
            .iter()
            .find(|p| !p.is_self_process && !p.is_embedded_webview)
            .map(|p| p.pid);
        Ok(())
    }

    pub fn selected_profile(&self) -> WorkloadProfile {
        WorkloadProfile::from_name(&self.vm.selected_workload).unwrap_or(WorkloadProfile {
            name: self.vm.selected_workload.clone(),
            alpha: [0.2, 0.2, 0.2, 0.2, 0.2],
            duration_estimate_s: 60.0,
        })
    }

    pub fn activate_dome(&mut self) -> Result<(), String> {
        let baseline = self.vm.metrics.clone();
        let profile = self.selected_profile();
        let result = self
            .runtime
            .block_on(orchestrator::activate(
                &profile,
                self.vm.eta,
                &baseline,
                self.vm.policy_mode,
                self.vm.target_pid,
            ))
            .map_err(|e| e.to_string())?;
        self.dome_snapshot = Some(baseline);
        self.vm.dome_active = true;
        self.vm.last_actions = result.actions;
        self.refresh_now()
    }

    pub fn rollback_dome(&mut self) -> Result<(), String> {
        let actions = self
            .runtime
            .block_on(orchestrator::rollback(
                self.dome_snapshot.clone(),
                self.vm.target_pid,
            ))
            .map_err(|e| e.to_string())?;
        self.vm.dome_active = false;
        self.vm.last_actions = actions;
        self.refresh_now()
    }

    pub fn enable_soulram(&mut self) -> Result<(), String> {
        let actions = self
            .runtime
            .block_on(platform::enable_soulram(self.vm.soulram_percent));
        self.vm.soulram_active = platform::soulram_enablement_effective(&actions);
        self.vm.last_actions = actions
            .into_iter()
            .map(|(msg, ok)| {
                if ok {
                    format!("✓ {msg}")
                } else {
                    format!("✗ {msg}")
                }
            })
            .collect();
        self.refresh_now()
    }

    pub fn disable_soulram(&mut self) -> Result<(), String> {
        let actions = self.runtime.block_on(platform::disable_soulram());
        self.vm.soulram_active = false;
        self.vm.last_actions = actions
            .into_iter()
            .map(|(msg, ok)| {
                if ok {
                    format!("✓ {msg}")
                } else {
                    format!("✗ {msg}")
                }
            })
            .collect();
        self.refresh_now()
    }
}
