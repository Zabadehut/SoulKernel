use soulkernel_core::audit::{default_audit_path, now_ms_local};
use soulkernel_core::benchmark::{
    self, compute_summary, BenchmarkHistoryResponse, BenchmarkPhase, BenchmarkSample,
    BenchmarkSession, BenchmarkState,
};
use soulkernel_core::external_power::{self, ExternalPowerStatus, MerossFileConfig};
use soulkernel_core::formula::{self, FormulaResult, WorkloadProfile};
use soulkernel_core::inventory::{self, DeviceInventoryReport};
use soulkernel_core::metrics::{self, ResourceState};
use soulkernel_core::orchestrator;
use soulkernel_core::platform::{self, PlatformInfo, PolicyMode, SoulRamBackendInfo};
use soulkernel_core::processes::{self, ProcessObservedReport};
use soulkernel_core::telemetry::{
    MachineActivity, TelemetryIngestRequest, TelemetryState, TelemetrySummary,
};
use soulkernel_core::workload_catalog::{self, WorkloadSceneDto};
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

fn command_silent(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

pub struct LiteViewModel {
    pub now_ms: u64,
    pub metrics: ResourceState,
    pub formula: FormulaResult,
    pub telemetry: TelemetrySummary,
    pub process_report: ProcessObservedReport,
    pub platform_info: PlatformInfo,
    pub soulram_backend: SoulRamBackendInfo,
    pub device_inventory: DeviceInventoryReport,
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
    pub auto_target: bool,
    pub manual_target_pid: Option<u32>,
    pub audit_path: String,
    pub last_actions: Vec<String>,
    pub external_config: MerossFileConfig,
    pub external_status: ExternalPowerStatus,
    pub external_bridge_running: bool,
    pub external_bridge_detail: String,
    pub benchmark_command: String,
    pub benchmark_args: String,
    pub benchmark_cwd: String,
    pub benchmark_runs_per_state: usize,
    pub benchmark_duration_ms: u64,
    pub benchmark_settle_ms: u64,
    pub benchmark_use_system_probe: bool,
    pub benchmark_last_session: Option<BenchmarkSession>,
    pub benchmark_history: Option<BenchmarkHistoryResponse>,
    pub show_hud: bool,
}

pub struct LiteState {
    runtime: Runtime,
    telemetry_state: TelemetryState,
    benchmark_state: BenchmarkState,
    pub vm: LiteViewModel,
    dome_snapshot: Option<ResourceState>,
    last_refresh: Instant,
    last_process_refresh: Instant,
    last_inventory_refresh: Instant,
    external_bridge_child: Option<Child>,
    refresh_rx: Option<Receiver<Result<LiteRefreshSnapshot, String>>>,
    refresh_in_flight: bool,
}

struct LiteRefreshSnapshot {
    now_ms: u64,
    metrics: ResourceState,
    formula: FormulaResult,
    process_report: Option<ProcessObservedReport>,
    platform_info: PlatformInfo,
    soulram_backend: SoulRamBackendInfo,
    device_inventory: Option<DeviceInventoryReport>,
    external_status: ExternalPowerStatus,
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
        let device_inventory = inventory::collect_device_inventory();
        let mut external_config = external_power::get_meross_config_or_default();
        // Pré-remplir les champs optionnels avec leurs valeurs par défaut calculées
        // pour que les champs UI ne soient pas vides au premier lancement.
        if external_config.power_file.is_none() {
            external_config.power_file =
                external_power::default_power_file().map(|p| p.to_string_lossy().into_owned());
        }
        if external_config.python_bin.is_none() {
            external_config.python_bin = Some(if cfg!(target_os = "windows") {
                "py".to_string()
            } else {
                "python3".to_string()
            });
        }
        if external_config.meross_region.is_none() {
            external_config.meross_region = Some("eu".to_string());
        }
        if external_config.meross_device_type.is_none() {
            external_config.meross_device_type = Some("mss315".to_string());
        }
        let external_status = external_power::get_external_power_status();
        let benchmark_path = default_benchmark_path();
        let benchmark_state = BenchmarkState::new(benchmark_path);
        let benchmark_history = benchmark_state.history(None, None, None, None);

        Ok(Self {
            runtime,
            telemetry_state,
            benchmark_state,
            vm: LiteViewModel {
                now_ms,
                metrics: baseline,
                formula,
                telemetry,
                process_report,
                platform_info,
                soulram_backend,
                device_inventory,
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
                auto_target: true,
                manual_target_pid: None,
                audit_path: default_audit_path().to_string_lossy().into_owned(),
                last_actions: Vec::new(),
                external_config,
                external_status,
                external_bridge_running: false,
                external_bridge_detail: "bridge inactif".to_string(),
                benchmark_command: String::new(),
                benchmark_args: String::new(),
                benchmark_cwd: String::new(),
                benchmark_runs_per_state: 4,
                benchmark_duration_ms: 3000,
                benchmark_settle_ms: 1200,
                benchmark_use_system_probe: true,
                benchmark_last_session: None,
                benchmark_history: Some(benchmark_history),
                show_hud: false,
            },
            dome_snapshot: None,
            last_refresh: Instant::now() - Duration::from_secs(10),
            last_process_refresh: Instant::now() - Duration::from_secs(10),
            last_inventory_refresh: Instant::now() - Duration::from_secs(20),
            external_bridge_child: None,
            refresh_rx: None,
            refresh_in_flight: false,
        })
    }

    pub fn refresh_if_needed(&mut self) -> Result<bool, String> {
        let applied = self.apply_pending_refresh()?;
        if self.last_refresh.elapsed() < Duration::from_secs(2) {
            return Ok(applied);
        }
        if self.refresh_in_flight {
            return Ok(applied);
        }
        self.last_refresh = Instant::now();
        self.spawn_refresh_task();
        Ok(applied)
    }

    pub fn refresh_now(&mut self) -> Result<(), String> {
        let snapshot =
            Self::collect_refresh_snapshot(self.selected_profile(), self.vm.kappa, true, true)?;
        self.apply_refresh_snapshot(snapshot)?;
        self.refresh_in_flight = false;
        self.refresh_rx = None;
        self.last_refresh = Instant::now();
        Ok(())
    }

    fn spawn_refresh_task(&mut self) {
        let profile = self.selected_profile();
        let kappa = self.vm.kappa;
        let refresh_processes = self.last_process_refresh.elapsed() >= Duration::from_secs(4);
        let refresh_inventory = self.last_inventory_refresh.elapsed() >= Duration::from_secs(15);
        let (tx, rx) = mpsc::channel();
        self.refresh_rx = Some(rx);
        self.refresh_in_flight = true;
        std::thread::spawn(move || {
            let _ = tx.send(Self::collect_refresh_snapshot(
                profile,
                kappa,
                refresh_processes,
                refresh_inventory,
            ));
        });
    }

    fn apply_pending_refresh(&mut self) -> Result<bool, String> {
        let Some(rx) = self.refresh_rx.take() else {
            return Ok(false);
        };
        match rx.try_recv() {
            Ok(result) => {
                self.refresh_in_flight = false;
                let snapshot = result?;
                self.apply_refresh_snapshot(snapshot)?;
                Ok(true)
            }
            Err(TryRecvError::Empty) => {
                self.refresh_rx = Some(rx);
                Ok(false)
            }
            Err(TryRecvError::Disconnected) => {
                self.refresh_in_flight = false;
                Err("rafraîchissement lite interrompu".to_string())
            }
        }
    }

    fn collect_refresh_snapshot(
        profile: WorkloadProfile,
        kappa: f64,
        refresh_processes: bool,
        refresh_inventory: bool,
    ) -> Result<LiteRefreshSnapshot, String> {
        let metrics = metrics::collect().map_err(|e| e.to_string())?;
        let formula = formula::compute(&metrics, &profile, kappa);
        Ok(LiteRefreshSnapshot {
            now_ms: now_ms_local(),
            metrics,
            formula,
            process_report: refresh_processes.then(|| processes::collect_observed_report(12)),
            platform_info: platform::info(),
            soulram_backend: platform::soulram_backend_info(),
            device_inventory: refresh_inventory.then(inventory::collect_device_inventory),
            external_status: external_power::get_external_power_status(),
        })
    }

    fn apply_refresh_snapshot(&mut self, snapshot: LiteRefreshSnapshot) -> Result<(), String> {
        self.vm.now_ms = snapshot.now_ms;
        self.vm.metrics = snapshot.metrics.clone();
        self.vm.formula = snapshot.formula.clone();
        if let Some(process_report) = snapshot.process_report {
            self.vm.process_report = process_report;
            self.last_process_refresh = Instant::now();
        }
        self.vm.platform_info = snapshot.platform_info;
        self.vm.soulram_backend = snapshot.soulram_backend;
        if let Some(device_inventory) = snapshot.device_inventory {
            self.vm.device_inventory = device_inventory;
            self.last_inventory_refresh = Instant::now();
        }
        self.vm.external_status = snapshot.external_status;
        self.vm.external_bridge_running = self.is_external_bridge_running();
        let _ = self.telemetry_state.ingest(TelemetryIngestRequest {
            ts_ms: Some(self.vm.now_ms),
            power_watts: snapshot.metrics.raw.power_watts,
            dome_active: self.vm.dome_active,
            soulram_active: self.vm.soulram_active,
            kpi_gain_median_pct: None,
            cpu_pct: Some(snapshot.metrics.raw.cpu_pct),
            pi: Some(snapshot.formula.pi),
            machine_activity: Some(MachineActivity::Active),
            mem_used_mb: Some(snapshot.metrics.raw.mem_used_mb as f64),
            mem_total_mb: Some(snapshot.metrics.raw.mem_total_mb as f64),
            power_source_tag: snapshot.metrics.raw.power_watts_source.clone(),
        });
        self.vm.telemetry = self.telemetry_state.summary(self.vm.now_ms);
        self.vm.target_pid = if self.vm.auto_target {
            self.vm
                .process_report
                .top_processes
                .iter()
                .find(|p| !p.is_self_process && !p.is_embedded_webview)
                .map(|p| p.pid)
        } else {
            self.vm.manual_target_pid
        };
        Ok(())
    }

    pub fn is_refresh_in_flight(&self) -> bool {
        self.refresh_in_flight
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

    pub fn save_external_config(&mut self) -> Result<(), String> {
        external_power::save_meross_config(&self.vm.external_config)?;
        self.vm.external_status = external_power::get_external_power_status();
        Ok(())
    }

    pub fn start_external_bridge(&mut self) -> Result<(), String> {
        if !self.vm.external_config.enabled {
            return Err("active d'abord la source puissance externe".to_string());
        }
        let email = self
            .vm
            .external_config
            .meross_email
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "MEROSS email manquant".to_string())?
            .to_string();
        let password = self
            .vm
            .external_config
            .meross_password
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "MEROSS password manquant".to_string())?
            .to_string();
        let region = self
            .vm
            .external_config
            .meross_region
            .as_deref()
            .unwrap_or("eu")
            .trim()
            .to_string();
        let device_type = self
            .vm
            .external_config
            .meross_device_type
            .as_deref()
            .unwrap_or("mss315")
            .trim()
            .to_string();
        let interval = self
            .vm
            .external_config
            .bridge_interval_s
            .unwrap_or(8.0)
            .clamp(2.0, 300.0);
        let python_bin = pick_python_bin(&self.vm.external_config)?;
        let script_path = resolve_bridge_script_path()?;
        let out_path = self
            .vm
            .external_config
            .power_file
            .as_ref()
            .map(PathBuf::from)
            .or_else(external_power::default_power_file)
            .ok_or_else(|| "power file path unavailable".to_string())?;
        let log_path = external_power::soulkernel_config_dir()
            .map(|d| d.join("meross_bridge.log"))
            .ok_or_else(|| "bridge log path unavailable".to_string())?;
        let creds_cache_path = external_power::default_creds_cache_file()
            .ok_or_else(|| "creds cache path unavailable".to_string())?;
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if let Some(parent) = creds_cache_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if self.is_external_bridge_running() {
            return Ok(());
        }
        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| e.to_string())?;
        let stderr = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| e.to_string())?;
        let mut cmd = command_silent(&python_bin);
        cmd.arg(script_path)
            .arg("--out")
            .arg(&out_path)
            .arg("--interval")
            .arg(format!("{interval:.1}"))
            .env("MEROSS_EMAIL", email)
            .env("MEROSS_PASSWORD", password)
            .env("MEROSS_REGION", &region)
            .env("MEROSS_DEVICE_TYPE", &device_type)
            .env("MEROSS_CREDS_CACHE", &creds_cache_path)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .stdin(Stdio::null());
        if let Some(proxy) = self
            .vm
            .external_config
            .meross_http_proxy
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            cmd.env("MEROSS_HTTP_PROXY", proxy);
        }
        if let Some(mfa) = self
            .vm
            .external_config
            .meross_mfa_code
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            cmd.env("MEROSS_MFA_CODE", mfa);
        }
        let child = cmd.spawn().map_err(|e| e.to_string())?;
        self.external_bridge_child = Some(child);
        self.set_external_bridge_detail(
            "bridge démarré, attente des premiers échantillons".to_string(),
        );
        self.vm.external_bridge_running = self.is_external_bridge_running();
        self.vm
            .last_actions
            .push("✓ bridge externe démarré".to_string());
        Ok(())
    }

    pub fn stop_external_bridge(&mut self) -> Result<(), String> {
        if let Some(child) = self.external_bridge_child.as_mut() {
            child.kill().map_err(|e| e.to_string())?;
            let _ = child.wait();
        }
        self.external_bridge_child = None;
        self.vm.external_bridge_running = false;
        self.set_external_bridge_detail("bridge arrêté par l'utilisateur".to_string());
        self.vm
            .last_actions
            .push("✓ bridge externe arrêté".to_string());
        Ok(())
    }

    pub fn is_external_bridge_running(&mut self) -> bool {
        if let Some(child) = self.external_bridge_child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let detail = self.describe_bridge_exit(status);
                    self.set_external_bridge_detail(detail.clone());
                    self.vm.last_actions.push(format!("✗ {detail}"));
                    self.external_bridge_child = None;
                    false
                }
                Ok(None) => {
                    if self.vm.external_status.is_fresh {
                        self.set_external_bridge_detail(format!(
                            "bridge actif · dernière mesure {}",
                            crate::fmt::watts(self.vm.external_status.last_watts)
                        ));
                    } else {
                        self.set_external_bridge_detail(
                            "bridge actif · en attente d'une mesure fraîche".to_string(),
                        );
                    }
                    true
                }
                Err(e) => {
                    self.set_external_bridge_detail(format!("bridge status error: {e}"));
                    self.external_bridge_child = None;
                    false
                }
            }
        } else {
            if self.vm.external_bridge_detail.is_empty() {
                self.set_external_bridge_detail("bridge inactif".to_string());
            }
            false
        }
    }

    fn set_external_bridge_detail(&mut self, detail: String) {
        self.vm.external_bridge_detail = detail;
    }

    fn describe_bridge_exit(&self, status: ExitStatus) -> String {
        let status_text = match status.code() {
            Some(code) => format!("bridge arrêté (code {code})"),
            None => "bridge arrêté (terminé par signal)".to_string(),
        };
        match bridge_log_last_non_empty_line() {
            Some(line) => format!("{status_text} · {line}"),
            None => status_text,
        }
    }

    pub fn run_benchmark(&mut self) -> Result<(), String> {
        if self.vm.dome_active {
            return Err("rollback le dôme avant un benchmark A/B".to_string());
        }
        let started_at = now_ms_local().to_string();
        let profile = self.selected_profile();
        let mut samples = Vec::new();
        let runs = self.vm.benchmark_runs_per_state.max(1);
        for idx in 0..runs {
            samples.push(self.run_benchmark_phase(idx + 1, BenchmarkPhase::Off, &profile)?);
        }

        let _ = self
            .runtime
            .block_on(tokio::time::sleep(Duration::from_millis(
                self.vm.benchmark_settle_ms,
            )));
        let _ = self.activate_dome();
        if self.vm.soulram_percent > 0 && !self.vm.soulram_active {
            let _ = self.enable_soulram();
        }
        let _ = self
            .runtime
            .block_on(tokio::time::sleep(Duration::from_millis(
                self.vm.benchmark_settle_ms,
            )));
        for idx in 0..runs {
            samples.push(self.run_benchmark_phase(idx + 1, BenchmarkPhase::On, &profile)?);
        }
        let _ = self.rollback_dome();

        let finished_at = now_ms_local().to_string();
        let summary = compute_summary(&samples);
        let session = BenchmarkSession {
            started_at,
            finished_at,
            command: self.vm.benchmark_command.clone(),
            args: split_args(&self.vm.benchmark_args),
            cwd: normalized_text(&self.vm.benchmark_cwd),
            runs_per_state: runs,
            settle_ms: self.vm.benchmark_settle_ms,
            workload: profile.name.clone(),
            kappa: self.vm.kappa,
            sigma_max: self.vm.sigma_max,
            eta: self.vm.eta,
            target_pid: self.vm.target_pid,
            policy_mode: Some(self.vm.policy_mode.as_name().to_string()),
            soulram_percent: Some(self.vm.soulram_percent),
            samples,
            summary,
        };
        self.benchmark_state.record_session(session.clone())?;
        self.vm.benchmark_last_session = Some(session);
        self.vm.benchmark_history = Some(self.benchmark_state.history(None, None, None, None));
        self.refresh_now()
    }

    fn run_benchmark_phase(
        &mut self,
        idx: usize,
        phase: BenchmarkPhase,
        profile: &WorkloadProfile,
    ) -> Result<BenchmarkSample, String> {
        let before = metrics::collect().map_err(|e| e.to_string())?;
        let f_before = formula::compute(&before, profile, self.vm.kappa);
        let probe =
            if self.vm.benchmark_use_system_probe || self.vm.benchmark_command.trim().is_empty() {
                self.runtime.block_on(benchmark::execute_system_probe(
                    self.vm.benchmark_duration_ms,
                ))?
            } else {
                self.runtime.block_on(benchmark::execute_probe(
                    self.vm.benchmark_command.clone(),
                    split_args(&self.vm.benchmark_args),
                    normalized_text(&self.vm.benchmark_cwd),
                ))?
            };
        let after = metrics::collect().map_err(|e| e.to_string())?;
        let f_after = formula::compute(&after, profile, self.vm.kappa);
        Ok(BenchmarkSample {
            idx,
            phase,
            ts: now_ms_local().to_string(),
            duration_ms: probe.duration_ms,
            success: probe.success,
            exit_code: probe.exit_code,
            dome_active: phase == BenchmarkPhase::On,
            workload: profile.name.clone(),
            kappa: self.vm.kappa,
            sigma_max: self.vm.sigma_max,
            eta: self.vm.eta,
            sigma_before: Some(before.sigma),
            sigma_after: Some(after.sigma),
            cpu_before_pct: Some(before.raw.cpu_pct),
            cpu_after_pct: Some(after.raw.cpu_pct),
            mem_before_gb: Some(before.raw.mem_used_mb as f64 / 1024.0),
            mem_after_gb: Some(after.raw.mem_used_mb as f64 / 1024.0),
            gpu_before_pct: before.raw.gpu_pct,
            gpu_after_pct: after.raw.gpu_pct,
            io_before_mb_s: Some(
                before.raw.io_read_mb_s.unwrap_or(0.0) + before.raw.io_write_mb_s.unwrap_or(0.0),
            ),
            io_after_mb_s: Some(
                after.raw.io_read_mb_s.unwrap_or(0.0) + after.raw.io_write_mb_s.unwrap_or(0.0),
            ),
            power_before_watts: before.raw.power_watts,
            power_after_watts: after.raw.power_watts,
            cpu_temp_before_c: before.raw.cpu_temp_c,
            cpu_temp_after_c: after.raw.cpu_temp_c,
            gpu_temp_before_c: before.raw.gpu_temp_c,
            gpu_temp_after_c: after.raw.gpu_temp_c,
            sigma_effective_before: Some(f_before.sigma_effective),
            sigma_effective_after: Some(f_after.sigma_effective),
            stdout_tail: probe.stdout_tail,
            stderr_tail: probe.stderr_tail,
        })
    }
}

