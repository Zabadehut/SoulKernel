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
    fn tone_for_ratio(value: f64) -> egui::Color32 {
        if value >= 0.85 {
            egui::Color32::from_rgb(210, 84, 84)
        } else if value >= 0.60 {
            egui::Color32::from_rgb(214, 153, 58)
        } else {
            egui::Color32::from_rgb(96, 168, 104)
        }
    }

    fn status_chip(ui: &mut egui::Ui, label: &str, active: bool) {
        let fill = if active {
            egui::Color32::from_rgb(70, 110, 78)
        } else {
            egui::Color32::from_rgb(70, 70, 70)
        };
        egui::Frame::new()
            .fill(fill)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(label)
                        .strong()
                        .color(egui::Color32::WHITE),
                );
            });
    }

    fn metric_badge(ui: &mut egui::Ui, title: &str, value: String, tone: egui::Color32) {
        egui::Frame::new()
            .stroke(egui::Stroke::new(1.0, egui::Color32::DARK_GRAY))
            .corner_radius(6.0)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(title)
                        .small()
                        .color(egui::Color32::GRAY),
                );
                ui.label(egui::RichText::new(value).strong().color(tone));
            });
    }

    fn section_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
        ui.heading(title);
        ui.label(subtitle);
        ui.separator();
    }

    fn top_bar(ui: &mut egui::Ui, vm: &LiteViewModel) {
        ui.horizontal_wrapped(|ui| {
            ui.heading("SoulKernel Lite");
            ui.separator();
            ui.label(format!("OS {}", vm.platform_info.os));
            ui.separator();
            Self::status_chip(ui, "Dome actif", vm.dome_active);
            Self::status_chip(ui, "SoulRAM actif", vm.soulram_active);
            Self::status_chip(ui, "Source externe fraîche", vm.external_status.is_fresh);
            ui.separator();
            ui.label(format!("Sigma {:.3}", vm.metrics.sigma));
            ui.separator();
            ui.label(format!("π {:.3}", vm.formula.pi));
            ui.separator();
            ui.label(format!("Workload {}", vm.selected_workload));
        });
    }

    fn metrics_strip(ui: &mut egui::Ui, vm: &LiteViewModel) {
        let mem_ratio = if vm.metrics.raw.mem_total_mb > 0 {
            vm.metrics.raw.mem_used_mb as f64 / vm.metrics.raw.mem_total_mb as f64
        } else {
            0.0
        };
        let io_level = (vm.metrics.raw.io_read_mb_s.unwrap_or(0.0)
            + vm.metrics.raw.io_write_mb_s.unwrap_or(0.0))
            / 200.0;
        let power_display = vm.metrics.raw.power_watts.or_else(|| {
            if vm.external_status.is_fresh {
                vm.external_status.last_watts
            } else {
                None
            }
        });

        ui.horizontal_wrapped(|ui| {
            Self::metric_badge(
                ui,
                "CPU observé",
                fmt::pct(vm.metrics.raw.cpu_pct),
                Self::tone_for_ratio(vm.metrics.cpu),
            );
            Self::metric_badge(
                ui,
                "RAM observée",
                fmt::gib_pair(vm.metrics.raw.mem_used_mb, vm.metrics.raw.mem_total_mb),
                Self::tone_for_ratio(mem_ratio),
            );
            Self::metric_badge(
                ui,
                "I/O instantané",
                format!(
                    "R {:.2} / W {:.2} MB/s",
                    vm.metrics.raw.io_read_mb_s.unwrap_or(0.0),
                    vm.metrics.raw.io_write_mb_s.unwrap_or(0.0)
                ),
                Self::tone_for_ratio(io_level.clamp(0.0, 1.0)),
            );
            Self::metric_badge(
                ui,
                "GPU observé",
                fmt::opt_pct(vm.metrics.raw.gpu_pct),
                Self::tone_for_ratio(vm.metrics.gpu.unwrap_or(0.0)),
            );
            Self::metric_badge(
                ui,
                "Puissance",
                fmt::watts(power_display),
                if power_display.is_some() {
                    egui::Color32::LIGHT_BLUE
                } else {
                    egui::Color32::GRAY
                },
            );
            Self::metric_badge(
                ui,
                "Décision",
                format!("Σ {:.3} · π {:.3}", vm.metrics.sigma, vm.formula.pi),
                Self::tone_for_ratio(vm.metrics.sigma),
            );
        });
    }

    fn material_overview_panel(ui: &mut egui::Ui, vm: &LiteViewModel) {
        let mem_ratio = if vm.metrics.raw.mem_total_mb > 0 {
            vm.metrics.raw.mem_used_mb as f64 / vm.metrics.raw.mem_total_mb as f64 * 100.0
        } else {
            0.0
        };
        let wall = vm
            .metrics
            .raw
            .wall_power_watts
            .or(if vm.external_status.is_fresh {
                vm.external_status.last_watts
            } else {
                None
            });
        let host = vm.metrics.raw.host_power_watts;
        let host_source =
            fmt::maybe_text(vm.metrics.raw.host_power_watts_source.as_deref(), "aucune");
        let external_source = if vm.external_status.source_tag.trim().is_empty() {
            "aucune".to_string()
        } else {
            vm.external_status.source_tag.clone()
        };
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
        let confidence = if wall.is_some() && host.is_some() {
            if vm.external_status.is_fresh {
                "bonne"
            } else {
                "à rafraîchir"
            }
        } else if wall.is_some() && host.is_none() {
            "mur seul"
        } else if host.is_some() && wall.is_none() {
            "hôte seul"
        } else {
            "faible"
        };
        ui.group(|ui| {
            Self::section_title(
                ui,
                "Matériel interne / externe",
                "Comparer en un bloc ce que l'hôte voit, ce que la prise voit, puis l'écart entre les deux.",
            );
            Self::priority_hint(ui, vm);
            ui.separator();
            ui.columns(3, |columns| {
                columns[0].label(egui::RichText::new("Interne").strong());
                columns[0].label(format!(
                    "CPU {} · {} MHz",
                    fmt::pct(vm.metrics.raw.cpu_pct),
                    vm.metrics.raw.cpu_clock_mhz.unwrap_or(0.0).round()
                ));
                columns[0].label(format!(
                    "RAM {} ({mem_ratio:.0} %)",
                    fmt::gib_pair(vm.metrics.raw.mem_used_mb, vm.metrics.raw.mem_total_mb)
                ));
                columns[0].label(format!(
                    "I/O R {:.2} / W {:.2} MB/s",
                    vm.metrics.raw.io_read_mb_s.unwrap_or(0.0),
                    vm.metrics.raw.io_write_mb_s.unwrap_or(0.0)
                ));
                columns[0].label(format!(
                    "GPU {} · {}",
                    fmt::opt_pct(vm.metrics.raw.gpu_pct),
                    fmt::watts(vm.metrics.raw.gpu_power_watts)
                ));
                columns[0].label(format!(
                    "Watts hôte {} · src {}",
                    fmt::watts(host),
                    host_source
                ));
                columns[0].label(format!(
                    "Temp CPU {} · Load {}",
                    vm.metrics
                        .raw
                        .cpu_temp_c
                        .map(|v| format!("{v:.1} C"))
                        .unwrap_or_else(|| "N/A".to_string()),
                    vm.metrics
                        .raw
                        .load_avg_1m_norm
                        .map(|v| format!("{v:.2} x/core"))
                        .unwrap_or_else(|| "N/A".to_string())
                ));
                columns[0].label(format!(
                    "Runnable {} · faults {}",
                    vm.metrics
                        .raw
                        .runnable_tasks
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "N/A".to_string()),
                    vm.metrics
                        .raw
                        .page_faults_per_sec
                        .map(|v| format!("{v:.1}/s"))
                        .unwrap_or_else(|| "N/A".to_string())
                ));

                columns[1].label(egui::RichText::new("Externe").strong());
                columns[1].label(format!("Watts mur {}", fmt::watts(wall)));
                columns[1].label(format!(
                    "Fraîcheur {}",
                    if vm.external_status.is_fresh { "fraîche" } else { "stale" }
                ));
                columns[1].label(format!(
                    "Bridge {}",
                    if vm.external_bridge_running { "actif" } else { "arrêté" }
                ));
                columns[1].label(format!("Source {}", external_source));
                columns[1].label(format!(
                    "Fichier {}",
                    vm.external_status.power_file_path
                ));

                columns[2].label(egui::RichText::new("Écart").strong());
                columns[2].label(format!(
                    "Expliqué {}",
                    explained
                        .map(|v| format!("{v:.1} %"))
                        .unwrap_or_else(|| "N/A".to_string())
                ));
                columns[2].label(format!("Non attribué {}", fmt::watts(unattributed)));
                columns[2].label(format!("Confiance {}", confidence));
                columns[2].label(if host.is_none() {
                    "Comparaison host↔mur indisponible: pas de watt interne natif.".to_string()
                } else if wall.is_none() {
                    "Comparaison host↔mur indisponible: pas de watt externe frais.".to_string()
                } else {
                    "Comparaison host↔mur mesurable.".to_string()
                });
                columns[2].label(format!(
                    "Compression {} · zRAM {}",
                    vm.metrics
                        .compression
                        .map(|v| format!("{v:.3}"))
                        .unwrap_or_else(|| "N/A".to_string()),
                    vm.metrics
                        .raw
                        .zram_used_mb
                        .map(|v| format!("{v} MiB"))
                        .unwrap_or_else(|| "N/A".to_string())
                ));
                columns[2].label(format!(
                    "PSI CPU {} · MEM {}",
                    vm.metrics
                        .raw
                        .psi_cpu
                        .map(|v| format!("{:.1}%", v * 100.0))
                        .unwrap_or_else(|| "N/A".to_string()),
                    vm.metrics
                        .raw
                        .psi_mem
                        .map(|v| format!("{:.1}%", v * 100.0))
                        .unwrap_or_else(|| "N/A".to_string())
                ));
                columns[2].label(format!(
                    "UI embarquée {:.1}% CPU · {}",
                    vm.metrics.raw.webview_host_cpu_sum.unwrap_or(0.0),
                    fmt::mib_from_kb(vm.metrics.raw.webview_host_mem_mb.unwrap_or(0) * 1024)
                ));
                columns[2].label(format!(
                    "Batterie {} · charge {}",
                    vm.metrics
                        .raw
                        .on_battery
                        .map(|v| if v { "sur batterie" } else { "sur secteur" }.to_string())
                        .unwrap_or_else(|| "N/A".to_string()),
                    vm.metrics
                        .raw
                        .battery_percent
                        .map(|v| format!("{v:.0}%"))
                        .unwrap_or_else(|| "N/A".to_string())
                ));
            });
        });
    }

    fn target_summary(ui: &mut egui::Ui, vm: &LiteViewModel) {
        let target = vm
            .target_pid
            .and_then(|pid| {
                vm.process_report
                    .top_processes
                    .iter()
                    .find(|p| p.pid == pid)
            })
            .map(|proc_| {
                format!(
                    "{} · PID {} · {}",
                    proc_.name,
                    proc_.pid,
                    fmt::pct(proc_.cpu_usage_pct)
                )
            })
            .unwrap_or_else(|| "aucune cible active".to_string());
        ui.label(egui::RichText::new("1. Cible").strong());
        ui.label(format!(
            "Mode {} · cible retenue {}",
            if vm.auto_target { "auto" } else { "manuel" },
            target
        ));
        if vm.auto_target {
            ui.label("Auto-cible retient le premier processus utile hors SoulKernel/UI.");
        }
    }

    fn tuning_summary(ui: &mut egui::Ui, vm: &LiteViewModel) {
        ui.label(egui::RichText::new("2. Réglage").strong());
        ui.label(format!(
            "Policy {} · κ {:.2} · Σmax {:.2} · η {:.2} · SoulRAM {}%",
            vm.policy_mode.as_name(),
            vm.kappa,
            vm.sigma_max,
            vm.eta,
            vm.soulram_percent
        ));
    }

    fn action_summary(ui: &mut egui::Ui, vm: &LiteViewModel) {
        ui.label(egui::RichText::new("3. Action").strong());
        ui.label(format!(
            "Dôme {} · SoulRAM {} · backend {}",
            if vm.dome_active { "actif" } else { "inactif" },
            if vm.soulram_active {
                "actif"
            } else {
                "inactif"
            },
            vm.soulram_backend.backend
        ));
        if !vm.last_actions.is_empty() {
            ui.label(format!("Dernière action: {}", vm.last_actions[0]));
        }
    }

    fn priority_hint(ui: &mut egui::Ui, vm: &LiteViewModel) {
        let (title, body, color) = if vm.metrics.sigma >= vm.sigma_max {
            (
                "Pression élevée",
                "La machine est déjà tendue. Réduire l'agressivité ou cibler plus finement.",
                egui::Color32::from_rgb(210, 84, 84),
            )
        } else if vm.formula.pi >= 0.6 {
            (
                "Fenêtre favorable",
                "Le contexte est bon pour activer le dôme sur une cible utile.",
                egui::Color32::from_rgb(96, 168, 104),
            )
        } else {
            (
                "Impact modéré",
                "Le gain attendu semble limité. Vérifier d'abord la cible et la charge réelle.",
                egui::Color32::from_rgb(214, 153, 58),
            )
        };
        ui.colored_label(color, egui::RichText::new(title).strong());
        ui.label(body);
    }

    fn decision_panel(ui: &mut egui::Ui, vm: &LiteViewModel) {
        ui.group(|ui| {
            Self::section_title(
                ui,
                "Lecture synthétique",
                "Trois réponses rapides: tension, gain attendu, garde.",
            );
            let pressure = if vm.metrics.sigma >= vm.sigma_max {
                ("Tension haute", egui::Color32::from_rgb(210, 84, 84))
            } else if vm.metrics.sigma >= vm.sigma_max * 0.75 {
                ("Tension surveillée", egui::Color32::from_rgb(214, 153, 58))
            } else {
                ("Tension basse", egui::Color32::from_rgb(96, 168, 104))
            };
            let expected_gain = if vm.formula.pi >= 0.6 {
                ("Gain attendu bon", egui::Color32::from_rgb(96, 168, 104))
            } else if vm.formula.pi >= 0.35 {
                ("Gain attendu modéré", egui::Color32::from_rgb(214, 153, 58))
            } else {
                ("Gain attendu faible", egui::Color32::from_rgb(210, 84, 84))
            };
            let guard = if vm.formula.advanced_guard >= 0.85 {
                ("Garde ouverte", egui::Color32::from_rgb(96, 168, 104))
            } else if vm.formula.advanced_guard >= 0.5 {
                ("Garde prudente", egui::Color32::from_rgb(214, 153, 58))
            } else {
                ("Garde fermée", egui::Color32::from_rgb(210, 84, 84))
            };

            ui.colored_label(
                pressure.1,
                format!(
                    "{} · Sigma {:.3} / Σmax {:.3}",
                    pressure.0, vm.metrics.sigma, vm.sigma_max
                ),
            );
            ui.colored_label(
                expected_gain.1,
                format!(
                    "{} · π {:.3} · opportunité {:.3}",
                    expected_gain.0, vm.formula.pi, vm.formula.opportunity
                ),
            );
            ui.colored_label(
                guard.1,
                format!(
                    "{} · garde {:.3} · 𝒟 {:.3}",
                    guard.0, vm.formula.advanced_guard, vm.formula.dome_gain
                ),
            );
        });
    }

    fn telemetry_panel(ui: &mut egui::Ui, vm: &LiteViewModel) {
        ui.group(|ui| {
            Self::section_title(
                ui,
                "Impact cumulé",
                "Historique énergie et différentiels. Ce n'est pas le live instantané.",
            );
            ui.label(format!(
                "Source énergie suivie: {}",
                vm.telemetry.power_source
            ));
            ui.label(format!(
                "CPU·h diff. {:.2} · RAM·GB·h diff. {:.2}",
                vm.telemetry.total.cpu_hours_differential,
                vm.telemetry.total.mem_gb_hours_differential
            ));
            ui.label(format!(
                "Échantillons {} · durée {:.2} h · activité dôme {:.0}%",
                vm.telemetry.total.samples,
                vm.telemetry.total.duration_h,
                vm.telemetry.total.dome_active_ratio * 100.0
            ));
            ui.label(format!(
                "kWh total {:.3} · CO₂ {:.3} kg · coût {:.2} {}",
                vm.telemetry.lifetime.total_energy_kwh,
                vm.telemetry.lifetime.total_co2_measured_kg,
                vm.telemetry.lifetime.total_energy_cost_measured,
                vm.telemetry.pricing.currency
            ));
            ui.label(format!(
                "Puiss. moy. {} · dôme ON {} · dôme OFF {}",
                fmt::watts(vm.telemetry.total.avg_power_w),
                fmt::watts(vm.telemetry.total.avg_power_dome_on_w),
                fmt::watts(vm.telemetry.total.avg_power_dome_off_w)
            ));
            ui.label(format!(
                "Énergie dôme estimée {} · idle {:.0}% · media {:.0}%",
                vm.telemetry
                    .total
                    .energy_saved_kwh
                    .map(|v| format!("{v:.3} kWh"))
                    .unwrap_or_else(|| "N/A".to_string()),
                vm.telemetry.total.idle_ratio * 100.0,
                vm.telemetry.total.media_ratio * 100.0
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
            Self::section_title(
                ui,
                "Source externe",
                "Mesure murale optionnelle. Sert à réconcilier les watts machine avec une prise réelle.",
            );
            ui.checkbox(
                &mut state.vm.external_config.enabled,
                "Activer la source externe",
            );
            ui.horizontal(|ui| {
                ui.label("Power file");
                let path = state.vm.external_config.power_file.get_or_insert_with(|| {
                    soulkernel_core::external_power::default_power_file()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default()
                });
                ui.text_edit_singleline(path);
            });
            ui.horizontal(|ui| {
                ui.label("Python");
                let bin = state.vm.external_config.python_bin.get_or_insert_with(|| {
                    if cfg!(target_os = "windows") {
                        "py".to_string()
                    } else {
                        "python3".to_string()
                    }
                });
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
            ui.label(format!("Bridge état: {}", state.vm.external_bridge_detail));
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
            Self::section_title(
                ui,
                "Benchmark A/B",
                "Comparer OFF vs ON sur une sonde système ou une commande cible.",
            );
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

    fn inventory_panel(ui: &mut egui::Ui, state: &LiteState) {
        fn endpoint_weight(kind: &str) -> f64 {
            let kind = kind.to_ascii_lowercase();
            if kind.contains("display_output")
                || kind.contains("monitor")
                || kind.contains("display")
            {
                2.4
            } else if kind.contains("usb_hub") {
                0.9
            } else if kind.contains("usb") {
                0.6
            } else if kind.contains("audio") || kind.contains("jack") {
                0.5
            } else if kind.contains("bluetooth") {
                0.25
            } else if kind.contains("hid") {
                0.15
            } else if kind.contains("port") {
                0.35
            } else {
                0.3
            }
        }

        fn render_inventory_items(
            ui: &mut egui::Ui,
            title: &str,
            items: &[soulkernel_core::inventory::DeviceInventoryItem],
            endpoint_budget_w: Option<f64>,
            total_weight: f64,
        ) {
            if items.is_empty() {
                return;
            }
            ui.label(egui::RichText::new(title).strong());
            for item in items {
                let estimated_w = endpoint_budget_w
                    .map(|budget| budget * (endpoint_weight(&item.kind) / total_weight));
                ui.horizontal_wrapped(|ui| {
                    ui.strong(&item.name);
                    ui.label(format!("[{}]", item.kind));
                    ui.label(crate::fmt::watts(estimated_w));
                    if let Some(scope) = &item.measurement_scope {
                        ui.label(format!("preuve {scope}"));
                    }
                    if let Some(active_state) = &item.active_state {
                        ui.label(format!("etat {active_state}"));
                    } else if let Some(status) = &item.status {
                        ui.label(status);
                    }
                    if let Some(link) = &item.physical_link_hint {
                        ui.label(format!("lien {link}"));
                    }
                    if let Some(score) = item.confidence_score {
                        ui.label(format!("fiab {:.0}%", score * 100.0));
                    }
                    if let Some(detail) = &item.detail {
                        ui.label(detail);
                    }
                });
            }
            ui.separator();
        }

        ui.group(|ui| {
            Self::section_title(
                ui,
                "Inventaire matériel",
                "Contexte physique connecté: écrans, GPU, stockage, réseau et périphériques.",
            );
            let endpoint_budget_w = state
                .vm
                .external_status
                .last_watts
                .filter(|_| state.vm.external_status.is_fresh)
                .or(state.vm.metrics.raw.power_watts)
                .map(|w| w * 0.18);
            let total_weight = state
                .vm
                .device_inventory
                .connected_endpoints
                .iter()
                .map(|item| endpoint_weight(&item.kind))
                .sum::<f64>()
                .max(0.0001);
            ui.label(format!(
                "Inventaire: {} displays · {} GPU · {} storage · {} net · {} endpoints",
                state.vm.device_inventory.displays.len(),
                state.vm.device_inventory.gpus.len(),
                state.vm.device_inventory.storage.len(),
                state.vm.device_inventory.network.len(),
                state.vm.device_inventory.connected_endpoints.len()
            ));
            egui::ScrollArea::vertical()
                .id_salt("endpoints_scroll")
                .max_height(260.0)
                .show(ui, |ui| {
                    // Displays, GPU, storage, network: no estimated watts — real data is in
                    // item.detail already (e.g. NVML watts). Fake apportioned estimates
                    // would show misleading numbers for devices with no power telemetry.
                    render_inventory_items(
                        ui,
                        "Displays",
                        &state.vm.device_inventory.displays,
                        None,
                        total_weight,
                    );
                    render_inventory_items(
                        ui,
                        "GPU",
                        &state.vm.device_inventory.gpus,
                        None,
                        total_weight,
                    );
                    render_inventory_items(
                        ui,
                        "Storage",
                        &state.vm.device_inventory.storage,
                        None,
                        total_weight,
                    );
                    render_inventory_items(
                        ui,
                        "Network",
                        &state.vm.device_inventory.network,
                        None,
                        total_weight,
                    );
                    render_inventory_items(
                        ui,
                        "Power",
                        &state.vm.device_inventory.power,
                        None,
                        total_weight,
                    );
                    // Connected endpoints: show apportioned estimate only here,
                    // where real per-port telemetry is absent.
                    render_inventory_items(
                        ui,
                        "Endpoints",
                        &state.vm.device_inventory.connected_endpoints,
                        endpoint_budget_w,
                        total_weight,
                    );
                });
        });
    }

    fn hud_panel(ui: &mut egui::Ui, state: &mut LiteState) {
        ui.group(|ui| {
            Self::section_title(
                ui,
                "HUD natif",
                "Mini vue toujours visible pour les signaux utiles pendant l'usage.",
            );
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

    fn pilotage_panel(
        ui: &mut egui::Ui,
        state: &mut LiteState,
        error: &mut Option<String>,
        info: &mut Option<String>,
    ) {
        ui.group(|ui| {
            Self::section_title(
                ui,
                "Pilotage",
                "Parcours court: choisir une cible, régler l'agressivité, puis agir.",
            );
            Self::target_summary(ui, &state.vm);
            ui.checkbox(&mut state.vm.auto_target, "Auto-cible");
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
                "PID actif: {}",
                state
                    .vm
                    .target_pid
                    .map(|pid| format!("PID {pid}"))
                    .unwrap_or_else(|| "aucune".to_string())
            ));
            Self::tuning_summary(ui, &state.vm);
            ui.collapsing("Réglages avancés", |ui| {
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
            });
            Self::action_summary(ui, &state.vm);
            ui.separator();
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
            ui.label(format!(
                "SoulRAM backend {}",
                state.vm.soulram_backend.backend
            ));
            ui.label(format!("Audit {}", state.vm.audit_path));
        });
    }

    fn processes_panel(ui: &mut egui::Ui, state: &LiteState) {
        ui.group(|ui| {
            Self::section_title(
                ui,
                "Processus observés",
                "Vue attributionnelle: qui consomme quoi dans le top courant.",
            );
            ui.label(format!(
                "{} processus | top {} | SoulKernel {:.1}% CPU / {} | UI embarquée {:.1}% CPU / {}",
                state.vm.process_report.summary.process_count,
                state.vm.process_report.summary.top_count,
                state.vm.process_report.summary.self_cpu_usage_pct,
                fmt::mib_from_kb(state.vm.process_report.summary.self_memory_kb),
                state.vm.process_report.summary.webview_cpu_usage_pct,
                fmt::mib_from_kb(state.vm.process_report.summary.webview_memory_kb)
            ));
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("processes_scroll")
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

    fn recent_actions_panel(ui: &mut egui::Ui, actions: &[String]) {
        ui.group(|ui| {
            Self::section_title(
                ui,
                "Dernières actions",
                "Historique court des actions appliquées par SoulKernel.",
            );
            for action in actions {
                ui.label(action);
            }
            if actions.is_empty() {
                ui.label("Aucune action récente.");
            }
        });
    }
}

impl eframe::App for LiteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let Some(state) = self.state.as_mut() else {
            ctx.request_repaint_after(std::time::Duration::from_millis(1000));
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("SoulKernel Lite");
                if let Some(err) = &self.error {
                    ui.colored_label(egui::Color32::RED, err);
                }
            });
            return;
        };

        let repaint_ms = if state.vm.show_hud || state.is_refresh_in_flight() {
            250
        } else {
            1000
        };
        ctx.request_repaint_after(std::time::Duration::from_millis(repaint_ms));

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
            egui::ScrollArea::vertical()
                .id_salt("central_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.push_id("dashboard_columns", |ui| {
                        ui.columns(2, |columns| {
                            Self::material_overview_panel(&mut columns[0], &state.vm);
                            columns[0].add_space(8.0);
                            Self::decision_panel(&mut columns[0], &state.vm);
                            columns[0].add_space(8.0);
                            Self::processes_panel(&mut columns[0], state);
                            columns[0].add_space(8.0);
                            Self::telemetry_panel(&mut columns[0], &state.vm);
                            columns[0].add_space(8.0);
                            Self::benchmark_panel(
                                &mut columns[0],
                                state,
                                &mut self.error,
                                &mut self.info,
                            );

                            Self::pilotage_panel(
                                &mut columns[1],
                                state,
                                &mut self.error,
                                &mut self.info,
                            );
                            columns[1].add_space(8.0);
                            Self::inventory_panel(&mut columns[1], state);
                            columns[1].add_space(8.0);
                            Self::external_power_panel(
                                &mut columns[1],
                                state,
                                &mut self.error,
                                &mut self.info,
                            );
                            columns[1].add_space(8.0);
                            Self::hud_panel(&mut columns[1], state);
                            columns[1].add_space(8.0);
                            Self::recent_actions_panel(&mut columns[1], &state.vm.last_actions);
                        });
                    });
                }); // ScrollArea
        });

        if state.vm.show_hud {
            Self::hud_overlay(ctx, &state.vm);
        }
    }
}
