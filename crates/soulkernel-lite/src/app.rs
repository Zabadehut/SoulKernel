use crate::export;
use crate::fmt;
use crate::state::{HostImpactDelta, LiteState, LiteViewModel};
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
            ui.heading("SoulKernel");
            ui.separator();
            ui.label(format!("OS {}", vm.platform_info.os));
            ui.separator();
            Self::status_chip(ui, "Dome", vm.dome_active);
            Self::status_chip(ui, "SoulRAM", vm.soulram_active);
            ui.separator();
            // Puissance : la vraie valeur, pas une formule
            let power = vm.metrics.raw.host_power_watts
                .or_else(|| vm.metrics.raw.wall_power_watts)
                .or_else(|| if vm.external_status.is_fresh { vm.external_status.last_watts } else { None });
            ui.label(format!("Puissance {}", fmt::watts(power)));
            ui.separator();
            // RAM : % d'utilisation direct
            if vm.metrics.raw.mem_total_mb > 0 {
                let pct = vm.metrics.raw.mem_used_mb as f64 / vm.metrics.raw.mem_total_mb as f64 * 100.0;
                ui.colored_label(
                    Self::tone_for_ratio(pct / 100.0),
                    format!("RAM {:.0}%", pct),
                );
            }
            ui.separator();
            ui.label(format!("CPU {}", fmt::pct(vm.metrics.raw.cpu_pct)));
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
        let io_total = vm.metrics.raw.io_read_mb_s.unwrap_or(0.0)
            + vm.metrics.raw.io_write_mb_s.unwrap_or(0.0);
        let power_display = vm
            .metrics
            .raw
            .host_power_watts
            .or(vm.metrics.raw.wall_power_watts)
            .or_else(|| {
                if vm.external_status.is_fresh {
                    vm.external_status.last_watts
                } else {
                    None
                }
            });
        // Plain-language tension level — no formula variables shown.
        let (tension_label, tension_color) = if vm.metrics.sigma >= vm.sigma_max {
            ("Tension haute", egui::Color32::from_rgb(210, 84, 84))
        } else if vm.metrics.sigma >= vm.sigma_max * 0.75 {
            ("Tension mod.", egui::Color32::from_rgb(214, 153, 58))
        } else {
            ("Tension basse", egui::Color32::from_rgb(96, 168, 104))
        };

        ui.horizontal_wrapped(|ui| {
            Self::metric_badge(
                ui,
                "CPU",
                fmt::pct(vm.metrics.raw.cpu_pct),
                Self::tone_for_ratio(vm.metrics.cpu),
            );
            Self::metric_badge(
                ui,
                "RAM",
                fmt::gib_pair(vm.metrics.raw.mem_used_mb, vm.metrics.raw.mem_total_mb),
                Self::tone_for_ratio(mem_ratio),
            );
            if io_total > 0.0 {
                Self::metric_badge(
                    ui,
                    "I/O",
                    format!("R {:.1} / W {:.1} MB/s", vm.metrics.raw.io_read_mb_s.unwrap_or(0.0), vm.metrics.raw.io_write_mb_s.unwrap_or(0.0)),
                    Self::tone_for_ratio((io_total / 200.0).clamp(0.0, 1.0)),
                );
            }
            if let Some(gpu) = vm.metrics.raw.gpu_pct.filter(|&g| g > 0.5) {
                Self::metric_badge(
                    ui,
                    "GPU",
                    fmt::pct(gpu),
                    Self::tone_for_ratio(vm.metrics.gpu.unwrap_or(0.0)),
                );
            }
            Self::metric_badge(
                ui,
                "Puissance",
                fmt::watts(power_display),
                if power_display.is_some() {
                    egui::Color32::LIGHT_BLUE
                } else {
                    egui::Color32::DARK_GRAY
                },
            );
            Self::metric_badge(ui, "Statut", tension_label.to_string(), tension_color);
            // KPI compact dans la barre principale
            {
                use soulkernel_core::kpi::KpiLabel;
                let kpi_color = match vm.kpi.label {
                    KpiLabel::Efficient   => egui::Color32::from_rgb(96, 168, 104),
                    KpiLabel::Moderate    => egui::Color32::from_rgb(214, 153, 58),
                    KpiLabel::Inefficient => egui::Color32::from_rgb(210, 84, 84),
                    KpiLabel::Unknown     => egui::Color32::DARK_GRAY,
                };
                let kpi_val = vm.kpi.kpi_penalized
                    .map(|k| format!("{:.1} W/%", k))
                    .unwrap_or_else(|| "KPI N/A".to_string());
                Self::metric_badge(ui, "KPI", kpi_val, kpi_color);
            }
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
        // Helper: only emit a row if the value string is not "N/A" / empty.
        fn row(ui: &mut egui::Ui, label: &str, value: String) {
            if value != "N/A" && !value.is_empty() {
                ui.label(format!("{label} {value}"));
            }
        }

        let has_battery = vm.metrics.raw.on_battery.is_some()
            && !(vm.metrics.raw.on_battery == Some(false)
                && vm.metrics.raw.battery_percent.unwrap_or(0.0) <= 0.0);

        ui.group(|ui| {
            Self::section_title(
                ui,
                "Matériel interne / externe",
                "Ce que l'hôte voit · ce que la prise voit · l'écart.",
            );
            Self::priority_hint(ui, vm);
            ui.separator();
            ui.columns(3, |columns| {
                // ── Interne ──────────────────────────────────────────────────
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
                // GPU — only when available
                if vm.metrics.raw.gpu_pct.is_some() || vm.metrics.raw.gpu_power_watts.is_some() {
                    columns[0].label(format!(
                        "GPU {} · {}",
                        fmt::opt_pct(vm.metrics.raw.gpu_pct),
                        fmt::watts(vm.metrics.raw.gpu_power_watts)
                    ));
                }
                // Host watts (RAPL / battery discharge / PDH) — absent on Windows desktop
                if let Some(w) = host {
                    columns[0].label(format!("Watts hôte {:.1} W · src {host_source}", w));
                } else {
                    columns[0].label(
                        egui::RichText::new(format!(
                            "Watts hôte N/A ({})",
                            Self::power_unavailable_hint(&vm.platform_info.os)
                        ))
                        .color(egui::Color32::DARK_GRAY)
                        .small(),
                    );
                }
                // Temp — only when measured
                row(
                    &mut columns[0],
                    "Temp CPU",
                    vm.metrics
                        .raw
                        .cpu_temp_c
                        .map(|v| format!("{v:.1} °C"))
                        .unwrap_or_else(|| "N/A".to_string()),
                );
                // Load avg — Linux/macOS only; absent on Windows
                row(
                    &mut columns[0],
                    "Load avg",
                    vm.metrics
                        .raw
                        .load_avg_1m_norm
                        .map(|v| format!("{v:.2} x/core"))
                        .unwrap_or_else(|| "N/A".to_string()),
                );
                // Page faults
                row(
                    &mut columns[0],
                    "Faults",
                    vm.metrics
                        .raw
                        .page_faults_per_sec
                        .map(|v| format!("{v:.0}/s"))
                        .unwrap_or_else(|| "N/A".to_string()),
                );
                // Compression mémoire (Windows) / zRAM (Linux)
                if let Some(ratio) = vm.metrics.compression {
                    let store_mb = ratio * vm.metrics.raw.mem_total_mb as f64;
                    let saved_mb = store_mb * 1.5; // ~2.5x typique Windows/zram
                    row(
                        &mut columns[0],
                        "Store compressé",
                        format!("{:.0} MiB ({:.1}% RAM)", store_mb, ratio * 100.0),
                    );
                    row(
                        &mut columns[0],
                        "RAM économisée ~",
                        format!("{:.0} MiB évités en swap", saved_mb),
                    );
                } else {
                    row(&mut columns[0], "Compression", "N/A".to_string());
                }
                // Swap / pagefile
                {
                    let swap_used = vm.metrics.raw.swap_used_mb;
                    let swap_total = vm.metrics.raw.swap_total_mb;
                    let label = if swap_used == 0 {
                        "inactif".to_string()
                    } else {
                        format!("{} / {} MiB", swap_used, swap_total)
                    };
                    row(&mut columns[0], "Swap/Pagefile", label);
                }
                // zRAM Linux
                if let Some(z) = vm.metrics.raw.zram_used_mb {
                    row(&mut columns[0], "zRAM", format!("{z} MiB"));
                }
                // PSI — Linux only
                if vm.metrics.raw.psi_cpu.is_some() || vm.metrics.raw.psi_mem.is_some() {
                    columns[0].label(format!(
                        "PSI CPU {:.1}% · MEM {:.1}%",
                        vm.metrics.raw.psi_cpu.unwrap_or(0.0) * 100.0,
                        vm.metrics.raw.psi_mem.unwrap_or(0.0) * 100.0
                    ));
                }

                // ── Externe ──────────────────────────────────────────────────
                columns[1].label(egui::RichText::new("Externe").strong());
                if let Some(w) = wall {
                    columns[1].label(format!("Watts mur {w:.1} W"));
                } else {
                    columns[1].label(
                        egui::RichText::new("Watts mur N/A")
                            .color(egui::Color32::DARK_GRAY)
                            .small(),
                    );
                }
                if !external_source.is_empty() && external_source != "aucune" {
                    columns[1].label(format!("Source {external_source}"));
                    columns[1].label(format!(
                        "Fraîcheur {}",
                        if vm.external_status.is_fresh { "fraîche" } else { "stale" }
                    ));
                    columns[1].label(format!(
                        "Bridge {}",
                        if vm.external_bridge_running { "actif" } else { "arrêté" }
                    ));
                    let path = &vm.external_status.power_file_path;
                    if !path.is_empty() {
                        columns[1].label(
                            egui::RichText::new(path)
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    }
                } else {
                    columns[1].label(
                        egui::RichText::new("Aucune prise externe connectée")
                            .color(egui::Color32::DARK_GRAY)
                            .small(),
                    );
                }
                // Battery in Externe block — only for laptops/devices with batteries
                if has_battery {
                    let bat_state = vm
                        .metrics
                        .raw
                        .on_battery
                        .map(|v| if v { "sur batterie" } else { "sur secteur" })
                        .unwrap_or("N/A");
                    let bat_pct = vm
                        .metrics
                        .raw
                        .battery_percent
                        .map(|v| format!("{v:.0}%"))
                        .unwrap_or_else(|| "N/A".to_string());
                    columns[1].label(format!("Batterie {bat_state} · {bat_pct}"));
                }

                // ── Écart ─────────────────────────────────────────────────────
                columns[2].label(egui::RichText::new("Écart").strong());
                match (host, wall) {
                    (Some(h), Some(w)) => {
                        let ratio = (h / w * 100.0).clamp(0.0, 100.0);
                        let unattr = (w - h).max(0.0);
                        columns[2].label(format!("Hôte {h:.1} W / Mur {w:.1} W"));
                        columns[2].label(format!("Hôte représente {ratio:.1}% du mur"));
                        columns[2].label(format!("Non attribué {unattr:.1} W"));
                        columns[2].label(format!("Confiance {confidence}"));
                    }
                    (None, Some(w)) => {
                        columns[2].label(format!("Mur {w:.1} W · hôte non mesuré"));
                        columns[2].label(format!("Confiance {confidence}"));
                        columns[2].label(
                            egui::RichText::new(Self::power_unavailable_hint(
                                &vm.platform_info.os,
                            ))
                            .small()
                            .color(egui::Color32::DARK_GRAY),
                        );
                    }
                    (Some(h), None) => {
                        columns[2].label(format!("Hôte {h:.1} W · prise non connectée"));
                        columns[2].label(format!("Confiance {confidence}"));
                    }
                    (None, None) => {
                        columns[2].label(
                            egui::RichText::new("Aucune mesure de puissance disponible.")
                                .color(egui::Color32::DARK_GRAY),
                        );
                        columns[2].label(
                            egui::RichText::new(Self::power_unavailable_hint(
                                &vm.platform_info.os,
                            ))
                            .small()
                            .color(egui::Color32::DARK_GRAY),
                        );
                    }
                }
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
            .map(|proc_| format!("{} · PID {} · {}", proc_.name, proc_.pid, fmt::pct(proc_.cpu_usage_pct)))
            .unwrap_or_else(|| "aucune cible active".to_string());
        ui.label(format!("Cible  {target}"));
    }

    fn tuning_summary(ui: &mut egui::Ui, vm: &LiteViewModel) {
        ui.label(
            egui::RichText::new(format!(
                "Policy {}  ·  κ {:.2}  ·  Σmax {:.2}  ·  η {:.2}  ·  SoulRAM {}%",
                vm.policy_mode.as_name(),
                vm.kappa,
                vm.sigma_max,
                vm.eta,
                vm.soulram_percent
            ))
            .small()
            .color(egui::Color32::GRAY),
        );
    }

    fn action_summary(ui: &mut egui::Ui, vm: &LiteViewModel) {
        ui.horizontal_wrapped(|ui| {
            Self::status_chip(ui, "Dôme", vm.dome_active);
            Self::status_chip(ui, "SoulRAM", vm.soulram_active);
            ui.label(
                egui::RichText::new(format!("backend {}", vm.soulram_backend.backend))
                    .small()
                    .color(egui::Color32::GRAY),
            );
        });
        if !vm.last_actions.is_empty() {
            ui.label(
                egui::RichText::new(format!("↳ {}", vm.last_actions[0]))
                    .small()
                    .color(egui::Color32::GRAY),
            );
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

    /// Panneau principal : ce que SoulKernel fait concrètement sur ce HOST.
    /// Pas de formules : puissance, RAM, page faults, et delta depuis la dernière action.
    fn host_impact_panel(ui: &mut egui::Ui, vm: &LiteViewModel) {
        ui.group(|ui| {
            Self::section_title(
                ui,
                "Impact HOST",
                "Ce que SoulKernel mesure et canalisé sur cette machine.",
            );

            // ── Puissance live ──────────────────────────────────────────────
            let host_w = vm.metrics.raw.host_power_watts;
            let wall_w = vm.metrics.raw.wall_power_watts.or_else(|| {
                if vm.external_status.is_fresh { vm.external_status.last_watts } else { None }
            });
            let power_src = vm.metrics.raw.host_power_watts_source.as_deref()
                .or(vm.metrics.raw.wall_power_watts_source.as_deref())
                .unwrap_or(if vm.external_status.is_fresh && wall_w.is_some() {
                    "Meross"
                } else {
                    "aucun capteur"
                });

            ui.horizontal_wrapped(|ui| {
                if let Some(w) = host_w.or(wall_w) {
                    Self::metric_badge(
                        ui,
                        "Puissance HOST",
                        format!("{:.1} W  [{}]", w, power_src),
                        egui::Color32::LIGHT_BLUE,
                    );
                } else {
                    Self::metric_badge(
                        ui,
                        "Puissance HOST",
                        format!("N/A — {}", Self::power_unavailable_hint(&vm.platform_info.os)),
                        egui::Color32::DARK_GRAY,
                    );
                }

                // ── RAM sous dôme ──────────────────────────────────────────
                let mem_pct = if vm.metrics.raw.mem_total_mb > 0 {
                    vm.metrics.raw.mem_used_mb as f64 / vm.metrics.raw.mem_total_mb as f64 * 100.0
                } else { 0.0 };
                Self::metric_badge(
                    ui,
                    "RAM utilisée",
                    format!(
                        "{} / {} ({:.0}%)",
                        fmt::mib_from_mb(vm.metrics.raw.mem_used_mb),
                        fmt::mib_from_mb(vm.metrics.raw.mem_total_mb),
                        mem_pct
                    ),
                    Self::tone_for_ratio(mem_pct / 100.0),
                );

                // ── Compression mémoire ───────────────────────────────────
                if let Some(ratio) = vm.metrics.compression {
                    let store_mb = ratio * vm.metrics.raw.mem_total_mb as f64;
                    let saved_mb = store_mb * 1.5; // ratio typique ~2.5x → économie = store * 1.5
                    let swap_used = vm.metrics.raw.swap_used_mb;
                    let faults = vm.metrics.raw.page_faults_per_sec.unwrap_or(0.0);

                    // Verdict : swap inactif + faults faibles = compression efficace
                    let (verdict_text, verdict_color) = if swap_used == 0 && faults < 500.0 {
                        ("bénéfique", egui::Color32::from_rgb(80, 180, 100))
                    } else if swap_used > 0 {
                        ("swap actif — pression élevée", egui::Color32::from_rgb(220, 120, 50))
                    } else {
                        ("active", egui::Color32::GRAY)
                    };

                    Self::metric_badge(
                        ui,
                        "Compression mém.",
                        format!(
                            "store {:.0} MiB · ~{:.0} MiB économisés · {}",
                            store_mb, saved_mb, verdict_text
                        ),
                        verdict_color,
                    );

                    // Swap / pagefile
                    let swap_label = if swap_used == 0 {
                        egui::RichText::new("Swap/Pagefile  inactif")
                            .color(egui::Color32::from_rgb(80, 180, 100))
                            .small()
                    } else {
                        egui::RichText::new(format!(
                            "Swap/Pagefile  {} MiB utilisé / {} MiB total",
                            swap_used, vm.metrics.raw.swap_total_mb
                        ))
                        .color(egui::Color32::from_rgb(220, 120, 50))
                        .small()
                    };
                    ui.label(swap_label);
                }

            });

            // ── Auto-cycle status ─────────────────────────────────────────
            if vm.soulram_active {
                ui.separator();
                if vm.auto_cycle_soulram {
                    match vm.next_cycle_in_s {
                        Some(0) | None => {
                            if let Some(last_ms) = vm.last_auto_cycle_ms {
                                let age = vm.now_ms.saturating_sub(last_ms) / 1000;
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Auto-cycle SoulRAM actif — dernier cycle il y a {age}s"
                                    ))
                                    .small()
                                    .color(egui::Color32::from_rgb(96, 168, 104)),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new("Auto-cycle SoulRAM actif — en attente de charge")
                                        .small()
                                        .color(egui::Color32::GRAY),
                                );
                            }
                        }
                        Some(remaining) => {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Auto-cycle SoulRAM — prochain cycle dans {}",
                                    crate::fmt::runtime_short(remaining)
                                ))
                                .small()
                                .color(egui::Color32::GRAY),
                            );
                        }
                    }
                } else {
                    ui.label(
                        egui::RichText::new(
                            "SoulRAM actif — auto-cycle désactivé (one-shot). Activer dans Commandes.",
                        )
                        .small()
                        .color(egui::Color32::DARK_GRAY),
                    );
                }
            }

            // ── KPI énergétique ───────────────────────────────────────────
            {
                use soulkernel_core::kpi::KpiLabel;
                ui.separator();
                let kpi = &vm.kpi;
                let label_str = kpi.label.as_str();
                let label_color = match kpi.label {
                    KpiLabel::Efficient   => egui::Color32::from_rgb(96, 168, 104),
                    KpiLabel::Moderate    => egui::Color32::from_rgb(214, 153, 58),
                    KpiLabel::Inefficient => egui::Color32::from_rgb(210, 84, 84),
                    KpiLabel::Unknown     => egui::Color32::DARK_GRAY,
                };
                // ── Alerte auto-sabotage ──────────────────────────────────
                if kpi.self_overload {
                    ui.colored_label(
                        egui::Color32::from_rgb(210, 84, 84),
                        format!(
                            "⚠ SoulKernel {:.0}% CPU — l'optimiseur consomme plus qu'il n'optimise",
                            kpi.cpu_self_pct
                        ),
                    );
                }

                ui.horizontal_wrapped(|ui| {
                    let cpu_out_of_scope = (kpi.cpu_total_pct
                        - kpi.cpu_useful_pct
                        - kpi.cpu_overhead_pct
                        - kpi.cpu_system_pct)
                        .max(0.0);
                    // KPI* valeur principale
                    match kpi.kpi_penalized {
                        Some(k) => Self::metric_badge(
                            ui, "KPI énergétique",
                            format!("{:.2} W/%  [{}]", k, label_str),
                            label_color,
                        ),
                        None => Self::metric_badge(
                            ui, "KPI énergétique",
                            "N/A — aucun capteur puissance".to_string(),
                            egui::Color32::DARK_GRAY,
                        ),
                    }
                    // CPU utile retenu par le KPI (somme bottom-up des top-N processus utiles).
                    Self::metric_badge(
                        ui, "CPU utile (top-N)",
                        format!("{:.1}%", kpi.cpu_useful_pct),
                        Self::tone_for_ratio(1.0 - kpi.cpu_useful_pct / 100.0_f64.max(kpi.cpu_total_pct)),
                    );
                    if kpi.cpu_overhead_pct > 1.0 {
                        Self::metric_badge(
                            ui, "Overhead",
                            format!("{:.1}%", kpi.cpu_overhead_pct),
                            egui::Color32::from_rgb(214, 153, 58),
                        );
                    }
                    if cpu_out_of_scope > 1.0 {
                        Self::metric_badge(
                            ui, "CPU hors KPI",
                            format!("{:.1}%", cpu_out_of_scope),
                            egui::Color32::GRAY,
                        );
                    }
                    // Faults / s
                    if let Some(pf) = vm.metrics.raw.page_faults_per_sec {
                        if pf > 0.0 {
                            let faults_k = pf / 1000.0;
                            let fault_color = if pf > 5000.0 {
                                egui::Color32::from_rgb(210, 84, 84)
                            } else if pf > 1500.0 {
                                egui::Color32::from_rgb(214, 153, 58)
                            } else {
                                egui::Color32::GRAY
                            };
                            let warn = if pf > 5000.0 { " ⚠" } else { "" };
                            Self::metric_badge(
                                ui, "Faults mém.",
                                format!("{:.0}k/s{}", faults_k, warn),
                                fault_color,
                            );
                        }
                    }
                    // Tendance Δ KPI*
                    if let Some(trend) = kpi.trend {
                        let (trend_str, trend_color) = if trend > 1.0 {
                            (format!("↑ +{:.2}", trend), egui::Color32::from_rgb(210, 84, 84))
                        } else if trend < -1.0 {
                            (format!("↓ {:.2}", trend), egui::Color32::from_rgb(96, 168, 104))
                        } else {
                            (format!("→ {:.2}", trend), egui::Color32::GRAY)
                        };
                        Self::metric_badge(ui, "Tendance", trend_str, trend_color);
                    }
                    // Ratio apprentissage
                    let ratio = vm.kpi_memory.reward_ratio();
                    if !vm.kpi_memory.records.is_empty() {
                        Self::metric_badge(
                            ui, "Actions efficaces",
                            format!("{:.0}%", ratio * 100.0),
                            if ratio >= 0.6 {
                                egui::Color32::from_rgb(96, 168, 104)
                            } else if ratio >= 0.4 {
                                egui::Color32::from_rgb(214, 153, 58)
                            } else {
                                egui::Color32::from_rgb(210, 84, 84)
                            },
                        );
                    }
                });
            }

            // ── Delta depuis dernière action ──────────────────────────────
            if let Some(delta) = &vm.host_impact {
                ui.separator();
                Self::host_impact_delta_row(ui, delta, vm.now_ms);
            }
        });
    }

    fn host_impact_delta_row(ui: &mut egui::Ui, delta: &HostImpactDelta, now_ms: u64) {
        let age_s = now_ms.saturating_sub(delta.captured_at_ms) / 1000;
        ui.label(egui::RichText::new(
            format!("Résultat dernière action : {}  (il y a {}s)", delta.source, age_s)
        ).strong());
        ui.horizontal_wrapped(|ui| {
            // RAM libérée
            let freed = delta.mem_freed_mb();
            let ram_color = if freed > 50 {
                egui::Color32::from_rgb(96, 168, 104)
            } else if freed < -50 {
                egui::Color32::from_rgb(210, 84, 84)
            } else {
                egui::Color32::GRAY
            };
            Self::metric_badge(
                ui,
                "RAM libérée",
                if freed > 0 {
                    format!("-{} MiB", freed)
                } else if freed < 0 {
                    format!("+{} MiB", freed.abs())
                } else {
                    "stable".to_string()
                },
                ram_color,
            );

            // Page faults réduites
            if let Some(pct) = delta.page_faults_reduction_pct() {
                let color = if pct > 10.0 {
                    egui::Color32::from_rgb(96, 168, 104)
                } else if pct < -10.0 {
                    egui::Color32::from_rgb(210, 84, 84)
                } else {
                    egui::Color32::GRAY
                };
                Self::metric_badge(
                    ui,
                    "Page faults",
                    format!("{:+.0}%", -pct),
                    color,
                );
            } else if delta.page_faults_before.is_some() || delta.page_faults_after.is_some() {
                Self::metric_badge(ui, "Page faults", "mesure en cours".to_string(), egui::Color32::GRAY);
            }

            // Puissance économisée
            if let Some(saved) = delta.power_saved_w() {
                let color = if saved > 1.0 {
                    egui::Color32::from_rgb(96, 168, 104)
                } else if saved < -1.0 {
                    egui::Color32::from_rgb(210, 84, 84)
                } else {
                    egui::Color32::GRAY
                };
                Self::metric_badge(
                    ui,
                    "Puissance",
                    format!("{:+.1} W", -saved),
                    color,
                );
            }

            // Compression avant/après
            if let (Some(before), Some(after)) = (delta.compression_before, delta.compression_after) {
                let delta_ratio = after - before;
                let color = if delta_ratio > 0.02 {
                    egui::Color32::from_rgb(96, 168, 104)
                } else {
                    egui::Color32::GRAY
                };
                Self::metric_badge(
                    ui,
                    "Compression",
                    format!("{:.1}% → {:.1}%", before * 100.0, after * 100.0),
                    color,
                );
            }
        });
    }

    fn power_unavailable_hint(os: &str) -> &'static str {
        if os.contains("Windows") {
            "desktop Windows: branchez un Meross"
        } else if os.contains("macOS") || os.contains("Darwin") {
            "Mac desktop: branchez un Meross"
        } else {
            "RAPL non disponible"
        }
    }

    fn decision_panel(ui: &mut egui::Ui, vm: &LiteViewModel) {
        ui.group(|ui| {
            Self::section_title(
                ui,
                "État système",
                "Pression, fenêtre d'action et garde — en un coup d'œil.",
            );
            let pressure = if vm.metrics.sigma >= vm.sigma_max {
                ("Pression élevée", "La machine est tendue — éviter d'ajouter une charge.")
            } else if vm.metrics.sigma >= vm.sigma_max * 0.75 {
                ("Pression surveillée", "Charge modérée — agir avec précaution.")
            } else {
                ("Pression basse", "La machine est à l'aise.")
            };
            let window = if vm.formula.pi >= 0.6 {
                ("Fenêtre ouverte", "Bon moment pour activer le dôme.")
            } else if vm.formula.pi >= 0.35 {
                ("Fenêtre modérée", "L'action peut avoir un effet limité.")
            } else {
                ("Fenêtre fermée", "Peu de gain attendu dans ce contexte.")
            };
            let guard = if vm.formula.advanced_guard >= 0.85 {
                ("Garde ouverte", "SoulKernel peut agir librement.")
            } else if vm.formula.advanced_guard >= 0.5 {
                ("Garde prudente", "SoulKernel attend un meilleur contexte.")
            } else {
                ("Garde fermée", "SoulKernel bloque l'action pour protéger le HOST.")
            };
            let pressure_color = if vm.metrics.sigma >= vm.sigma_max {
                egui::Color32::from_rgb(210, 84, 84)
            } else if vm.metrics.sigma >= vm.sigma_max * 0.75 {
                egui::Color32::from_rgb(214, 153, 58)
            } else {
                egui::Color32::from_rgb(96, 168, 104)
            };
            let window_color = if vm.formula.pi >= 0.6 {
                egui::Color32::from_rgb(96, 168, 104)
            } else if vm.formula.pi >= 0.35 {
                egui::Color32::from_rgb(214, 153, 58)
            } else {
                egui::Color32::from_rgb(210, 84, 84)
            };
            let guard_color = if vm.formula.advanced_guard >= 0.85 {
                egui::Color32::from_rgb(96, 168, 104)
            } else if vm.formula.advanced_guard >= 0.5 {
                egui::Color32::from_rgb(214, 153, 58)
            } else {
                egui::Color32::from_rgb(210, 84, 84)
            };
            ui.horizontal_wrapped(|ui| {
                Self::metric_badge(ui, pressure.0, pressure.1.to_string(), pressure_color);
                Self::metric_badge(ui, window.0, window.1.to_string(), window_color);
                Self::metric_badge(ui, guard.0, guard.1.to_string(), guard_color);
            });
        });
    }

    fn gains_panel(ui: &mut egui::Ui, vm: &LiteViewModel) {
        let lt = &vm.telemetry.lifetime;
        let mem = &vm.kpi_memory;

        ui.group(|ui| {
            Self::section_title(
                ui,
                "Gains SoulKernel",
                "Ce que l'application a concretement fait pour toi depuis le premier lancement.",
            );

            // ── Ancienneté ────────────────────────────────────────────────────
            if lt.first_launch_ts > 0 {
                let age_h = lt.total_samples as f64 * vm.telemetry.total.avg_power_w
                    .map(|_| 1.0).unwrap_or(1.0); // just for age display
                // Use total_samples * refresh_interval to estimate age
                // refresh ~5s → total_idle_hours + total_dome_hours + reste
                let monitored_h = lt.total_idle_hours + lt.total_dome_hours + lt.soulram_active_hours;
                if monitored_h > 0.01 {
                    ui.label(
                        egui::RichText::new(format!(
                            "Suivi depuis {:.0}h  ({} samples  ·  {:.0}h idle  ·  {:.1}h dôme)",
                            monitored_h,
                            lt.total_samples,
                            lt.total_idle_hours,
                            lt.total_dome_hours,
                        ))
                        .small()
                        .color(egui::Color32::GRAY),
                    );
                }
                let _ = age_h; // unused
            }

            ui.separator();

            // ── Dôme ──────────────────────────────────────────────────────────
            ui.label(egui::RichText::new("Dôme").strong());
            ui.horizontal_wrapped(|ui| {
                Self::metric_badge(
                    ui,
                    "Activations",
                    format!("{}", lt.total_dome_activations),
                    if lt.total_dome_activations > 0 {
                        egui::Color32::from_rgb(96, 168, 104)
                    } else {
                        egui::Color32::GRAY
                    },
                );
                Self::metric_badge(
                    ui,
                    "Temps actif",
                    format!("{:.1}h", lt.total_dome_hours),
                    egui::Color32::GRAY,
                );
                if lt.total_cpu_hours_differential > 0.0 {
                    Self::metric_badge(
                        ui,
                        "CPU·h économisé",
                        format!("{:.3} CPU·h", lt.total_cpu_hours_differential),
                        egui::Color32::from_rgb(96, 168, 104),
                    );
                }
                if lt.total_mem_gb_hours_differential > 0.0 {
                    Self::metric_badge(
                        ui,
                        "RAM·GB·h libérée",
                        format!("{:.3} GB·h", lt.total_mem_gb_hours_differential),
                        egui::Color32::from_rgb(96, 168, 104),
                    );
                }
            });

            // ── SoulRAM ───────────────────────────────────────────────────────
            ui.separator();
            ui.label(egui::RichText::new("SoulRAM").strong());
            ui.horizontal_wrapped(|ui| {
                Self::metric_badge(
                    ui,
                    "Temps actif",
                    format!("{:.1}h", lt.soulram_active_hours),
                    if lt.soulram_active_hours > 0.0 {
                        egui::Color32::from_rgb(96, 168, 104)
                    } else {
                        egui::Color32::GRAY
                    },
                );
            });

            // ── KPI / efficacité des actions ──────────────────────────────────
            ui.separator();
            ui.label(egui::RichText::new("Efficacité des actions").strong());
            ui.horizontal_wrapped(|ui| {
                // Session courante : ratio de récompense
                let reward = mem.reward_ratio();
                Self::metric_badge(
                    ui,
                    "Actions efficaces (session)",
                    format!("{:.0}%", reward * 100.0),
                    if reward >= 0.6 {
                        egui::Color32::from_rgb(96, 168, 104)
                    } else if reward >= 0.4 {
                        egui::Color32::from_rgb(214, 153, 58)
                    } else {
                        egui::Color32::from_rgb(200, 80, 80)
                    },
                );
                // Gain KPI médian session (négatif = amélioration)
                if let Some(avg_delta) = mem.avg_kpi_gain() {
                    Self::metric_badge(
                        ui,
                        "Δ KPI médian (session)",
                        format!("{:+.2} W/%", avg_delta),
                        egui::Color32::from_rgb(96, 168, 104),
                    );
                }
                // Amélioration KPI lifetime (% calculé au fil des ticks)
                if let Some(gain_pct) = lt.avg_kpi_gain_pct {
                    Self::metric_badge(
                        ui,
                        "Amélioration KPI (lifetime)",
                        format!("{:+.1}%", gain_pct),
                        if gain_pct < 0.0 {
                            egui::Color32::from_rgb(96, 168, 104) // négatif = bien
                        } else {
                            egui::Color32::from_rgb(200, 80, 80)
                        },
                    );
                }
            });

            // ── Énergie & coût (si capteur réel disponible) ───────────────────
            ui.separator();
            ui.label(egui::RichText::new("Énergie & coût").strong());
            if lt.has_real_power {
                ui.horizontal_wrapped(|ui| {
                    Self::metric_badge(
                        ui,
                        "Énergie mesurée",
                        format!("{:.3} kWh", lt.total_energy_kwh),
                        egui::Color32::LIGHT_BLUE,
                    );
                    if lt.total_energy_cost_measured > 0.0 {
                        Self::metric_badge(
                            ui,
                            "Coût cumulé",
                            format!("{:.2} {}", lt.total_energy_cost_measured, vm.telemetry.pricing.currency),
                            egui::Color32::LIGHT_BLUE,
                        );
                    }
                    if lt.total_co2_measured_kg > 0.0 {
                        Self::metric_badge(
                            ui,
                            "CO₂ mesuré",
                            format!("{:.3} kg", lt.total_co2_measured_kg),
                            egui::Color32::GRAY,
                        );
                    }
                });
                // Économie dôme session (puissance moy dôme ON vs OFF)
                if let Some(saved) = vm.telemetry.total.energy_saved_kwh.filter(|&v| v > 0.0) {
                    ui.label(
                        egui::RichText::new(format!(
                            "Économie estimée dôme cette session  {:.4} kWh  (~{:.3} {})",
                            saved,
                            saved * vm.telemetry.pricing.price_per_kwh,
                            vm.telemetry.pricing.currency,
                        ))
                        .color(egui::Color32::from_rgb(96, 168, 104)),
                    );
                }
            } else {
                ui.label(
                    egui::RichText::new(
                        "Capteur de puissance non disponible — branchez un Meross ou activez RAPL \
                         pour mesurer kWh et calculer les économies en euros.",
                    )
                    .small()
                    .color(egui::Color32::from_rgb(180, 130, 50)),
                );
                // On peut quand même montrer un estimé si on a des données CPU diff
                if lt.total_cpu_hours_differential > 0.0 {
                    // Estimation très conservatrice : ~0.5 W par point de % CPU (TDP ~100W / 100% / 2)
                    let est_kwh = lt.total_cpu_hours_differential * 0.5;
                    let est_cost = est_kwh * vm.telemetry.pricing.price_per_kwh;
                    ui.label(
                        egui::RichText::new(format!(
                            "Estimation sans capteur (0.5 W/%·CPU)  ~{:.4} kWh  ~{:.3} {}",
                            est_kwh, est_cost, vm.telemetry.pricing.currency,
                        ))
                        .small()
                        .color(egui::Color32::GRAY),
                    );
                }
            }
        });
    }

    fn telemetry_panel(ui: &mut egui::Ui, vm: &LiteViewModel) {
        ui.group(|ui| {
            Self::section_title(
                ui,
                "Impact mesuré",
                "Énergie consommée et gain du dôme depuis le début de la session.",
            );

            // ── Énergie live ─────────────────────────────────────────────────
            let live_w = vm.telemetry.live_power_w;
            ui.horizontal_wrapped(|ui| {
                Self::metric_badge(
                    ui,
                    "Source",
                    vm.telemetry.power_source.clone(),
                    egui::Color32::LIGHT_BLUE,
                );
                Self::metric_badge(
                    ui,
                    "Live",
                    fmt::watts(live_w),
                    if live_w.is_some() {
                        egui::Color32::LIGHT_BLUE
                    } else {
                        egui::Color32::DARK_GRAY
                    },
                );
                if vm.telemetry.total.dome_active_ratio > 0.0 {
                    Self::metric_badge(
                        ui,
                        "Dôme actif",
                        format!("{:.0}% du temps", vm.telemetry.total.dome_active_ratio * 100.0),
                        egui::Color32::from_rgb(96, 168, 104),
                    );
                }
            });

            // ── Puissance moyenne ─────────────────────────────────────────────
            if vm.telemetry.total.avg_power_w.is_some()
                || vm.telemetry.total.avg_power_dome_on_w.is_some()
            {
                ui.separator();
                ui.label(format!(
                    "Puiss. moy. {}  |  dôme ON {}  |  dôme OFF {}",
                    fmt::watts(vm.telemetry.total.avg_power_w),
                    fmt::watts(vm.telemetry.total.avg_power_dome_on_w),
                    fmt::watts(vm.telemetry.total.avg_power_dome_off_w)
                ));
                if let Some(saved) = vm.telemetry.total.energy_saved_kwh.filter(|&v| v > 0.0) {
                    ui.label(format!("Économie estimée dôme {saved:.3} kWh cette session"));
                }
            }

            // ── Fenêtres temporelles ──────────────────────────────────────────
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                Self::metric_badge(
                    ui,
                    "1h",
                    format!("{:.3} kWh", vm.telemetry.hour.energy_kwh),
                    egui::Color32::GRAY,
                );
                Self::metric_badge(
                    ui,
                    "24h",
                    format!("{:.3} kWh", vm.telemetry.day.energy_kwh),
                    egui::Color32::GRAY,
                );
                Self::metric_badge(
                    ui,
                    "7j",
                    format!("{:.3} kWh", vm.telemetry.week.energy_kwh),
                    egui::Color32::GRAY,
                );
                Self::metric_badge(
                    ui,
                    "30j",
                    format!("{:.3} kWh", vm.telemetry.month.energy_kwh),
                    egui::Color32::GRAY,
                );
            });

            // ── Lifetime ──────────────────────────────────────────────────────
            if vm.telemetry.lifetime.total_energy_kwh > 0.0 {
                ui.separator();
                ui.label(format!(
                    "Total vie  {:.3} kWh  ·  CO₂ {:.3} kg  ·  coût {:.2} {}",
                    vm.telemetry.lifetime.total_energy_kwh,
                    vm.telemetry.lifetime.total_co2_measured_kg,
                    vm.telemetry.lifetime.total_energy_cost_measured,
                    vm.telemetry.pricing.currency
                ));
            }
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
                "Prise intelligente (Meross)",
                "Mesure murale optionnelle — pour voir ce que la prise consomme vraiment.",
            );

            // ── État courant ──────────────────────────────────────────────────
            ui.horizontal_wrapped(|ui| {
                let bridge_color = if state.vm.external_bridge_running {
                    egui::Color32::from_rgb(96, 168, 104)
                } else {
                    egui::Color32::DARK_GRAY
                };
                Self::metric_badge(
                    ui,
                    "Bridge",
                    if state.vm.external_bridge_running { "actif".to_string() } else { "arrêté".to_string() },
                    bridge_color,
                );
                let fresh_color = if state.vm.external_status.is_fresh {
                    egui::Color32::from_rgb(96, 168, 104)
                } else {
                    egui::Color32::from_rgb(214, 153, 58)
                };
                Self::metric_badge(
                    ui,
                    "Mesure",
                    fmt::watts(state.vm.external_status.last_watts),
                    fresh_color,
                );
            });
            if !state.vm.external_bridge_detail.is_empty()
                && state.vm.external_bridge_detail != "ok"
            {
                ui.label(
                    egui::RichText::new(&state.vm.external_bridge_detail)
                        .small()
                        .color(egui::Color32::GRAY),
                );
            }

            ui.separator();
            ui.checkbox(
                &mut state.vm.external_config.enabled,
                "Activer la source externe",
            );
            ui.horizontal(|ui| {
                ui.label("Fichier données");
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
            ui.collapsing("Identifiants Meross", |ui| {
                ui.horizontal(|ui| {
                    ui.label("E-mail");
                    let email = state.vm.external_config.meross_email.get_or_insert_default();
                    ui.text_edit_singleline(email);
                });
                ui.horizontal(|ui| {
                    ui.label("Mot de passe");
                    let pwd = state.vm.external_config.meross_password.get_or_insert_default();
                    ui.add(egui::TextEdit::singleline(pwd).password(true));
                });
                ui.horizontal(|ui| {
                    ui.label("Région");
                    let region = state.vm.external_config.meross_region.get_or_insert("eu".to_string());
                    ui.text_edit_singleline(region);
                    ui.label("Modèle");
                    let device = state.vm.external_config.meross_device_type.get_or_insert("mss315".to_string());
                    ui.text_edit_singleline(device);
                });
            });
            ui.horizontal(|ui| {
                if ui.button("Enregistrer").clicked() {
                    match state.save_external_config() {
                        Ok(()) => *info = Some("Config Meross enregistrée".to_string()),
                        Err(err) => *error = Some(err),
                    }
                }
                if ui.button("Démarrer bridge").clicked() {
                    match state.start_external_bridge() {
                        Ok(()) => *info = Some("Bridge démarré".to_string()),
                        Err(err) => *error = Some(err),
                    }
                }
                if ui.button("Arrêter bridge").clicked() {
                    match state.stop_external_bridge() {
                        Ok(()) => *info = Some("Bridge arrêté".to_string()),
                        Err(err) => *error = Some(err),
                    }
                }
            });
            if !state.vm.external_status.bridge_log_path.is_empty() {
                ui.label(
                    egui::RichText::new(format!("Log: {}", state.vm.external_status.bridge_log_path))
                        .small()
                        .color(egui::Color32::DARK_GRAY),
                );
            }
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
                "Test A/B",
                "Mesurer l'effet réel du dôme : comparer OFF vs ON sur une charge concrète.",
            );
            ui.checkbox(
                &mut state.vm.benchmark_use_system_probe,
                "Utiliser la sonde système (sans commande externe)",
            );
            ui.collapsing("Commande externe", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Commande");
                    ui.add_enabled(
                        !state.vm.benchmark_use_system_probe,
                        egui::TextEdit::singleline(&mut state.vm.benchmark_command),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Arguments");
                    ui.add_enabled(
                        !state.vm.benchmark_use_system_probe,
                        egui::TextEdit::singleline(&mut state.vm.benchmark_args),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Dossier");
                    ui.add_enabled(
                        !state.vm.benchmark_use_system_probe,
                        egui::TextEdit::singleline(&mut state.vm.benchmark_cwd),
                    );
                });
            });
            ui.add(
                egui::Slider::new(&mut state.vm.benchmark_runs_per_state, 1..=8)
                    .text("Répétitions par état"),
            );
            ui.add(
                egui::Slider::new(&mut state.vm.benchmark_duration_ms, 500..=20_000)
                    .text("Durée sonde (ms)"),
            );
            ui.add(
                egui::Slider::new(&mut state.vm.benchmark_settle_ms, 250..=5_000)
                    .text("Stabilisation (ms)"),
            );
            if ui.button("Lancer le test A/B").clicked() {
                match state.run_benchmark() {
                    Ok(()) => *info = Some("Test A/B terminé".to_string()),
                    Err(err) => *error = Some(err),
                }
            }
            if let Some(session) = &state.vm.benchmark_last_session {
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    Self::metric_badge(
                        ui,
                        "Médiane",
                        session.summary.gain_median_pct
                            .map(|v| format!("{v:.1}%"))
                            .unwrap_or_else(|| "N/A".to_string()),
                        egui::Color32::from_rgb(96, 168, 104),
                    );
                    Self::metric_badge(
                        ui,
                        "p95",
                        session.summary.gain_p95_pct
                            .map(|v| format!("{v:.1}%"))
                            .unwrap_or_else(|| "N/A".to_string()),
                        egui::Color32::from_rgb(96, 168, 104),
                    );
                    Self::metric_badge(
                        ui,
                        "Efficacité",
                        session.summary.efficiency_score
                            .map(|v| format!("{v:.2}"))
                            .unwrap_or_else(|| "N/A".to_string()),
                        egui::Color32::LIGHT_BLUE,
                    );
                    Self::metric_badge(
                        ui,
                        "U/W",
                        session
                            .summary
                            .gain_utility_per_watt_pct
                            .map(|v| format!("{v:+.1}%"))
                            .unwrap_or_else(|| "N/A".to_string()),
                        if session.summary.gain_utility_per_watt_pct.unwrap_or(0.0) >= 0.0 {
                            egui::Color32::from_rgb(96, 168, 104)
                        } else {
                            egui::Color32::from_rgb(210, 84, 84)
                        },
                    );
                    Self::metric_badge(
                        ui,
                        "kWh/U",
                        session
                            .summary
                            .gain_kwh_per_utility_pct
                            .map(|v| format!("{v:+.1}%"))
                            .unwrap_or_else(|| "N/A".to_string()),
                        if session.summary.gain_kwh_per_utility_pct.unwrap_or(0.0) >= 0.0 {
                            egui::Color32::from_rgb(96, 168, 104)
                        } else {
                            egui::Color32::from_rgb(210, 84, 84)
                        },
                    );
                });
                if let (Some(off), Some(on)) = (
                    session.summary.measured_efficiency_off.as_ref(),
                    session.summary.measured_efficiency_on.as_ref(),
                ) {
                    ui.label(
                        egui::RichText::new(format!(
                            "Mesuré: OFF {:.4} U/W → ON {:.4} U/W  ·  OFF {:.6} kWh/U → ON {:.6} kWh/U",
                            off.utility_per_watt,
                            on.utility_per_watt,
                            off.kwh_per_utility,
                            on.kwh_per_utility
                        ))
                        .small()
                        .color(egui::Color32::GRAY),
                    );
                }
            }
            if let Some(history) = &state.vm.benchmark_history {
                if history.sessions.len() > 1 {
                    ui.label(format!("{} sessions enregistrées", history.sessions.len()));
                }
                if let Some(advice) = &history.advice {
                    ui.separator();
                    ui.label(egui::RichText::new("Réglages conseillés").strong());
                    ui.label(format!(
                        "κ {:.1}  ·  Σmax {:.2}  ·  η {:.2}  ·  policy {}",
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
                ui.horizontal_wrapped(|ui| {
                    // Nom — gras si non vide
                    if !item.name.is_empty() {
                        ui.strong(&item.name);
                    }
                    ui.label(format!("[{}]", item.kind));

                    // Watts : seulement pour les endpoints ou si mesure réelle disponible
                    if endpoint_budget_w.is_some() {
                        let estimated_w = endpoint_budget_w
                            .map(|budget| budget * (endpoint_weight(&item.kind) / total_weight));
                        ui.label(format!("~{}", crate::fmt::watts(estimated_w)));
                    }

                    // Scope de mesure : seulement si ce n'est pas "detected" banal
                    if let Some(scope) = &item.measurement_scope {
                        if scope != "detected" {
                            ui.label(egui::RichText::new(format!("[{scope}]")).small());
                        }
                    }

                    // État
                    if let Some(active_state) = &item.active_state {
                        let (label, color) = match active_state.as_str() {
                            "active" => ("actif", egui::Color32::from_rgb(96, 168, 104)),
                            "connected" => ("connecté", egui::Color32::from_rgb(100, 149, 237)),
                            "idle" => ("veille", egui::Color32::GRAY),
                            other => (other, egui::Color32::GRAY),
                        };
                        ui.colored_label(color, label);
                    }

                    // Lien physique
                    if let Some(link) = &item.physical_link_hint {
                        ui.label(egui::RichText::new(link).small().color(egui::Color32::DARK_GRAY));
                    }

                    // Fiabilité : seulement si mesurée ou dérivée (pas "detected" à 65%)
                    if let Some(score) = item.confidence_score {
                        if score < 0.64 || score > 0.66 {
                            // Uniquement si ≠ 65% (valeur par défaut sans intérêt)
                            let color = if score >= 0.85 {
                                egui::Color32::from_rgb(96, 168, 104)
                            } else if score >= 0.6 {
                                egui::Color32::from_rgb(214, 153, 58)
                            } else {
                                egui::Color32::from_rgb(210, 84, 84)
                            };
                            ui.colored_label(color, format!("{:.0}%", score * 100.0));
                        }
                    }

                    // Détail
                    if let Some(detail) = &item.detail {
                        ui.label(egui::RichText::new(detail).small());
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
        let power = vm
            .metrics
            .raw
            .host_power_watts
            .or(vm.metrics.raw.wall_power_watts)
            .or_else(|| {
                if vm.external_status.is_fresh {
                    vm.external_status.last_watts
                } else {
                    None
                }
            });
        egui::Window::new("SoulKernel HUD")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 16.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    Self::status_chip(ui, "Dome", vm.dome_active);
                    Self::status_chip(ui, "SoulRAM", vm.soulram_active);
                    ui.strong(fmt::watts(power));
                });
                ui.label(format!(
                    "CPU {}  RAM {}",
                    fmt::pct(vm.metrics.raw.cpu_pct),
                    fmt::gib_pair(vm.metrics.raw.mem_used_mb, vm.metrics.raw.mem_total_mb)
                ));
                {
                    use soulkernel_core::kpi::KpiLabel;
                    let kpi_str = vm.kpi.kpi_penalized
                        .map(|k| format!("KPI {:.1} W/%  [{}]", k, vm.kpi.label.as_str()))
                        .unwrap_or_else(|| "KPI —".to_string());
                    let kpi_color = match vm.kpi.label {
                        KpiLabel::Efficient   => egui::Color32::from_rgb(96, 168, 104),
                        KpiLabel::Moderate    => egui::Color32::from_rgb(214, 153, 58),
                        KpiLabel::Inefficient => egui::Color32::from_rgb(210, 84, 84),
                        KpiLabel::Unknown     => egui::Color32::DARK_GRAY,
                    };
                    ui.colored_label(kpi_color, kpi_str);
                }
                if vm.metrics.raw.gpu_pct.map_or(false, |g| g > 0.5) {
                    ui.label(format!(
                        "GPU {}  I/O R{:.1}/W{:.1} MB/s",
                        fmt::opt_pct(vm.metrics.raw.gpu_pct),
                        vm.metrics.raw.io_read_mb_s.unwrap_or(0.0),
                        vm.metrics.raw.io_write_mb_s.unwrap_or(0.0)
                    ));
                }
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
                "Commandes",
                "Choisir une cible, activer le dôme, libérer la mémoire.",
            );

            // ── État courant ──────────────────────────────────────────────────
            Self::action_summary(ui, &state.vm);
            ui.separator();

            // ── Cible ─────────────────────────────────────────────────────────
            ui.checkbox(&mut state.vm.auto_target, "Cible automatique");
            if !state.vm.auto_target {
                // Selected text: show group name if multi-instance, else PID.
                let selected_label = state.vm.manual_target_pid.map(|pid| {
                    // Check if this PID belongs to a multi-instance group.
                    let name = state
                        .vm
                        .process_report
                        .top_processes
                        .iter()
                        .find(|p| p.pid == pid)
                        .map(|p| p.name.as_str())
                        .unwrap_or("");
                    let count = state
                        .vm
                        .process_report
                        .groups
                        .iter()
                        .find(|g| g.top_pid == pid)
                        .map(|g| g.instance_count)
                        .unwrap_or(1);
                    if count > 1 {
                        format!("{name} ×{count}")
                    } else {
                        format!("PID {pid}")
                    }
                }).unwrap_or_else(|| "aucune".to_string());

                egui::ComboBox::from_label("Cible manuelle")
                    .selected_text(selected_label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.vm.manual_target_pid, None, "aucune");
                        // Show groups with multiple instances first (top targets).
                        let multi_groups: Vec<_> = state
                            .vm
                            .process_report
                            .groups
                            .iter()
                            .filter(|g| g.instance_count > 1 && g.total_cpu_pct >= 0.1)
                            .collect();
                        if !multi_groups.is_empty() {
                            ui.separator();
                            ui.label(egui::RichText::new("— Applications (plusieurs instances) —").small().color(egui::Color32::GRAY));
                            for g in multi_groups {
                                ui.selectable_value(
                                    &mut state.vm.manual_target_pid,
                                    Some(g.top_pid),
                                    format!("{} ×{}  {:.1}%  {}", g.name, g.instance_count, g.total_cpu_pct, fmt::mib_from_kb(g.total_memory_kb)),
                                );
                            }
                            ui.separator();
                            ui.label(egui::RichText::new("— Processus individuels —").small().color(egui::Color32::GRAY));
                        }
                        for proc_ in &state.vm.process_report.top_processes {
                            if proc_.is_self_process || proc_.is_embedded_webview {
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
            }
            Self::target_summary(ui, &state.vm);
            egui::ComboBox::from_label("Charge de travail")
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

            ui.separator();

            // ── Dôme ─────────────────────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Dôme").strong());
                // État visible en permanence, pas seulement en mode auto.
                Self::status_chip(ui, if state.vm.dome_active { "ACTIF" } else { "inactif" }, state.vm.dome_active);
            });
            ui.horizontal(|ui| {
                // "Activer" grisé si déjà actif, "Annuler" grisé si inactif.
                if ui.add_enabled(!state.vm.dome_active, egui::Button::new("⚡ Activer")).clicked() {
                    match state.activate_dome() {
                        Ok(()) => *info = Some("Dôme activé".to_string()),
                        Err(err) => *error = Some(err),
                    }
                }
                if ui.add_enabled(state.vm.dome_active, egui::Button::new("↩ Annuler")).clicked() {
                    match state.rollback_dome() {
                        Ok(()) => *info = Some("Dôme annulé".to_string()),
                        Err(err) => *error = Some(err),
                    }
                }
                ui.separator();
                ui.checkbox(&mut state.vm.auto_dome, "Auto (KPI)");
            });
            if state.vm.auto_dome {
                let (auto_label, auto_color) = if state.vm.kpi.self_overload {
                    (
                        format!("⚠ suspendu — SoulKernel {:.0}% CPU", state.vm.kpi.cpu_self_pct),
                        egui::Color32::from_rgb(210, 84, 84),
                    )
                } else if state.vm.dome_active {
                    (
                        format!(
                            "actif — KPI {:.1} W/% [{}]",
                            state.vm.kpi.kpi_penalized.unwrap_or(0.0),
                            state.vm.kpi.label.as_str()
                        ),
                        egui::Color32::from_rgb(96, 168, 104),
                    )
                } else {
                    match state.vm.auto_dome_next_eval_s {
                        Some(s) if s > 0 => (
                            format!("cooldown {}s — KPI {}", s, state.vm.kpi.label.as_str()),
                            egui::Color32::GRAY,
                        ),
                        _ => (
                            format!("prêt — KPI {} / garde {:.0}%",
                                state.vm.kpi.label.as_str(),
                                state.vm.formula.advanced_guard * 100.0
                            ),
                            if state.vm.kpi.should_act_with_profile(&state.vm.device_profile)
                                && state.vm.formula.advanced_guard
                                    >= state.vm.device_profile.auto_dome_guard_min
                            {
                                egui::Color32::from_rgb(214, 153, 58) // va activer
                            } else {
                                egui::Color32::GRAY
                            },
                        ),
                    }
                };
                ui.label(egui::RichText::new(format!("↳ {auto_label}")).small().color(auto_color));
            }

            ui.separator();

            // ── SoulRAM ───────────────────────────────────────────────────────
            ui.label(egui::RichText::new("SoulRAM").strong());
            ui.horizontal(|ui| {
                if ui.button("🧠 Activer").clicked() {
                    if let Err(err) = state.enable_soulram() {
                        *error = Some(err);
                    }
                }
                if ui.button("🧠 Désactiver").clicked() {
                    if let Err(err) = state.disable_soulram() {
                        *error = Some(err);
                    }
                }
            });
            // Auto-cycle : re-applique SoulRAM dès que le cooldown est écoulé et sigma > 0.3.
            ui.checkbox(
                &mut state.vm.auto_cycle_soulram,
                "Auto-cycle (re-application automatique)",
            );
            if state.vm.auto_cycle_soulram {
                let mode_hint = if soulkernel_core::workload_catalog::is_burst(
                    &state.vm.selected_workload,
                ) {
                    "Burst : cycle toutes les ~3 min"
                } else {
                    "Sustain : cycle toutes les ~15 min"
                };
                ui.label(
                    egui::RichText::new(mode_hint)
                        .small()
                        .color(egui::Color32::GRAY),
                );
            }
            ui.horizontal(|ui| {
                if ui.button("Actualiser").clicked() {
                    if let Err(err) = state.refresh_now() {
                        *error = Some(err);
                    }
                }
                if ui.button("Exporter JSON").clicked() {
                    match export::export_snapshot(&state.vm) {
                        Ok(path) => *info = Some(format!("Export: {path}")),
                        Err(err) => *error = Some(err),
                    }
                }
            });

            // ── Réglages avancés ──────────────────────────────────────────────
            ui.collapsing("Réglages avancés", |ui| {
                // Profil appareil
                egui::ComboBox::from_label("Profil appareil")
                    .selected_text(state.vm.device_profile.label)
                    .show_ui(ui, |ui| {
                        for p in soulkernel_core::device_profile::DeviceProfile::list_all() {
                            let label = p.label;
                            let id = p.id;
                            let selected = state.vm.device_profile.id == id;
                            if ui.selectable_label(selected, format!(
                                "{} — {}",
                                label,
                                if p.can_act { "actions activées" } else { "monitoring seul" }
                            )).clicked() {
                                state.vm.device_profile = p;
                                state.vm.kpi_lambda = state.vm.device_profile.kpi_lambda_default;
                            }
                        }
                    });
                if !state.vm.device_profile.can_act {
                    ui.colored_label(
                        egui::Color32::from_rgb(214, 153, 58),
                        "Monitoring seul — dôme et SoulRAM désactivés.",
                    );
                }
                ui.separator();
                egui::ComboBox::from_label("Politique")
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
                ui.add(egui::Slider::new(&mut state.vm.kappa, 0.5..=5.0).text("Agressivité (κ)"));
                ui.add(egui::Slider::new(&mut state.vm.sigma_max, 0.3..=0.95).text("Seuil (Σmax)"));
                ui.add(egui::Slider::new(&mut state.vm.eta, 0.01..=0.5).text("Lissage (η)"));
                ui.add(egui::Slider::new(&mut state.vm.soulram_percent, 5..=60).text("SoulRAM %"));
                ui.add(
                    egui::Slider::new(&mut state.vm.kpi_lambda, 0.0..=2.0)
                        .step_by(0.05)
                        .text("KPI λ (pénalité faults)"),
                );
                Self::tuning_summary(ui, &state.vm);
            });

            // ── Info fichiers ─────────────────────────────────────────────────
            ui.separator();
            ui.label(
                egui::RichText::new(format!("Audit  {}", state.vm.audit_path))
                    .small()
                    .color(egui::Color32::DARK_GRAY),
            );
            ui.label(
                egui::RichText::new(format!("Observabilité  {}", state.vm.observability_path))
                    .small()
                    .color(egui::Color32::DARK_GRAY),
            );
            ui.label(
                egui::RichText::new(format!(
                    "Journal time-series auto  fichier courant .jsonl + archives .jsonl.gz  rotation à partir de {:.0} MiB  archives conservées: 8",
                    crate::export::observability_rotation_bytes() as f64 / (1024.0 * 1024.0)
                ))
                .small()
                .color(egui::Color32::GRAY),
            );
            if cfg!(target_os = "windows") {
                ui.label(
                    egui::RichText::new(
                        "Windows  AppData/Roaming/SoulKernel/telemetry/observability_samples.jsonl",
                    )
                    .small()
                    .color(egui::Color32::GRAY),
                );
            } else {
                ui.label(
                    egui::RichText::new(
                        "Linux/macOS  XDG_DATA_HOME ou ~/.local/share/SoulKernel/telemetry/observability_samples.jsonl",
                    )
                    .small()
                    .color(egui::Color32::GRAY),
                );
            }
        });
    }

    fn processes_panel(ui: &mut egui::Ui, state: &LiteState) {
        let summary = &state.vm.process_report.summary;
        ui.group(|ui| {
            Self::section_title(
                ui,
                "Processus observés",
                "Actifs en ce moment — groupés par application, puis détail.",
            );

            // ── Alertes ────────────────────────────────────────────────────────
            if summary.bridge_python_count > 1 {
                ui.colored_label(
                    egui::Color32::from_rgb(214, 153, 58),
                    format!(
                        "⚠ {} processus Python bridge actifs — accumulation probable. Arrêter et redémarrer le bridge.",
                        summary.bridge_python_count
                    ),
                );
            }
            if summary.memory_compression_active {
                ui.label(
                    egui::RichText::new("Memory Compression actif (SoulRAM opérationnel)")
                        .small()
                        .color(egui::Color32::from_rgb(96, 168, 104)),
                );
            }

            // ── En-tête ────────────────────────────────────────────────────────
            ui.label(
                egui::RichText::new(format!(
                    "{} processus  ·  SoulKernel {:.1}% / {}  ·  UI {:.1}% / {}",
                    summary.process_count,
                    summary.self_cpu_usage_pct,
                    fmt::mib_from_kb(summary.self_memory_kb),
                    summary.webview_cpu_usage_pct,
                    fmt::mib_from_kb(summary.webview_memory_kb),
                ))
                .small()
                .color(egui::Color32::GRAY),
            );
            ui.separator();

            // ── Vue groupée — applications avec plusieurs instances ou CPU notable ──
            let notable_groups: Vec<_> = state
                .vm
                .process_report
                .groups
                .iter()
                .filter(|g| g.instance_count > 1 || g.total_cpu_pct >= 0.5)
                .take(10)
                .collect();
            if !notable_groups.is_empty() {
                for g in &notable_groups {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(&g.name).strong());
                        if g.instance_count > 1 {
                            ui.colored_label(
                                egui::Color32::from_rgb(214, 153, 58),
                                format!("×{}", g.instance_count),
                            );
                        }
                        ui.label(format!("{:.1}%", g.total_cpu_pct));
                        ui.label(fmt::mib_from_kb(g.total_memory_kb));
                    });
                }
                ui.separator();
            }

            // ── Détail individuel (top processus actifs) ──────────────────────
            egui::ScrollArea::vertical()
                .id_salt("processes_scroll")
                .max_height(200.0)
                .show(ui, |ui| {
                    for proc_ in &state.vm.process_report.top_processes {
                        ui.horizontal_wrapped(|ui| {
                            use soulkernel_core::kpi::{classify_by_name, ProcessClass};
                            let class = if proc_.is_self_process || proc_.is_embedded_webview {
                                None
                            } else {
                                classify_by_name(&state.vm.device_profile, &proc_.name)
                            };
                            let name_color = match class {
                                Some(ProcessClass::SystemKernel) => egui::Color32::DARK_GRAY,
                                Some(ProcessClass::OverheadCritical) => egui::Color32::from_rgb(214, 153, 58),
                                Some(ProcessClass::OverheadSoft) => egui::Color32::from_rgb(170, 130, 60),
                                _ if proc_.is_self_process || proc_.is_embedded_webview => egui::Color32::DARK_GRAY,
                                _ => egui::Color32::WHITE,
                            };
                            ui.label(egui::RichText::new(&proc_.name).strong().color(name_color));
                            ui.label(
                                egui::RichText::new(format!("#{}", proc_.pid))
                                    .small()
                                    .color(egui::Color32::DARK_GRAY),
                            );
                            ui.label(fmt::pct(proc_.cpu_usage_pct));
                            ui.label(fmt::mib_from_kb(proc_.memory_kb));
                            if proc_.disk_read_bytes > 0 || proc_.disk_written_bytes > 0 {
                                ui.label(fmt::io_pair(proc_.disk_read_bytes, proc_.disk_written_bytes));
                            }
                            ui.label(
                                egui::RichText::new(fmt::runtime_short(proc_.run_time_s))
                                    .small()
                                    .color(egui::Color32::DARK_GRAY),
                            );
                            // Tag de classification KPI
                            match class {
                                Some(ProcessClass::OverheadCritical) => {
                                    ui.label(egui::RichText::new("overhead-sec").small().color(egui::Color32::from_rgb(214, 153, 58)));
                                }
                                Some(ProcessClass::OverheadSoft) => {
                                    ui.label(egui::RichText::new("overhead").small().color(egui::Color32::from_rgb(170, 130, 60)));
                                }
                                Some(ProcessClass::SystemKernel) => {
                                    ui.label(egui::RichText::new("sys").small().color(egui::Color32::DARK_GRAY));
                                }
                                _ if proc_.is_self_process => {
                                    ui.label(egui::RichText::new("SoulKernel").small().color(egui::Color32::DARK_GRAY));
                                }
                                _ if proc_.is_embedded_webview => {
                                    ui.label(egui::RichText::new("UI").small().color(egui::Color32::DARK_GRAY));
                                }
                                _ => {}
                            }
                        });
                    }
                });
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
            500
        } else {
            2000
        };
        ctx.request_repaint_after(std::time::Duration::from_millis(repaint_ms));

        if let Err(err) = state.refresh_if_needed() {
            self.error = Some(err);
        }

        // Nettoie les messages d'info périmés quand l'état réel du dôme a changé.
        if matches!(self.info.as_deref(), Some("Dôme activé")) && !state.vm.dome_active {
            self.info = None;
        }
        if matches!(self.info.as_deref(), Some("Dôme annulé")) && state.vm.dome_active {
            self.info = None;
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
                            // Colonne gauche : observer → comprendre → mesurer
                            Self::host_impact_panel(&mut columns[0], &state.vm);
                            columns[0].add_space(8.0);
                            Self::material_overview_panel(&mut columns[0], &state.vm);
                            columns[0].add_space(8.0);
                            Self::processes_panel(&mut columns[0], state);
                            columns[0].add_space(8.0);
                            Self::decision_panel(&mut columns[0], &state.vm);
                            columns[0].add_space(8.0);
                            Self::telemetry_panel(&mut columns[0], &state.vm);

                            // Colonne droite : gains → agir → configurer → inventaire
                            Self::gains_panel(&mut columns[1], &state.vm);
                            columns[1].add_space(8.0);
                            Self::pilotage_panel(
                                &mut columns[1],
                                state,
                                &mut self.error,
                                &mut self.info,
                            );
                            columns[1].add_space(8.0);
                            Self::external_power_panel(
                                &mut columns[1],
                                state,
                                &mut self.error,
                                &mut self.info,
                            );
                            columns[1].add_space(8.0);
                            Self::inventory_panel(&mut columns[1], state);
                            columns[1].add_space(8.0);
                            Self::benchmark_panel(
                                &mut columns[1],
                                state,
                                &mut self.error,
                                &mut self.info,
                            );
                            columns[1].add_space(8.0);
                            Self::hud_panel(&mut columns[1], state);
                        });
                    });
                }); // ScrollArea
        });

        if state.vm.show_hud {
            Self::hud_overlay(ctx, &state.vm);
        }
    }
}
