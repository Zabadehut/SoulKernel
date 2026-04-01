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
    }
}
