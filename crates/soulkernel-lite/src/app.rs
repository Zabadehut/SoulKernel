use crate::export;
use crate::fmt;
use crate::state::{LiteState, LiteViewModel};
use eframe::egui;

pub struct LiteApp {
    state: Option<LiteState>,
    error: Option<String>,
    info: Option<String>,
}

impl Default for LiteApp {
    fn default() -> Self {
        match LiteState::new() {
            Ok(state) => Self {
                state: Some(state),
                error: None,
                info: None,
            },
            Err(err) => Self {
                state: None,
                error: Some(err),
                info: None,
            },
        }
    }
}

impl LiteApp {
    fn top_bar(ui: &mut egui::Ui, vm: &LiteViewModel) {
        ui.horizontal_wrapped(|ui| {
            ui.heading("SoulKernel Lite");
            ui.label("Rust natif quotidien");
            ui.separator();
            ui.label(format!("OS {}", vm.platform_info.os));
            ui.separator();
            ui.label(if vm.dome_active {
                "DOME ON"
            } else {
                "DOME IDLE"
            });
            ui.separator();
            ui.label(if vm.soulram_active {
                "SOULRAM ON"
            } else {
                "SOULRAM OFF"
            });
            ui.separator();
            ui.label(format!("Σ {:.3}", vm.metrics.sigma));
            ui.separator();
            ui.label(format!("π {:.3}", vm.formula.pi));
        });
    }