fn normalized_text(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn split_args(input: &str) -> Vec<String> {
    input.split_whitespace().map(|s| s.to_string()).collect()
}

fn default_benchmark_path() -> PathBuf {
    default_audit_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("soulkernel_benchmark_history.jsonl")
}

fn bundled_python_relative_paths() -> &'static [&'static str] {
    #[cfg(target_os = "windows")]
    {
        &[
            "runtime/python/windows/python.exe",
            "python/windows/python.exe",
        ]
    }
    #[cfg(target_os = "macos")]
    {
        &[
            "runtime/python/macos/bin/python3",
            "python/macos/bin/python3",
        ]
    }
    #[cfg(target_os = "linux")]
    {
        &[
            "runtime/python/linux/bin/python3",
            "python/linux/bin/python3",
        ]
    }
}

fn resolve_bundled_python() -> Option<PathBuf> {
    let mut bases = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            bases.push(dir.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.push(cwd);
    }

    for base in bases {
        for rel in bundled_python_relative_paths() {
            let candidate = base.join(rel);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn effective_python_candidates(cfg: &MerossFileConfig) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(path) = resolve_bundled_python() {
        out.push(path.to_string_lossy().into_owned());
    }
    if let Some(bin) = cfg
        .python_bin
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        out.push(bin.to_string());
    }
    #[cfg(target_os = "windows")]
    {
        out.push("py".to_string());
        out.push("python".to_string());
        out.push("python3".to_string());
    }
    #[cfg(not(target_os = "windows"))]
    {
        out.push("python3".to_string());
        out.push("python".to_string());
    }
    out.dedup();
    out
}

fn pick_python_bin(cfg: &MerossFileConfig) -> Result<String, String> {
    for candidate in effective_python_candidates(cfg) {
        if command_silent(&candidate)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Ok(candidate);
        }
    }
    Err(
        "python introuvable (essayés: runtime embarqué, python configuré, python3/python/py)"
            .to_string(),
    )
}

fn bridge_log_last_non_empty_line() -> Option<String> {
    let path = external_power::soulkernel_config_dir()?.join("meross_bridge.log");
    let raw = std::fs::read_to_string(path).ok()?;
    raw.lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn resolve_bridge_script_path() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let repo_script = cwd.join("scripts").join("meross_mss315_bridge.py");
    if repo_script.exists() {
        return Ok(repo_script);
    }
    let exe_dir = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| "executable dir unavailable".to_string())?;
    let bundled = exe_dir.join("scripts").join("meross_mss315_bridge.py");
    if bundled.exists() {
        return Ok(bundled);
    }
    Err("meross bridge script not found".to_string())
}