    fn metrics_strip(ui: &mut egui::Ui, vm: &LiteViewModel) {
        egui::Grid::new("metrics_strip")
            .num_columns(5)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("CPU");
                ui.label("RAM");
                ui.label("I/O");
                ui.label("GPU");
                ui.label("Power");
                ui.end_row();
                ui.strong(fmt::pct(vm.metrics.raw.cpu_pct));
                ui.strong(fmt::gib_pair(
                    vm.metrics.raw.mem_used_mb,
                    vm.metrics.raw.mem_total_mb,
                ));
                ui.strong(format!(
                    "R {:.2} / W {:.2} MB/s",
                    vm.metrics.raw.io_read_mb_s.unwrap_or(0.0),
                    vm.metrics.raw.io_write_mb_s.unwrap_or(0.0)
                ));
                ui.strong(fmt::opt_pct(vm.metrics.raw.gpu_pct));
                ui.strong(fmt::watts(vm.metrics.raw.power_watts));
                ui.end_row();
            });
    }

    fn left_panel(ui: &mut egui::Ui, vm: &LiteViewModel) {
        ui.group(|ui| {
            ui.heading("Hardware r(t) · Live");
            ui.label(
                "Machine globale. Usage observé d’abord, watts seulement si réellement exposés.",
            );
            ui.separator();
            ui.label(format!(
                "CPU {} · {} MHz",
                fmt::pct(vm.metrics.raw.cpu_pct),
                vm.metrics.raw.cpu_clock_mhz.unwrap_or(0.0).round()
            ));
            ui.label(format!(
                "RAM {} · swap {} / {} MiB",
                fmt::gib_pair(vm.metrics.raw.mem_used_mb, vm.metrics.raw.mem_total_mb),
                vm.metrics.raw.swap_used_mb,
                vm.metrics.raw.swap_total_mb
            ));
            ui.label(format!(
                "Compression {}",
                vm.metrics
                    .compression
                    .map(|v| format!("{v:.3}"))
                    .unwrap_or_else(|| "N/A".to_string())
            ));
            ui.label(format!(
                "I/O R {:.2} / W {:.2} MB/s",
                vm.metrics.raw.io_read_mb_s.unwrap_or(0.0),
                vm.metrics.raw.io_write_mb_s.unwrap_or(0.0)
            ));
            ui.label(format!(
                "GPU {} · {}",
                fmt::opt_pct(vm.metrics.raw.gpu_pct),
                fmt::watts(vm.metrics.raw.gpu_power_watts)
            ));
            ui.label(format!(
                "WebView host {:.1}% CPU · {}",
                vm.metrics.raw.webview_host_cpu_sum.unwrap_or(0.0),
                fmt::mib_from_kb(vm.metrics.raw.webview_host_mem_mb.unwrap_or(0) * 1024)
            ));
        });
    }

    fn center_panel(ui: &mut egui::Ui, vm: &LiteViewModel) {
        ui.group(|ui| {
            ui.heading("Formule unifiée π(t)");
            ui.label(format!(
                "π {:.4} · brut {:.4} · friction {:.4} · frein {:.4}",
                vm.formula.pi, vm.formula.brut, vm.formula.friction, vm.formula.brake
            ));
            ui.label(format!(
                "𝒟 {:.4} · opportunité {:.3} · garde {:.3} · Σ eff. {:.3}",
                vm.formula.dome_gain,
                vm.formula.opportunity,
                vm.formula.advanced_guard,
                vm.formula.sigma_effective
            ));
            ui.separator();
            ui.heading("Green IT · Impact");
            ui.label(format!("Source énergie: {}", vm.telemetry.power_source));
            ui.label(format!(
                "CPU·h diff. {:.2} · RAM·GB·h diff. {:.2}",
                vm.telemetry.total.cpu_hours_differential,
                vm.telemetry.total.mem_gb_hours_differential
            ));
            ui.label(format!(
                "kWh total {:.3} · CO₂ {:.3} kg · coût {:.2} {}",
                vm.telemetry.lifetime.total_energy_kwh,
                vm.telemetry.lifetime.total_co2_measured_kg,
                vm.telemetry.lifetime.total_energy_cost_measured,
                vm.telemetry.pricing.currency
            ));
            ui.label(format!(
                "1h {:.3} kWh · 24h {:.3} · 7j {:.3} · 30j {:.3}",
                vm.telemetry.hour.energy_kwh,
                vm.telemetry.day.energy_kwh,
                vm.telemetry.week.energy_kwh,
                vm.telemetry.month.energy_kwh
            ));
        });
    }

    fn external_power_panel(
        ui: &mut egui::Ui,
        state: &mut LiteState,
        error: &mut Option<String>,
        info: &mut Option<String>,
    ) {
        ui.group(|ui| {
            ui.heading("Prise externe · Source énergie");
            ui.checkbox(
                &mut state.vm.external_config.enabled,
                "Activer la source externe",
            );
            ui.horizontal(|ui| {
                ui.label("Power file");
                let path = state.vm.external_config.power_file.get_or_insert_default();
                ui.text_edit_singleline(path);
            });
            ui.horizontal(|ui| {
                ui.label("Python");
                let bin = state.vm.external_config.python_bin.get_or_insert_default();
                ui.text_edit_singleline(bin);
            });
            ui.horizontal(|ui| {
                ui.label("Email");
                let email = state
                    .vm
                    .external_config
                    .meross_email
                    .get_or_insert_default();
                ui.text_edit_singleline(email);
            });
            ui.horizontal(|ui| {
                ui.label("Password");
                let password = state
                    .vm
                    .external_config
                    .meross_password
                    .get_or_insert_default();
                ui.add(egui::TextEdit::singleline(password).password(true));
            });
            ui.horizontal(|ui| {
                ui.label("Region");
                let region = state
                    .vm
                    .external_config
                    .meross_region
                    .get_or_insert("eu".to_string());
                ui.text_edit_singleline(region);
                ui.label("Device");
                let device = state
                    .vm
                    .external_config
                    .meross_device_type
                    .get_or_insert("mss315".to_string());
                ui.text_edit_singleline(device);
            });
            ui.horizontal(|ui| {
                if ui.button("Save config").clicked() {
                    match state.save_external_config() {
                        Ok(()) => *info = Some("Config externe enregistrée".to_string()),
                        Err(err) => *error = Some(err),
                    }
                }
                if ui.button("Start bridge").clicked() {
                    match state.start_external_bridge() {
                        Ok(()) => *info = Some("Bridge externe démarré".to_string()),
                        Err(err) => *error = Some(err),
                    }
                }
                if ui.button("Stop bridge").clicked() {
                    match state.stop_external_bridge() {
                        Ok(()) => *info = Some("Bridge externe arrêté".to_string()),
                        Err(err) => *error = Some(err),
                    }
                }
            });
            ui.separator();
            ui.label(format!(
                "Status: {} · frais={} · bridge={}",
                state.vm.external_status.source_tag,
                state.vm.external_status.is_fresh,
                if state.vm.external_bridge_running {
                    "on"
                } else {
                    "off"
                }
            ));
            ui.label(format!(
                "Dernier mur: {}",
                fmt::watts(state.vm.external_status.last_watts)
            ));
            ui.label(format!(
                "Fichier: {}",
                state.vm.external_status.power_file_path
            ));
            ui.label(format!(
                "Bridge log: {}",
                state.vm.external_status.bridge_log_path
            ));
        });
    }

    fn benchmark_panel(
        ui: &mut egui::Ui,
        state: &mut LiteState,
        error: &mut Option<String>,
        info: &mut Option<String>,
    ) {
        ui.group(|ui| {
            ui.heading("Benchmark A/B");
            ui.checkbox(
                &mut state.vm.benchmark_use_system_probe,
                "Sonde système intégrée",
            );
            ui.horizontal(|ui| {
                ui.label("Command");
                ui.add_enabled(
                    !state.vm.benchmark_use_system_probe,
                    egui::TextEdit::singleline(&mut state.vm.benchmark_command),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Args");
                ui.add_enabled(
                    !state.vm.benchmark_use_system_probe,
                    egui::TextEdit::singleline(&mut state.vm.benchmark_args),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Cwd");
                ui.add_enabled(
                    !state.vm.benchmark_use_system_probe,
                    egui::TextEdit::singleline(&mut state.vm.benchmark_cwd),
                );
            });
            ui.add(
                egui::Slider::new(&mut state.vm.benchmark_runs_per_state, 1..=8).text("Runs/state"),
            );
            ui.add(
                egui::Slider::new(&mut state.vm.benchmark_duration_ms, 500..=20_000)
                    .text("Probe ms"),
            );
            ui.add(
                egui::Slider::new(&mut state.vm.benchmark_settle_ms, 250..=5_000).text("Settle ms"),
            );
            if ui.button("Lancer benchmark A/B").clicked() {
                match state.run_benchmark() {
                    Ok(()) => *info = Some("Benchmark A/B terminé".to_string()),
                    Err(err) => *error = Some(err),
                }
            }
            if let Some(session) = &state.vm.benchmark_last_session {
                ui.separator();
                ui.label(format!(
                    "Dernier score: median {:.2?}% · p95 {:.2?}% · eff {:.2?}",
                    session.summary.gain_median_pct,
                    session.summary.gain_p95_pct,
                    session.summary.efficiency_score
                ));
            }
            if let Some(history) = &state.vm.benchmark_history {
                ui.label(format!("Historique: {} sessions", history.sessions.len()));
                if let Some(advice) = &history.advice {
                    ui.label(format!(
                        "Advice κ {:.1} · Σmax {:.2} · η {:.2} · policy {}",
                        advice.recommended_kappa,
                        advice.recommended_sigma_max,
                        advice.recommended_eta,
                        advice.recommended_policy_mode
                    ));
                }
            }
        });
    }

    fn power_audit_panel(ui: &mut egui::Ui, state: &LiteState) {
        let wall = state.vm.external_status.last_watts;
        let host = state.vm.metrics.raw.power_watts;
        let explained = if let (Some(wall), Some(host)) = (wall, host) {
            if wall > 0.0 {
                Some((host / wall * 100.0).clamp(0.0, 100.0))
            } else {
                None
            }
        } else {
            None
        };
        let unattributed = match (wall, host) {
            (Some(wall), Some(host)) => Some((wall - host).max(0.0)),
            _ => None,
        };
        ui.group(|ui| {
            ui.heading("Audit puissance");
            ui.label(format!(
                "Mur {} · host {} · expliqué {} · non attribué {}",
                fmt::watts(wall),
                fmt::watts(host),
                explained
                    .map(|v| format!("{v:.1} %"))
                    .unwrap_or_else(|| "N/A".to_string()),
                fmt::watts(unattributed)
            ));
            ui.separator();
            ui.label(format!(
                "Inventaire: {} displays · {} GPU · {} storage · {} net · {} endpoints",
                state.vm.device_inventory.displays.len(),
                state.vm.device_inventory.gpus.len(),
                state.vm.device_inventory.storage.len(),
                state.vm.device_inventory.network.len(),
                state.vm.device_inventory.connected_endpoints.len()
            ));
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    for item in state
                        .vm
                        .device_inventory
                        .connected_endpoints
                        .iter()
                        .take(18)
                    {
                        ui.horizontal_wrapped(|ui| {
                            ui.strong(&item.name);
                            ui.label(format!("[{}]", item.kind));
                            if let Some(status) = &item.status {
                                ui.label(status);
                            }
                            if let Some(detail) = &item.detail {
                                ui.label(detail);
                            }
                        });
                    }
                });
        });
    }

    fn hud_panel(ui: &mut egui::Ui, state: &mut LiteState) {
        ui.group(|ui| {
            ui.heading("HUD natif");
            ui.checkbox(&mut state.vm.show_hud, "Afficher le HUD compact");
            ui.label("Mode lite: HUD compact natif intégré, sans WebView.");
        });
    }

    fn hud_overlay(ctx: &egui::Context, vm: &LiteViewModel) {
        egui::Window::new("SoulKernel HUD")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 16.0))
            .show(ctx, |ui| {
                ui.strong(format!(
                    "Σ {:.3} · π {:.3} · {}",
                    vm.metrics.sigma,
                    vm.formula.pi,
                    fmt::watts(vm.metrics.raw.power_watts)
                ));
                ui.label(format!(
                    "CPU {} · RAM {}",
                    fmt::pct(vm.metrics.raw.cpu_pct),
                    fmt::gib_pair(vm.metrics.raw.mem_used_mb, vm.metrics.raw.mem_total_mb)
                ));
                ui.label(format!(
                    "GPU {} · I/O {:.2}/{:.2}",
                    fmt::opt_pct(vm.metrics.raw.gpu_pct),
                    vm.metrics.raw.io_read_mb_s.unwrap_or(0.0),
                    vm.metrics.raw.io_write_mb_s.unwrap_or(0.0)
                ));
            });
    }

    fn right_panel(
        ui: &mut egui::Ui,
        state: &mut LiteState,
        error: &mut Option<String>,
        info: &mut Option<String>,
    ) {
        ui.group(|ui| {
            ui.heading("Pilotage · Actions");
            egui::ComboBox::from_label("Workload")
                .selected_text(state.vm.selected_workload.clone())
                .show_ui(ui, |ui| {
                    for workload in &state.vm.workloads {
                        ui.selectable_value(
                            &mut state.vm.selected_workload,
                            workload.id.clone(),
                            format!("{} · {}", workload.label, workload.category),
                        );
                    }
                });
            egui::ComboBox::from_label("Policy")
                .selected_text(state.vm.policy_mode.as_name())
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut state.vm.policy_mode,
                        soulkernel_core::platform::PolicyMode::Privileged,
                        "privileged",
                    );
                    ui.selectable_value(
                        &mut state.vm.policy_mode,
                        soulkernel_core::platform::PolicyMode::Safe,
                        "safe",
                    );
                });
            ui.add(egui::Slider::new(&mut state.vm.kappa, 0.5..=5.0).text("κ"));
            ui.add(egui::Slider::new(&mut state.vm.sigma_max, 0.3..=0.95).text("Σmax"));
            ui.add(egui::Slider::new(&mut state.vm.eta, 0.01..=0.5).text("η"));
            ui.add(egui::Slider::new(&mut state.vm.soulram_percent, 5..=60).text("SoulRAM %"));
            ui.checkbox(&mut state.vm.auto_target, "Auto-cible");
            egui::ComboBox::from_label("Cible manuelle")
                .selected_text(
                    state
                        .vm
                        .manual_target_pid
                        .map(|pid| format!("PID {pid}"))
                        .unwrap_or_else(|| "aucune".to_string()),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.vm.manual_target_pid, None, "aucune");
                    for proc_ in &state.vm.process_report.top_processes {
                        if proc_.is_self_process {
                            continue;
                        }
                        ui.selectable_value(
                            &mut state.vm.manual_target_pid,
                            Some(proc_.pid),
                            format!(
                                "{} · {} · PID {}",
                                proc_.name,
                                fmt::pct(proc_.cpu_usage_pct),
                                proc_.pid
                            ),
                        );
                    }
                });
            ui.label(format!(
                "Cible auto: {}",
                state
                    .vm
                    .target_pid
                    .map(|pid| format!("PID {pid}"))
                    .unwrap_or_else(|| "aucune".to_string())
            ));

            ui.horizontal(|ui| {
                if ui.button("Refresh").clicked() {
                    if let Err(err) = state.refresh_now() {
                        *error = Some(err);
                    }
                }
                if ui.button("Export JSON").clicked() {
                    match export::export_snapshot(&state.vm) {
                        Ok(path) => *info = Some(format!("Export écrit: {path}")),
                        Err(err) => *error = Some(err),
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Dome ON").clicked() {
                    if let Err(err) = state.activate_dome() {
                        *error = Some(err);
                    }
                }
                if ui.button("Rollback").clicked() {
                    if let Err(err) = state.rollback_dome() {
                        *error = Some(err);
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button("SoulRAM ON").clicked() {
                    if let Err(err) = state.enable_soulram() {
                        *error = Some(err);
                    }
                }
                if ui.button("SoulRAM OFF").clicked() {
                    if let Err(err) = state.disable_soulram() {
                        *error = Some(err);
                    }
                }
            });
            ui.separator();
            ui.label(format!("Policy {}", state.vm.policy_mode.as_name()));
            ui.label(format!(
                "SoulRAM backend {}",
                state.vm.soulram_backend.backend
            ));
            ui.label(format!("Audit {}", state.vm.audit_path));
        });

        ui.add_space(8.0);
        ui.group(|ui| {
            ui.heading("Impact processus");
            ui.label(format!(
                "{} processus | top {} | SoulKernel {:.1}% CPU / {} | WebView {:.1}% CPU / {}",
                state.vm.process_report.summary.process_count,
                state.vm.process_report.summary.top_count,
                state.vm.process_report.summary.self_cpu_usage_pct,
                fmt::mib_from_kb(state.vm.process_report.summary.self_memory_kb),
                state.vm.process_report.summary.webview_cpu_usage_pct,
                fmt::mib_from_kb(state.vm.process_report.summary.webview_memory_kb)
            ));
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    for proc_ in &state.vm.process_report.top_processes {
                        ui.horizontal_wrapped(|ui| {
                            ui.strong(&proc_.name);
                            ui.label(format!("#{}", proc_.pid));
                            ui.label(fmt::pct(proc_.cpu_usage_pct));
                            ui.label(fmt::mib_from_kb(proc_.memory_kb));
                            ui.label(fmt::io_pair(
                                proc_.disk_read_bytes,
                                proc_.disk_written_bytes,
                            ));
                            ui.label(fmt::runtime_short(proc_.run_time_s));
                            if proc_.is_self_process {
                                ui.label("SELF");
                            } else if proc_.is_embedded_webview {
                                ui.label("WV");
                            } else {
                                ui.label(&proc_.status);
                            }
                        });
                    }
                });
        });
    }
}

impl eframe::App for LiteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        let Some(state) = self.state.as_mut() else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("SoulKernel Lite");
                if let Some(err) = &self.error {
                    ui.colored_label(egui::Color32::RED, err);
                }
            });
            return;
        };

        if let Err(err) = state.refresh_if_needed() {
            self.error = Some(err);
        }

        let state_vm = &state.vm;
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            Self::top_bar(ui, state_vm);
            ui.separator();
            Self::metrics_strip(ui, state_vm);
            if let Some(info) = &self.info {
                ui.colored_label(egui::Color32::LIGHT_GREEN, info);
            }
            if let Some(err) = &self.error {
                ui.colored_label(egui::Color32::RED, err);
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(3, |columns| {
                Self::left_panel(&mut columns[0], &state.vm);
                Self::center_panel(&mut columns[1], &state.vm);
                Self::right_panel(&mut columns[2], state, &mut self.error, &mut self.info);
            });

            ui.add_space(8.0);
            ui.columns(2, |columns| {
                Self::external_power_panel(&mut columns[0], state, &mut self.error, &mut self.info);
                Self::benchmark_panel(&mut columns[1], state, &mut self.error, &mut self.info);
            });
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                Self::power_audit_panel(&mut columns[0], state);
                Self::hud_panel(&mut columns[1], state);
            });
            ui.add_space(8.0);
            ui.group(|ui| {
                ui.heading("Dernières actions");
                for action in &state.vm.last_actions {
                    ui.label(action);
                }
                if state.vm.last_actions.is_empty() {
                    ui.label("Aucune action récente.");
                }
            });
        });

        if state.vm.show_hud {
            Self::hud_overlay(ctx, &state.vm);
        }
    }
}
