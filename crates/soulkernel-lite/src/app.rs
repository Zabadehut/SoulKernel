use crate::export;
use crate::fmt;
use crate::state::{HostImpactDelta, LiteState, LiteViewModel};
use eframe::egui;
use egui::{Color32, RichText, Stroke, Vec2};

// ── Design tokens ──────────────────────────────────────────────────────────────

const C_BG: Color32 = Color32::from_rgb(6, 16, 25);
const C_PANEL: Color32 = Color32::from_rgb(12, 24, 40);
const C_PANEL2: Color32 = Color32::from_rgb(17, 32, 52);
const C_PANEL3: Color32 = Color32::from_rgb(22, 40, 64);
const C_BORDER: Color32 = Color32::from_rgb(30, 52, 78);
const C_TEXT: Color32 = Color32::from_rgb(210, 228, 248);
const C_MUTED: Color32 = Color32::from_rgb(110, 152, 196);
const C_GREEN: Color32 = Color32::from_rgb(72, 210, 120);
const C_YELLOW: Color32 = Color32::from_rgb(250, 198, 60);
const C_RED: Color32 = Color32::from_rgb(239, 83, 104);
const C_CYAN: Color32 = Color32::from_rgb(34, 211, 238);

// ── App ────────────────────────────────────────────────────────────────────────

pub struct LiteApp {
    state: Option<LiteState>,
    error: Option<String>,
    info: Option<String>,
    active_tab: DashboardTab,
    visuals_configured: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DashboardTab {
    Home,
    Actions,
    Processes,
    Energy,
    Hardware,
}

impl Default for LiteApp {
    fn default() -> Self {
        match LiteState::new() {
            Ok(state) => Self {
                state: Some(state),
                error: None,
                info: None,
                active_tab: DashboardTab::Home,
                visuals_configured: false,
            },
            Err(err) => Self {
                state: None,
                error: Some(err),
                info: None,
                active_tab: DashboardTab::Home,
                visuals_configured: false,
            },
        }
    }
}

// ── Visual primitives ──────────────────────────────────────────────────────────

impl LiteApp {
    fn configure_visuals(ctx: &egui::Context) {
        let mut v = egui::Visuals::dark();
        v.override_text_color = Some(C_TEXT);
        v.panel_fill = C_BG;
        v.window_fill = C_PANEL;
        v.window_stroke = Stroke::new(1.0, C_BORDER);
        // Separator / non-interactive backgrounds
        v.widgets.noninteractive.bg_fill = C_PANEL2;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, C_BORDER);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, C_MUTED);
        // Inactive buttons / inputs
        v.widgets.inactive.bg_fill = C_PANEL2;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, C_BORDER);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, C_TEXT);
        // Hover
        v.widgets.hovered.bg_fill = C_PANEL3;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, C_CYAN);
        v.widgets.hovered.fg_stroke = Stroke::new(1.5, C_TEXT);
        // Active / pressed
        v.widgets.active.bg_fill = Color32::from_rgb(25, 52, 84);
        v.widgets.active.bg_stroke = Stroke::new(1.5, C_CYAN);
        v.widgets.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
        // Selection (text inputs, etc.)
        v.selection.bg_fill = Color32::from_rgba_unmultiplied(34, 211, 238, 60);
        v.selection.stroke = Stroke::new(1.0, C_CYAN);
        ctx.set_visuals(v);
    }

    // ── Card frame ──────────────────────────────────────────────────────────────

    fn panel_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
        egui::Frame::new()
            .fill(C_PANEL)
            .stroke(Stroke::new(1.0, C_BORDER))
            .corner_radius(16.0)
            .inner_margin(egui::Margin::same(16))
            .show(ui, add_contents);
    }

    fn section_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
        egui::Frame::new()
            .fill(C_PANEL2)
            .stroke(Stroke::new(1.0, C_BORDER))
            .corner_radius(10.0)
            .inner_margin(egui::Margin::same(12))
            .show(ui, add_contents);
    }

    // ── Section header ──────────────────────────────────────────────────────────

    fn section_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
        ui.label(RichText::new(title).size(15.0).strong().color(C_TEXT));
        if !subtitle.is_empty() {
            ui.label(RichText::new(subtitle).size(11.0).color(C_MUTED));
        }
        ui.add_space(8.0);
    }

    fn eyebrow(ui: &mut egui::Ui, label: &str) {
        ui.label(
            RichText::new(label.to_uppercase())
                .size(9.5)
                .color(C_CYAN)
                .strong(),
        );
    }

    // ── Metric card ────────────────────────────────────────────────────────────

    /// Full metric card: label (top, small) + big value + accent bar at bottom.
    fn metric_badge(ui: &mut egui::Ui, title: &str, value: String, tone: Color32) {
        let width = if value.len() > 28 { 220.0 } else { 132.0 };
        let height = 62.0;
        ui.allocate_ui_with_layout(
            Vec2::new(width, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_min_size(Vec2::new(width, height));
                ui.set_max_size(Vec2::new(width, height));
                egui::Frame::new()
                    .fill(C_PANEL2)
                    .stroke(Stroke::new(1.0, C_BORDER))
                    .corner_radius(12.0)
                    .inner_margin(egui::Margin::ZERO)
                    .show(ui, |ui| {
                        ui.set_min_size(Vec2::new(width, height));
                        ui.set_max_size(Vec2::new(width, height));
                        egui::Frame::new()
                            .inner_margin(egui::Margin {
                                left: 12,
                                right: 12,
                                top: 10,
                                bottom: 6,
                            })
                            .show(ui, |ui| {
                                ui.set_max_width(width - 24.0);
                                ui.set_max_height(height - 13.0);
                                ui.label(RichText::new(title).size(9.5).color(C_MUTED).strong());
                                ui.label(RichText::new(&value).size(15.0).strong().color(tone));
                            });
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(width, 3.0), egui::Sense::hover());
                        ui.painter()
                            .rect_filled(rect, 0.0, tone.gamma_multiply(0.7));
                    });
            },
        );
    }

    /// Compact inline badge: just text on a colored pill background.
    fn status_chip(ui: &mut egui::Ui, label: &str, active: bool) {
        let (fill, dot_color, text_color) = if active {
            (
                Color32::from_rgba_unmultiplied(72, 210, 120, 30),
                C_GREEN,
                C_GREEN,
            )
        } else {
            (C_PANEL3, C_BORDER, C_MUTED)
        };
        egui::Frame::new()
            .fill(fill)
            .stroke(Stroke::new(
                1.0,
                if active {
                    C_GREEN.gamma_multiply(0.5)
                } else {
                    C_BORDER
                },
            ))
            .corner_radius(999.0)
            .inner_margin(egui::Margin {
                left: 10,
                right: 10,
                top: 4,
                bottom: 4,
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    // Dot indicator
                    let (dot_rect, _) =
                        ui.allocate_exact_size(Vec2::splat(7.0), egui::Sense::hover());
                    ui.painter()
                        .circle_filled(dot_rect.center(), 3.5, dot_color);
                    ui.label(RichText::new(label).size(11.5).strong().color(text_color));
                });
            });
    }

    // ── Tab navigation ─────────────────────────────────────────────────────────

    fn tab_button(ui: &mut egui::Ui, active: bool, label: &str) -> egui::Response {
        let (fill, text_color, stroke) = if active {
            (
                Color32::from_rgba_unmultiplied(34, 211, 238, 28),
                C_CYAN,
                Stroke::new(1.0, C_CYAN.gamma_multiply(0.6)),
            )
        } else {
            (Color32::TRANSPARENT, C_MUTED, Stroke::new(1.0, C_BORDER))
        };
        ui.add(
            egui::Button::new(RichText::new(label).size(12.0).strong().color(text_color))
                .fill(fill)
                .stroke(stroke)
                .corner_radius(999.0),
        )
    }

    fn dashboard_tabs(ui: &mut egui::Ui, active_tab: &mut DashboardTab) {
        egui::Frame::new()
            .fill(C_PANEL2)
            .stroke(Stroke::new(1.0, C_BORDER))
            .corner_radius(999.0)
            .inner_margin(egui::Margin::symmetric(4, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (tab, label) in [
                        (DashboardTab::Home, "Accueil"),
                        (DashboardTab::Actions, "Actions"),
                        (DashboardTab::Processes, "Processus"),
                        (DashboardTab::Energy, "Énergie"),
                        (DashboardTab::Hardware, "Matériel"),
                    ] {
                        if Self::tab_button(ui, *active_tab == tab, label).clicked() {
                            *active_tab = tab;
                        }
                    }
                });
            });
    }

    // ── Color helpers ──────────────────────────────────────────────────────────

    fn tone_for_ratio(value: f64) -> Color32 {
        if value >= 0.85 {
            C_RED
        } else if value >= 0.60 {
            C_YELLOW
        } else {
            C_GREEN
        }
    }

    fn kpi_color(label: &soulkernel_core::kpi::KpiLabel) -> Color32 {
        use soulkernel_core::kpi::KpiLabel;
        match label {
            KpiLabel::Efficient => C_GREEN,
            KpiLabel::Moderate => C_YELLOW,
            KpiLabel::Inefficient => C_RED,
            KpiLabel::Unknown => C_MUTED,
        }
    }

    // ── Progress bar ──────────────────────────────────────────────────────────

    fn progress_bar(ui: &mut egui::Ui, value: f32, color: Color32) {
        let desired = Vec2::new(ui.available_width(), 4.0);
        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, C_PANEL3);
        if value > 0.0 {
            let filled = egui::Rect::from_min_size(
                rect.min,
                Vec2::new(rect.width() * value.clamp(0.0, 1.0), rect.height()),
            );
            ui.painter().rect_filled(filled, 2.0, color);
        }
    }

    // ── Key-value row ─────────────────────────────────────────────────────────

    fn kv_row(ui: &mut egui::Ui, label: &str, value: &str) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(11.0).color(C_MUTED));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(value).size(11.0).color(C_TEXT).strong());
            });
        });
    }

    fn kv_row_colored(ui: &mut egui::Ui, label: &str, value: &str, color: Color32) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(11.0).color(C_MUTED));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(value).size(11.0).strong().color(color));
            });
        });
    }

    // ── Power unavailable hint ─────────────────────────────────────────────────

    fn power_unavailable_hint(os: &str) -> &'static str {
        if os.contains("Windows") {
            "desktop Windows: branchez un Meross"
        } else if os.contains("macOS") || os.contains("Darwin") {
            "Mac desktop: branchez un Meross"
        } else {
            "RAPL non disponible"
        }
    }

    // ── Top bar ────────────────────────────────────────────────────────────────

    fn top_bar(ui: &mut egui::Ui, vm: &LiteViewModel) {
        ui.horizontal(|ui| {
            // Logo / title
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: 0,
                    right: 16,
                    top: 0,
                    bottom: 0,
                })
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("SoulKernel")
                            .size(17.0)
                            .strong()
                            .color(C_CYAN),
                    );
                    ui.label(RichText::new("Lite").size(12.0).color(C_MUTED));
                });

            ui.separator();

            // OS + Workload
            ui.label(
                RichText::new(format!(
                    "{}  ·  {}",
                    vm.platform_info.os, vm.selected_workload
                ))
                .size(11.0)
                .color(C_MUTED),
            );

            ui.separator();

            // Status chips — Dome + SoulRAM
            Self::status_chip(ui, "Dôme", vm.dome_active);
            ui.add_space(4.0);
            Self::status_chip(ui, "SoulRAM", vm.soulram_active);

            ui.separator();

            // Power
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
            let (power_label, power_color) = match power {
                Some(w) => (format!("{:.1} W", w), C_CYAN),
                None => ("— W".to_string(), C_MUTED),
            };
            ui.label(RichText::new("⚡").size(12.0).color(C_YELLOW));
            ui.label(
                RichText::new(&power_label)
                    .size(13.0)
                    .strong()
                    .color(power_color),
            );

            ui.separator();

            // CPU
            let cpu_color = Self::tone_for_ratio(vm.metrics.cpu);
            ui.label(RichText::new("CPU").size(10.0).color(C_MUTED));
            ui.label(
                RichText::new(fmt::pct(vm.metrics.raw.cpu_pct))
                    .size(13.0)
                    .strong()
                    .color(cpu_color),
            );

            ui.separator();

            // RAM
            let mem_ratio = if vm.metrics.raw.mem_total_mb > 0 {
                vm.metrics.raw.mem_used_mb as f64 / vm.metrics.raw.mem_total_mb as f64
            } else {
                0.0
            };
            let ram_color = Self::tone_for_ratio(mem_ratio);
            ui.label(RichText::new("RAM").size(10.0).color(C_MUTED));
            ui.label(
                RichText::new(format!("{:.0}%", mem_ratio * 100.0))
                    .size(13.0)
                    .strong()
                    .color(ram_color),
            );

            // KPI pill on the right
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let kpi_color = Self::kpi_color(&vm.kpi.label);
                egui::Frame::new()
                    .fill(kpi_color.gamma_multiply(0.15))
                    .stroke(Stroke::new(1.0, kpi_color.gamma_multiply(0.5)))
                    .corner_radius(999.0)
                    .inner_margin(egui::Margin::symmetric(10, 3))
                    .show(ui, |ui| {
                        let kpi_str = vm
                            .kpi
                            .kpi_penalized
                            .map(|k| format!("{:.1} W/%  [{}]", k, vm.kpi.label.as_str()))
                            .unwrap_or_else(|| "KPI —".to_string());
                        ui.label(RichText::new(&kpi_str).size(11.0).strong().color(kpi_color));
                    });
            });
        });
    }

    // ── Metrics strip ─────────────────────────────────────────────────────────

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

        let (tension_label, tension_color) = if vm.metrics.sigma >= vm.sigma_max {
            ("Tension haute", C_RED)
        } else if vm.metrics.sigma >= vm.sigma_max * 0.75 {
            ("Tension mod.", C_YELLOW)
        } else {
            ("Tension basse", C_GREEN)
        };

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);

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
                    format!(
                        "R {:.1} / W {:.1} MB/s",
                        vm.metrics.raw.io_read_mb_s.unwrap_or(0.0),
                        vm.metrics.raw.io_write_mb_s.unwrap_or(0.0)
                    ),
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
                    C_CYAN
                } else {
                    C_MUTED
                },
            );
            Self::metric_badge(ui, "Statut", tension_label.to_string(), tension_color);
            {
                let kpi_color = Self::kpi_color(&vm.kpi.label);
                let kpi_val = vm
                    .kpi
                    .kpi_penalized
                    .map(|k| format!("{:.1} W/%", k))
                    .unwrap_or_else(|| "KPI N/A".to_string());
                Self::metric_badge(ui, "KPI", kpi_val, kpi_color);
            }
        });
    }

    // ── Material overview panel ────────────────────────────────────────────────

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

        fn row(ui: &mut egui::Ui, label: &str, value: String) {
            if value != "N/A" && !value.is_empty() {
                LiteApp::kv_row(ui, label, &value);
            }
        }

        let has_battery = vm.metrics.raw.on_battery.is_some()
            && !(vm.metrics.raw.on_battery == Some(false)
                && vm.metrics.raw.battery_percent.unwrap_or(0.0) <= 0.0);

        Self::panel_card(ui, |ui| {
            Self::section_title(
                ui,
                "Matériel interne / externe",
                "Ce que l'hôte voit · ce que la prise voit · l'écart.",
            );
            Self::priority_hint(ui, vm);
            ui.add_space(8.0);
            ui.columns(3, |columns| {
                // ── Interne ──────────────────────────────────────────────────
                Self::eyebrow(&mut columns[0], "Interne");
                columns[0].add_space(4.0);
                row(
                    &mut columns[0],
                    "CPU",
                    format!(
                        "{} · {} MHz",
                        fmt::pct(vm.metrics.raw.cpu_pct),
                        vm.metrics.raw.cpu_clock_mhz.unwrap_or(0.0).round()
                    ),
                );
                {
                    let cpu_r = (vm.metrics.raw.cpu_pct / 100.0).clamp(0.0, 1.0) as f32;
                    Self::progress_bar(
                        &mut columns[0],
                        cpu_r,
                        Self::tone_for_ratio(vm.metrics.cpu),
                    );
                    columns[0].add_space(2.0);
                }
                row(
                    &mut columns[0],
                    "RAM",
                    format!(
                        "{} ({mem_ratio:.0}%)",
                        fmt::gib_pair(vm.metrics.raw.mem_used_mb, vm.metrics.raw.mem_total_mb)
                    ),
                );
                {
                    Self::progress_bar(
                        &mut columns[0],
                        (mem_ratio / 100.0) as f32,
                        Self::tone_for_ratio(mem_ratio / 100.0),
                    );
                    columns[0].add_space(2.0);
                }
                row(
                    &mut columns[0],
                    "I/O",
                    format!(
                        "R {:.2} / W {:.2} MB/s",
                        vm.metrics.raw.io_read_mb_s.unwrap_or(0.0),
                        vm.metrics.raw.io_write_mb_s.unwrap_or(0.0)
                    ),
                );
                if vm.metrics.raw.gpu_pct.is_some() || vm.metrics.raw.gpu_power_watts.is_some() {
                    row(
                        &mut columns[0],
                        "GPU",
                        format!(
                            "{} · {}",
                            fmt::opt_pct(vm.metrics.raw.gpu_pct),
                            fmt::watts(vm.metrics.raw.gpu_power_watts)
                        ),
                    );
                }
                if let Some(w) = host {
                    row(
                        &mut columns[0],
                        "Watts hôte",
                        format!("{:.1} W  [{}]", w, host_source),
                    );
                } else {
                    columns[0].label(
                        RichText::new(format!(
                            "Watts hôte N/A ({})",
                            Self::power_unavailable_hint(&vm.platform_info.os)
                        ))
                        .size(10.0)
                        .color(C_MUTED),
                    );
                }
                row(
                    &mut columns[0],
                    "Temp CPU",
                    vm.metrics
                        .raw
                        .cpu_temp_c
                        .map(|v| format!("{v:.1} °C"))
                        .unwrap_or_else(|| "N/A".to_string()),
                );
                row(
                    &mut columns[0],
                    "Load avg",
                    vm.metrics
                        .raw
                        .load_avg_1m_norm
                        .map(|v| format!("{v:.2} x/core"))
                        .unwrap_or_else(|| "N/A".to_string()),
                );
                row(
                    &mut columns[0],
                    "Faults",
                    vm.metrics
                        .raw
                        .page_faults_per_sec
                        .map(|v| format!("{v:.0}/s"))
                        .unwrap_or_else(|| "N/A".to_string()),
                );
                if let Some(ratio) = vm.metrics.compression {
                    let store_mb = ratio * vm.metrics.raw.mem_total_mb as f64;
                    let saved_mb = store_mb * 1.5;
                    row(
                        &mut columns[0],
                        "Store compressé",
                        format!("{:.0} MiB ({:.1}% RAM)", store_mb, ratio * 100.0),
                    );
                    row(
                        &mut columns[0],
                        "RAM éco. ~",
                        format!("{:.0} MiB évités en swap", saved_mb),
                    );
                }
                {
                    let sw = vm.metrics.raw.swap_used_mb;
                    let st = vm.metrics.raw.swap_total_mb;
                    row(
                        &mut columns[0],
                        "Swap/Pagefile",
                        if sw == 0 {
                            "inactif".to_string()
                        } else {
                            format!("{sw} / {st} MiB")
                        },
                    );
                }
                if let Some(z) = vm.metrics.raw.zram_used_mb {
                    row(&mut columns[0], "zRAM", format!("{z} MiB"));
                }
                if vm.metrics.raw.psi_cpu.is_some() || vm.metrics.raw.psi_mem.is_some() {
                    row(
                        &mut columns[0],
                        "PSI CPU/MEM",
                        format!(
                            "{:.1}% · {:.1}%",
                            vm.metrics.raw.psi_cpu.unwrap_or(0.0) * 100.0,
                            vm.metrics.raw.psi_mem.unwrap_or(0.0) * 100.0
                        ),
                    );
                }

                // ── Externe ──────────────────────────────────────────────────
                Self::eyebrow(&mut columns[1], "Externe");
                columns[1].add_space(4.0);
                if let Some(w) = wall {
                    LiteApp::kv_row_colored(
                        &mut columns[1],
                        "Watts mur",
                        &format!("{w:.1} W"),
                        C_CYAN,
                    );
                } else {
                    columns[1].label(RichText::new("Watts mur N/A").size(11.0).color(C_MUTED));
                }
                if !external_source.is_empty() && external_source != "aucune" {
                    row(&mut columns[1], "Source", external_source.clone());
                    row(
                        &mut columns[1],
                        "Fraîcheur",
                        if vm.external_status.is_fresh {
                            "fraîche".to_string()
                        } else {
                            "stale".to_string()
                        },
                    );
                    row(
                        &mut columns[1],
                        "Bridge",
                        if vm.external_bridge_running {
                            "actif".to_string()
                        } else {
                            "arrêté".to_string()
                        },
                    );
                    let path = &vm.external_status.power_file_path;
                    if !path.is_empty() {
                        columns[1].label(RichText::new(path).size(9.5).color(C_MUTED));
                    }
                } else {
                    columns[1].label(
                        RichText::new("Aucune prise externe connectée")
                            .size(11.0)
                            .color(C_MUTED),
                    );
                }
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
                    row(
                        &mut columns[1],
                        "Batterie",
                        format!("{bat_state} · {bat_pct}"),
                    );
                }

                // ── Écart ─────────────────────────────────────────────────────
                Self::eyebrow(&mut columns[2], "Écart");
                columns[2].add_space(4.0);
                match (host, wall) {
                    (Some(h), Some(w)) => {
                        let ratio = (h / w * 100.0).clamp(0.0, 100.0);
                        let unattr = (w - h).max(0.0);
                        LiteApp::kv_row(
                            &mut columns[2],
                            "Hôte / Mur",
                            &format!("{h:.1} / {w:.1} W"),
                        );
                        LiteApp::kv_row(
                            &mut columns[2],
                            "Hôte repr.",
                            &format!("{ratio:.1}% du mur"),
                        );
                        LiteApp::kv_row(&mut columns[2], "Non attribué", &format!("{unattr:.1} W"));
                        LiteApp::kv_row(&mut columns[2], "Confiance", confidence);
                    }
                    (None, Some(w)) => {
                        LiteApp::kv_row(&mut columns[2], "Mur", &format!("{w:.1} W"));
                        LiteApp::kv_row(&mut columns[2], "Hôte", "non mesuré");
                        LiteApp::kv_row(&mut columns[2], "Confiance", confidence);
                        columns[2].label(
                            RichText::new(Self::power_unavailable_hint(&vm.platform_info.os))
                                .size(9.5)
                                .color(C_MUTED),
                        );
                    }
                    (Some(h), None) => {
                        LiteApp::kv_row(&mut columns[2], "Hôte", &format!("{h:.1} W"));
                        LiteApp::kv_row(&mut columns[2], "Prise", "non connectée");
                        LiteApp::kv_row(&mut columns[2], "Confiance", confidence);
                    }
                    (None, None) => {
                        columns[2].label(
                            RichText::new("Aucune mesure de puissance disponible.")
                                .size(11.0)
                                .color(C_MUTED),
                        );
                        columns[2].label(
                            RichText::new(Self::power_unavailable_hint(&vm.platform_info.os))
                                .size(9.5)
                                .color(C_MUTED),
                        );
                    }
                }
            });
        });
    }

    // ── Target summary ─────────────────────────────────────────────────────────

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
        ui.label(
            RichText::new(format!("Cible  {target}"))
                .size(11.0)
                .color(C_MUTED),
        );
    }

    // ── Tuning summary ─────────────────────────────────────────────────────────

    fn tuning_summary(ui: &mut egui::Ui, vm: &LiteViewModel) {
        ui.label(
            RichText::new(format!(
                "Policy {}  ·  κ {:.2}  ·  Σmax {:.2}  ·  η {:.2}  ·  SoulRAM {}%",
                vm.policy_mode.as_name(),
                vm.kappa,
                vm.sigma_max,
                vm.eta,
                vm.soulram_percent
            ))
            .size(10.0)
            .color(C_MUTED),
        );
        let t = &vm.adaptive_tuning;
        ui.label(
            RichText::new(format!(
                "Formule dynamique {}  ·  λ {:.2}  ·  garde {:.0}%  ·  confiance {:.0}%  ·  attr {:.0}%  ·  reward EMA {:.0}%  ·  {} obs / {} skip",
                if t.enabled { "ON" } else { "OFF" },
                vm.kpi_lambda,
                vm.device_profile.auto_dome_guard_min * 100.0,
                t.decision_confidence * 100.0,
                t.process_attribution_confidence * 100.0,
                t.reward_ema * 100.0,
                t.samples,
                t.skipped_samples
            ))
            .size(10.0).color(C_MUTED),
        );
        if t.memory_fault_guard_active || !t.last_learning_note.is_empty() {
            ui.label(
                RichText::new(format!("↳ {}", t.last_learning_note))
                    .size(10.0)
                    .color(if t.memory_fault_guard_active {
                        C_YELLOW
                    } else {
                        C_MUTED
                    }),
            );
        }
    }

    // ── Action summary ─────────────────────────────────────────────────────────

    fn action_summary(ui: &mut egui::Ui, vm: &LiteViewModel) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            Self::status_chip(ui, "Dôme", vm.dome_active);
            Self::status_chip(ui, "SoulRAM", vm.soulram_active);
            ui.label(
                RichText::new(format!("backend {}", vm.soulram_backend.backend))
                    .size(10.0)
                    .color(C_MUTED),
            );
        });
        if !vm.last_actions.is_empty() {
            ui.label(
                RichText::new(format!("↳ {}", vm.last_actions[0]))
                    .size(10.0)
                    .color(C_MUTED),
            );
        }
    }

    // ── Priority hint ──────────────────────────────────────────────────────────

    fn priority_hint(ui: &mut egui::Ui, vm: &LiteViewModel) {
        let dome_delta_w = vm
            .telemetry
            .total
            .avg_power_dome_off_w
            .zip(vm.telemetry.total.avg_power_dome_on_w)
            .map(|(off, on)| off - on);
        let real_savings = vm.telemetry.total.energy_saved_kwh.unwrap_or(0.0) > 0.01
            || dome_delta_w.unwrap_or(0.0) > 5.0;

        let (accent, title, body) = if vm.metrics.sigma >= vm.sigma_max {
            (
                C_RED,
                "Pression élevée",
                "La machine est déjà tendue. Réduire l'agressivité ou cibler plus finement."
                    .to_string(),
            )
        } else if real_savings {
            let body = if let (Some(saved_kwh), Some(delta_w)) =
                (vm.telemetry.total.energy_saved_kwh, dome_delta_w)
            {
                format!(
                    "Moy. dôme ON vs OFF : {:.1} W d'écart ({:.4} kWh si cet écart était causal). \
                     Corrélation observée, pas une causalité prouvée.",
                    delta_w, saved_kwh
                )
            } else if let Some(delta_w) = dome_delta_w {
                format!(
                    "Moy. dôme ON vs OFF : {:.1} W d'écart mesuré (corrélation, pas causalité prouvée).",
                    delta_w
                )
            } else {
                format!(
                    "Écart énergétique session : {:.4} kWh (corrélation dôme ON/OFF).",
                    vm.telemetry.total.energy_saved_kwh.unwrap_or(0.0)
                )
            };
            (C_GREEN, "Données comparatives disponibles", body)
        } else if vm.formula.pi >= 0.6 {
            (
                C_GREEN,
                "Fenêtre favorable",
                "Le contexte est bon pour activer le dôme sur une cible utile.".to_string(),
            )
        } else {
            (
                C_YELLOW,
                "Impact modéré",
                "Le gain attendu semble limité. Vérifier d'abord la cible et la charge réelle."
                    .to_string(),
            )
        };

        egui::Frame::new()
            .fill(accent.gamma_multiply(0.08))
            .stroke(Stroke::new(1.0, accent.gamma_multiply(0.4)))
            .corner_radius(10.0)
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("●").size(9.0).color(accent));
                    ui.label(RichText::new(title).size(12.0).strong().color(accent));
                });
                ui.label(RichText::new(&body).size(11.0).color(C_MUTED));
            });
    }

    // ── Host impact panel ──────────────────────────────────────────────────────

    fn host_impact_panel(ui: &mut egui::Ui, vm: &LiteViewModel) {
        Self::panel_card(ui, |ui| {
            Self::section_title(
                ui,
                "Impact HOST",
                "Ce que SoulKernel mesure et canalise sur cette machine.",
            );

            let host_w = vm.metrics.raw.host_power_watts;
            let wall_w = vm.metrics.raw.wall_power_watts.or_else(|| {
                if vm.external_status.is_fresh {
                    vm.external_status.last_watts
                } else {
                    None
                }
            });
            let power_src = vm
                .metrics
                .raw
                .host_power_watts_source
                .as_deref()
                .or(vm.metrics.raw.wall_power_watts_source.as_deref())
                .unwrap_or(if vm.external_status.is_fresh && wall_w.is_some() {
                    "Meross"
                } else {
                    "aucun capteur"
                });

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                if let Some(w) = host_w.or(wall_w) {
                    Self::metric_badge(
                        ui,
                        "Puissance HOST",
                        format!("{:.1} W  [{}]", w, power_src),
                        C_CYAN,
                    );
                } else {
                    Self::metric_badge(
                        ui,
                        "Puissance HOST",
                        format!(
                            "N/A — {}",
                            Self::power_unavailable_hint(&vm.platform_info.os)
                        ),
                        C_MUTED,
                    );
                }

                let mem_pct = if vm.metrics.raw.mem_total_mb > 0 {
                    vm.metrics.raw.mem_used_mb as f64 / vm.metrics.raw.mem_total_mb as f64 * 100.0
                } else {
                    0.0
                };
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

                if let Some(ratio) = vm.metrics.compression {
                    let store_mb = ratio * vm.metrics.raw.mem_total_mb as f64;
                    let saved_mb = store_mb * 1.5;
                    let swap_used = vm.metrics.raw.swap_used_mb;
                    let faults = vm.metrics.raw.page_faults_per_sec.unwrap_or(0.0);
                    let (verdict_text, verdict_color) = if swap_used == 0 && faults < 500.0 {
                        ("bénéfique", C_GREEN)
                    } else if swap_used > 0 {
                        ("swap actif — pression élevée", C_YELLOW)
                    } else {
                        ("active", C_MUTED)
                    };
                    Self::metric_badge(
                        ui,
                        "Compression mém.",
                        format!(
                            "store {:.0} MiB · ~{:.0} MiB éco. · {}",
                            store_mb, saved_mb, verdict_text
                        ),
                        verdict_color,
                    );

                    let swap_label = if swap_used == 0 {
                        RichText::new("Swap/Pagefile  inactif")
                            .size(10.5)
                            .color(C_GREEN)
                    } else {
                        RichText::new(format!(
                            "Swap/Pagefile  {} MiB utilisé / {} MiB total",
                            swap_used, vm.metrics.raw.swap_total_mb
                        ))
                        .size(10.5)
                        .color(C_YELLOW)
                    };
                    ui.label(swap_label);
                }
            });

            if vm.soulram_active {
                ui.add_space(6.0);
                if vm.auto_cycle_soulram {
                    match vm.next_cycle_in_s {
                        Some(0) | None => {
                            if let Some(last_ms) = vm.last_auto_cycle_ms {
                                let age = vm.now_ms.saturating_sub(last_ms) / 1000;
                                ui.label(
                                    RichText::new(format!(
                                        "Auto-cycle SoulRAM actif — dernier cycle il y a {age}s"
                                    ))
                                    .size(10.5)
                                    .color(C_GREEN),
                                );
                            } else {
                                ui.label(
                                    RichText::new(
                                        "Auto-cycle SoulRAM actif — en attente de charge",
                                    )
                                    .size(10.5)
                                    .color(C_MUTED),
                                );
                            }
                        }
                        Some(remaining) => {
                            ui.label(
                                RichText::new(format!(
                                    "Auto-cycle SoulRAM — prochain cycle dans {}",
                                    crate::fmt::runtime_short(remaining)
                                ))
                                .size(10.5)
                                .color(C_MUTED),
                            );
                        }
                    }
                } else {
                    ui.label(
                        RichText::new(
                            "SoulRAM actif — auto-cycle désactivé (one-shot). Activer dans Commandes.",
                        ).size(10.5).color(C_MUTED),
                    );
                }
            }

            // ── KPI block ─────────────────────────────────────────────────────
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);
            {
                let kpi = &vm.kpi;
                let label_color = Self::kpi_color(&kpi.label);
                if kpi.self_overload {
                    egui::Frame::new()
                        .fill(C_RED.gamma_multiply(0.1))
                        .stroke(Stroke::new(1.0, C_RED.gamma_multiply(0.4)))
                        .corner_radius(8.0)
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.label(RichText::new(format!(
                                "⚠ SoulKernel {:.0}% CPU — l'optimiseur consomme plus qu'il n'optimise",
                                kpi.cpu_self_pct
                            )).size(11.0).color(C_RED));
                        });
                    ui.add_space(6.0);
                }

                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                    let cpu_out_of_scope = (kpi.cpu_total_pct
                        - kpi.cpu_useful_pct
                        - kpi.cpu_overhead_pct
                        - kpi.cpu_system_pct)
                        .max(0.0);
                    match kpi.kpi_penalized {
                        Some(k) => Self::metric_badge(
                            ui,
                            "KPI énergétique",
                            format!("{:.2} W/%  [{}]", k, kpi.label.as_str()),
                            label_color,
                        ),
                        None => Self::metric_badge(
                            ui,
                            "KPI énergétique",
                            "N/A — aucun capteur puissance".to_string(),
                            C_MUTED,
                        ),
                    }
                    Self::metric_badge(
                        ui,
                        "CPU utile (top-N)",
                        format!("{:.1}%", kpi.cpu_useful_pct),
                        Self::tone_for_ratio(
                            1.0 - kpi.cpu_useful_pct / 100.0_f64.max(kpi.cpu_total_pct),
                        ),
                    );
                    if kpi.cpu_overhead_pct > 1.0 {
                        Self::metric_badge(
                            ui,
                            "Overhead",
                            format!("{:.1}%", kpi.cpu_overhead_pct),
                            C_YELLOW,
                        );
                    }
                    if cpu_out_of_scope > 1.0 {
                        Self::metric_badge(
                            ui,
                            "CPU hors KPI",
                            format!("{:.1}%", cpu_out_of_scope),
                            C_MUTED,
                        );
                    }
                    if let Some(pf) = vm.metrics.raw.page_faults_per_sec {
                        if pf > 0.0 {
                            let fault_color = if pf > 5000.0 {
                                C_RED
                            } else if pf > 1500.0 {
                                C_YELLOW
                            } else {
                                C_MUTED
                            };
                            let warn = if pf > 5000.0 { " ⚠" } else { "" };
                            Self::metric_badge(
                                ui,
                                "Faults mém.",
                                format!("{:.0}k/s{}", pf / 1000.0, warn),
                                fault_color,
                            );
                        }
                    }
                    if let Some(trend) = kpi.trend {
                        let (trend_str, trend_color) = if trend > 1.0 {
                            (format!("↑ +{:.2}", trend), C_RED)
                        } else if trend < -1.0 {
                            (format!("↓ {:.2}", trend), C_GREEN)
                        } else {
                            (format!("→ {:.2}", trend), C_MUTED)
                        };
                        Self::metric_badge(ui, "Tendance", trend_str, trend_color);
                    }
                    let ratio = vm.kpi_memory.reward_ratio();
                    if !vm.kpi_memory.records.is_empty() {
                        Self::metric_badge(
                            ui,
                            "Actions efficaces",
                            format!("{:.0}%", ratio * 100.0),
                            if ratio >= 0.6 {
                                C_GREEN
                            } else if ratio >= 0.4 {
                                C_YELLOW
                            } else {
                                C_RED
                            },
                        );
                    }
                });
            }

            if let Some(delta) = &vm.host_impact {
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                Self::host_impact_delta_row(ui, delta, vm.now_ms);
            }
        });
    }

    // ── Host impact delta row ─────────────────────────────────────────────────

    fn host_impact_delta_row(ui: &mut egui::Ui, delta: &HostImpactDelta, now_ms: u64) {
        let age_s = now_ms.saturating_sub(delta.captured_at_ms) / 1000;
        ui.label(
            RichText::new(format!(
                "Résultat dernière action : {}  (il y a {}s)",
                delta.source, age_s
            ))
            .size(12.0)
            .strong(),
        );
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
            let freed = delta.mem_freed_mb();
            let ram_color = if freed > 50 {
                C_GREEN
            } else if freed < -50 {
                C_RED
            } else {
                C_MUTED
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
            if let Some(pct) = delta.page_faults_reduction_pct() {
                let color = if pct > 10.0 {
                    C_GREEN
                } else if pct < -10.0 {
                    C_RED
                } else {
                    C_MUTED
                };
                Self::metric_badge(ui, "Page faults", format!("{:+.0}%", -pct), color);
            } else if delta.page_faults_before.is_some() || delta.page_faults_after.is_some() {
                Self::metric_badge(ui, "Page faults", "mesure en cours".to_string(), C_MUTED);
            }
            if let Some(saved) = delta.power_saved_w() {
                let color = if saved > 1.0 {
                    C_GREEN
                } else if saved < -1.0 {
                    C_RED
                } else {
                    C_MUTED
                };
                Self::metric_badge(ui, "Puissance", format!("{:+.1} W", -saved), color);
            }
            if let (Some(before), Some(after)) = (delta.compression_before, delta.compression_after)
            {
                let delta_ratio = after - before;
                let color = if delta_ratio > 0.02 { C_GREEN } else { C_MUTED };
                Self::metric_badge(
                    ui,
                    "Compression",
                    format!("{:.1}% → {:.1}%", before * 100.0, after * 100.0),
                    color,
                );
            }
        });
    }

    // ── Decision panel ─────────────────────────────────────────────────────────

    fn decision_panel(ui: &mut egui::Ui, vm: &LiteViewModel) {
        Self::panel_card(ui, |ui| {
            Self::section_title(
                ui,
                "État système",
                "Pression, fenêtre d'action et garde — en un coup d'œil.",
            );
            let (pressure_text, pressure_desc, pressure_color) = if vm.metrics.sigma >= vm.sigma_max
            {
                (
                    "Pression élevée",
                    "La machine est tendue — éviter d'ajouter une charge.",
                    C_RED,
                )
            } else if vm.metrics.sigma >= vm.sigma_max * 0.75 {
                (
                    "Pression surveillée",
                    "Charge modérée — agir avec précaution.",
                    C_YELLOW,
                )
            } else {
                ("Pression basse", "La machine est à l'aise.", C_GREEN)
            };
            let (window_text, window_desc, window_color) = if vm.formula.pi >= 0.6 {
                (
                    "Fenêtre ouverte",
                    "Bon moment pour activer le dôme.",
                    C_GREEN,
                )
            } else if vm.formula.pi >= 0.35 {
                (
                    "Fenêtre modérée",
                    "L'action peut avoir un effet limité.",
                    C_YELLOW,
                )
            } else {
                (
                    "Fenêtre fermée",
                    "Peu de gain attendu dans ce contexte.",
                    C_RED,
                )
            };
            let (guard_text, guard_desc, guard_color) = if vm.formula.advanced_guard >= 0.85 {
                ("Garde ouverte", "SoulKernel peut agir librement.", C_GREEN)
            } else if vm.formula.advanced_guard >= 0.5 {
                (
                    "Garde prudente",
                    "SoulKernel attend un meilleur contexte.",
                    C_YELLOW,
                )
            } else {
                (
                    "Garde fermée",
                    "SoulKernel bloque l'action pour protéger le HOST.",
                    C_RED,
                )
            };
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                Self::metric_badge(ui, pressure_text, pressure_desc.to_string(), pressure_color);
                Self::metric_badge(ui, window_text, window_desc.to_string(), window_color);
                Self::metric_badge(ui, guard_text, guard_desc.to_string(), guard_color);
            });
        });
    }

    // ── Gains panel ────────────────────────────────────────────────────────────

    fn gains_panel(ui: &mut egui::Ui, vm: &LiteViewModel) {
        let lt = &vm.telemetry.lifetime;
        let mem = &vm.kpi_memory;

        Self::panel_card(ui, |ui| {
            Self::section_title(
                ui,
                "Gains SoulKernel",
                "Ce que l'application a concrètement fait pour toi depuis le premier lancement.",
            );

            if lt.first_launch_ts > 0 {
                let monitored_h =
                    lt.total_idle_hours + lt.total_dome_hours + lt.soulram_active_hours;
                if monitored_h > 0.01 {
                    ui.label(
                        RichText::new(format!(
                            "Suivi depuis {:.0}h  ({} samples  ·  {:.0}h idle  ·  {:.1}h dôme)",
                            monitored_h, lt.total_samples, lt.total_idle_hours, lt.total_dome_hours,
                        ))
                        .size(10.5)
                        .color(C_MUTED),
                    );
                }
            }

            ui.add_space(8.0);

            // ── Dôme ──────────────────────────────────────────────────────────
            Self::eyebrow(ui, "Dôme");
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                Self::metric_badge(
                    ui,
                    "Activations",
                    format!("{}", lt.total_dome_activations),
                    if lt.total_dome_activations > 0 {
                        C_GREEN
                    } else {
                        C_MUTED
                    },
                );
                Self::metric_badge(
                    ui,
                    "Temps actif",
                    format!("{:.1}h", lt.total_dome_hours),
                    C_MUTED,
                );
                if lt.total_cpu_hours_differential > 0.0 {
                    Self::metric_badge(
                        ui,
                        "CPU·h éco.",
                        format!("{:.3} CPU·h", lt.total_cpu_hours_differential),
                        C_GREEN,
                    );
                }
                if lt.total_mem_gb_hours_differential > 0.0 {
                    Self::metric_badge(
                        ui,
                        "RAM·GB·h libérée",
                        format!("{:.3} GB·h", lt.total_mem_gb_hours_differential),
                        C_GREEN,
                    );
                }
            });

            ui.add_space(8.0);

            // ── SoulRAM ───────────────────────────────────────────────────────
            Self::eyebrow(ui, "SoulRAM");
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                Self::metric_badge(
                    ui,
                    "Temps actif",
                    format!("{:.1}h", lt.soulram_active_hours),
                    if lt.soulram_active_hours > 0.0 {
                        C_GREEN
                    } else {
                        C_MUTED
                    },
                );
            });

            ui.add_space(8.0);

            // ── Efficacité des actions ────────────────────────────────────────
            Self::eyebrow(ui, "Efficacité des actions");
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                let reward = mem.reward_ratio();
                Self::metric_badge(
                    ui,
                    "Actions efficaces (session)",
                    format!("{:.0}%", reward * 100.0),
                    if reward >= 0.6 {
                        C_GREEN
                    } else if reward >= 0.4 {
                        C_YELLOW
                    } else {
                        C_RED
                    },
                );
                if let Some(avg_delta) = mem.avg_kpi_gain() {
                    Self::metric_badge(
                        ui,
                        "Δ KPI médian (session)",
                        format!("{:+.2} W/%", avg_delta),
                        C_GREEN,
                    );
                }
                if let Some(gain_pct) = lt.avg_kpi_gain_pct {
                    Self::metric_badge(
                        ui,
                        "Amélioration KPI (lifetime)",
                        format!("{:+.1}%", gain_pct),
                        if gain_pct < 0.0 { C_GREEN } else { C_RED },
                    );
                }
            });

            ui.add_space(8.0);

            // ── Énergie & coût ────────────────────────────────────────────────
            Self::eyebrow(ui, "Énergie & coût");
            ui.add_space(4.0);
            if lt.has_real_power {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                    Self::metric_badge(
                        ui,
                        "Énergie mesurée",
                        format!("{:.3} kWh", lt.total_energy_kwh),
                        C_CYAN,
                    );
                    if lt.total_energy_cost_measured > 0.0 {
                        Self::metric_badge(
                            ui,
                            "Coût cumulé",
                            format!(
                                "{:.2} {}",
                                lt.total_energy_cost_measured, vm.telemetry.pricing.currency
                            ),
                            C_CYAN,
                        );
                    }
                    if lt.total_co2_measured_kg > 0.0 {
                        Self::metric_badge(
                            ui,
                            "CO₂ mesuré",
                            format!("{:.3} kg", lt.total_co2_measured_kg),
                            C_MUTED,
                        );
                    }
                });
                if let Some(saved) = vm.telemetry.total.energy_saved_kwh.filter(|&v| v > 0.0) {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "Économie estimée dôme cette session  {:.4} kWh  (~{:.3} {})",
                            saved,
                            saved * vm.telemetry.pricing.price_per_kwh,
                            vm.telemetry.pricing.currency,
                        ))
                        .size(11.0)
                        .color(C_GREEN),
                    );
                }
            } else {
                egui::Frame::new()
                    .fill(C_YELLOW.gamma_multiply(0.07))
                    .stroke(Stroke::new(1.0, C_YELLOW.gamma_multiply(0.3)))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(
                                "Capteur de puissance non disponible — branchez un Meross ou activez RAPL \
                                 pour mesurer kWh et calculer les économies en euros.",
                            ).size(11.0).color(C_YELLOW),
                        );
                        if lt.total_cpu_hours_differential > 0.0 {
                            let est_kwh = lt.total_cpu_hours_differential * 0.5;
                            let est_cost = est_kwh * vm.telemetry.pricing.price_per_kwh;
                            ui.label(
                                RichText::new(format!(
                                    "Estimation sans capteur (0.5 W/%·CPU)  ~{:.4} kWh  ~{:.3} {}",
                                    est_kwh, est_cost, vm.telemetry.pricing.currency,
                                )).size(10.5).color(C_MUTED),
                            );
                        }
                    });
            }
        });
    }

    // ── Telemetry panel ────────────────────────────────────────────────────────

    fn telemetry_panel(ui: &mut egui::Ui, vm: &LiteViewModel) {
        Self::panel_card(ui, |ui| {
            Self::section_title(
                ui,
                "Impact mesuré",
                "Énergie consommée et gain du dôme depuis le début de la session.",
            );
            let live_w = vm.telemetry.live_power_w;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                Self::metric_badge(ui, "Source", vm.telemetry.power_source.clone(), C_CYAN);
                Self::metric_badge(
                    ui,
                    "Live",
                    fmt::watts(live_w),
                    if live_w.is_some() { C_CYAN } else { C_MUTED },
                );
                if vm.telemetry.total.dome_active_ratio > 0.0 {
                    Self::metric_badge(
                        ui,
                        "Dôme actif",
                        format!(
                            "{:.0}% du temps",
                            vm.telemetry.total.dome_active_ratio * 100.0
                        ),
                        C_GREEN,
                    );
                }
            });

            if vm.telemetry.total.avg_power_w.is_some()
                || vm.telemetry.total.avg_power_dome_on_w.is_some()
            {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "Puiss. moy. {}  |  dôme ON {}  |  dôme OFF {}",
                        fmt::watts(vm.telemetry.total.avg_power_w),
                        fmt::watts(vm.telemetry.total.avg_power_dome_on_w),
                        fmt::watts(vm.telemetry.total.avg_power_dome_off_w)
                    ))
                    .size(11.0)
                    .color(C_MUTED),
                );
                if let Some(saved) = vm.telemetry.total.energy_saved_kwh.filter(|&v| v > 0.0) {
                    ui.label(
                        RichText::new(format!(
                            "Économie estimée dôme {saved:.3} kWh cette session"
                        ))
                        .size(11.0)
                        .color(C_GREEN),
                    );
                }
            }

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            // Time windows
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                Self::metric_badge(
                    ui,
                    "1h",
                    format!("{:.3} kWh", vm.telemetry.hour.energy_kwh),
                    C_MUTED,
                );
                Self::metric_badge(
                    ui,
                    "24h",
                    format!("{:.3} kWh", vm.telemetry.day.energy_kwh),
                    C_MUTED,
                );
                Self::metric_badge(
                    ui,
                    "7j",
                    format!("{:.3} kWh", vm.telemetry.week.energy_kwh),
                    C_MUTED,
                );
                Self::metric_badge(
                    ui,
                    "30j",
                    format!("{:.3} kWh", vm.telemetry.month.energy_kwh),
                    C_MUTED,
                );
            });

            if vm.telemetry.lifetime.total_energy_kwh > 0.0 {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "Total vie  {:.3} kWh  ·  CO₂ {:.3} kg  ·  coût {:.2} {}",
                        vm.telemetry.lifetime.total_energy_kwh,
                        vm.telemetry.lifetime.total_co2_measured_kg,
                        vm.telemetry.lifetime.total_energy_cost_measured,
                        vm.telemetry.pricing.currency
                    ))
                    .size(11.0)
                    .color(C_MUTED),
                );
            }
        });
    }

    // ── External power panel ───────────────────────────────────────────────────

    fn external_power_panel(
        ui: &mut egui::Ui,
        state: &mut LiteState,
        error: &mut Option<String>,
        info: &mut Option<String>,
    ) {
        Self::panel_card(ui, |ui| {
            Self::section_title(
                ui,
                "Prise intelligente (Meross)",
                "Mesure murale optionnelle — pour voir ce que la prise consomme vraiment.",
            );
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                let bridge_color = if state.vm.external_bridge_running {
                    C_GREEN
                } else {
                    C_MUTED
                };
                Self::metric_badge(
                    ui,
                    "Bridge",
                    if state.vm.external_bridge_running {
                        "actif".to_string()
                    } else {
                        "arrêté".to_string()
                    },
                    bridge_color,
                );
                let fresh_color = if state.vm.external_status.is_fresh {
                    C_GREEN
                } else {
                    C_YELLOW
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
                    RichText::new(&state.vm.external_bridge_detail)
                        .size(10.5)
                        .color(C_MUTED),
                );
            }
            ui.add_space(8.0);
            ui.checkbox(
                &mut state.vm.external_config.enabled,
                "Activer la source externe",
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new("Fichier données").size(11.0).color(C_MUTED));
                let path = state.vm.external_config.power_file.get_or_insert_with(|| {
                    soulkernel_core::external_power::default_power_file()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default()
                });
                ui.text_edit_singleline(path);
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Python").size(11.0).color(C_MUTED));
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
                    ui.label(RichText::new("E-mail").size(11.0).color(C_MUTED));
                    let email = state
                        .vm
                        .external_config
                        .meross_email
                        .get_or_insert_default();
                    ui.text_edit_singleline(email);
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Mot de passe").size(11.0).color(C_MUTED));
                    let pwd = state
                        .vm
                        .external_config
                        .meross_password
                        .get_or_insert_default();
                    ui.add(egui::TextEdit::singleline(pwd).password(true));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Région").size(11.0).color(C_MUTED));
                    let region = state
                        .vm
                        .external_config
                        .meross_region
                        .get_or_insert("eu".to_string());
                    ui.text_edit_singleline(region);
                    ui.label(RichText::new("Modèle").size(11.0).color(C_MUTED));
                    let device = state
                        .vm
                        .external_config
                        .meross_device_type
                        .get_or_insert("mss315".to_string());
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
                    RichText::new(format!("Log: {}", state.vm.external_status.bridge_log_path))
                        .size(10.0)
                        .color(C_MUTED),
                );
            }
        });
    }

    // ── Benchmark panel ────────────────────────────────────────────────────────

    fn benchmark_panel(
        ui: &mut egui::Ui,
        state: &mut LiteState,
        error: &mut Option<String>,
        info: &mut Option<String>,
    ) {
        Self::panel_card(ui, |ui| {
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
                    ui.label(RichText::new("Commande").size(11.0).color(C_MUTED));
                    ui.add_enabled(
                        !state.vm.benchmark_use_system_probe,
                        egui::TextEdit::singleline(&mut state.vm.benchmark_command),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Arguments").size(11.0).color(C_MUTED));
                    ui.add_enabled(
                        !state.vm.benchmark_use_system_probe,
                        egui::TextEdit::singleline(&mut state.vm.benchmark_args),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Dossier").size(11.0).color(C_MUTED));
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
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                    Self::metric_badge(
                        ui,
                        "Médiane",
                        session
                            .summary
                            .gain_median_pct
                            .map(|v| format!("{v:.1}%"))
                            .unwrap_or_else(|| "N/A".to_string()),
                        C_GREEN,
                    );
                    Self::metric_badge(
                        ui,
                        "p95",
                        session
                            .summary
                            .gain_p95_pct
                            .map(|v| format!("{v:.1}%"))
                            .unwrap_or_else(|| "N/A".to_string()),
                        C_GREEN,
                    );
                    Self::metric_badge(
                        ui,
                        "Efficacité",
                        session
                            .summary
                            .efficiency_score
                            .map(|v| format!("{v:.2}"))
                            .unwrap_or_else(|| "N/A".to_string()),
                        C_CYAN,
                    );
                    let uw_pct = session.summary.gain_utility_per_watt_pct.unwrap_or(0.0);
                    Self::metric_badge(
                        ui,
                        "U/W",
                        session
                            .summary
                            .gain_utility_per_watt_pct
                            .map(|v| format!("{v:+.1}%"))
                            .unwrap_or_else(|| "N/A".to_string()),
                        if uw_pct >= 0.0 { C_GREEN } else { C_RED },
                    );
                    let kw_pct = session.summary.gain_kwh_per_utility_pct.unwrap_or(0.0);
                    Self::metric_badge(
                        ui,
                        "kWh/U",
                        session
                            .summary
                            .gain_kwh_per_utility_pct
                            .map(|v| format!("{v:+.1}%"))
                            .unwrap_or_else(|| "N/A".to_string()),
                        if kw_pct >= 0.0 { C_GREEN } else { C_RED },
                    );
                });
                if let (Some(off), Some(on)) = (
                    session.summary.measured_efficiency_off.as_ref(),
                    session.summary.measured_efficiency_on.as_ref(),
                ) {
                    ui.label(
                        RichText::new(format!(
                            "Mesuré: OFF {:.4} U/W → ON {:.4} U/W  ·  OFF {:.6} kWh/U → ON {:.6} kWh/U",
                            off.utility_per_watt, on.utility_per_watt,
                            off.kwh_per_utility, on.kwh_per_utility
                        )).size(10.0).color(C_MUTED),
                    );
                }
            }
            if let Some(history) = &state.vm.benchmark_history {
                if history.sessions.len() > 1 {
                    ui.label(
                        RichText::new(format!("{} sessions enregistrées", history.sessions.len()))
                            .size(11.0)
                            .color(C_MUTED),
                    );
                }
                if let Some(advice) = &history.advice {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Réglages conseillés")
                            .size(12.0)
                            .strong()
                            .color(C_TEXT),
                    );
                    ui.label(
                        RichText::new(format!(
                            "κ {:.1}  ·  Σmax {:.2}  ·  η {:.2}  ·  policy {}",
                            advice.recommended_kappa,
                            advice.recommended_sigma_max,
                            advice.recommended_eta,
                            advice.recommended_policy_mode
                        ))
                        .size(11.0)
                        .color(C_MUTED),
                    );
                }
            }
        });
    }

    // ── Inventory panel ────────────────────────────────────────────────────────

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
            ui.label(RichText::new(title).size(11.5).strong().color(C_CYAN));
            for item in items {
                ui.horizontal_wrapped(|ui| {
                    if !item.name.is_empty() {
                        ui.label(RichText::new(&item.name).strong());
                    }
                    ui.label(
                        RichText::new(format!("[{}]", item.kind))
                            .size(10.0)
                            .color(C_MUTED),
                    );
                    if endpoint_budget_w.is_some() {
                        let estimated_w = endpoint_budget_w
                            .map(|budget| budget * (endpoint_weight(&item.kind) / total_weight));
                        ui.label(
                            RichText::new(format!("~{}", crate::fmt::watts(estimated_w)))
                                .size(10.0),
                        );
                    }
                    if let Some(scope) = &item.measurement_scope {
                        if scope != "detected" {
                            ui.label(RichText::new(format!("[{scope}]")).size(9.5).color(C_MUTED));
                        }
                    }
                    if let Some(active_state) = &item.active_state {
                        let (label, color) = match active_state.as_str() {
                            "active" => ("actif", C_GREEN),
                            "connected" => ("connecté", Color32::from_rgb(100, 149, 237)),
                            "idle" => ("veille", C_MUTED),
                            other => (other, C_MUTED),
                        };
                        ui.colored_label(color, label);
                    }
                    if let Some(link) = &item.physical_link_hint {
                        ui.label(RichText::new(link).size(9.5).color(C_MUTED));
                    }
                    if let Some(score) = item.confidence_score {
                        if score < 0.64 || score > 0.66 {
                            let color = if score >= 0.85 {
                                C_GREEN
                            } else if score >= 0.6 {
                                C_YELLOW
                            } else {
                                C_RED
                            };
                            ui.colored_label(color, format!("{:.0}%", score * 100.0));
                        }
                    }
                    if let Some(detail) = &item.detail {
                        ui.label(RichText::new(detail).size(10.0).color(C_MUTED));
                    }
                });
            }
            ui.add_space(4.0);
        }

        Self::panel_card(ui, |ui| {
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
            ui.label(
                RichText::new(format!(
                    "Inventaire: {} displays · {} GPU · {} storage · {} net · {} endpoints",
                    state.vm.device_inventory.displays.len(),
                    state.vm.device_inventory.gpus.len(),
                    state.vm.device_inventory.storage.len(),
                    state.vm.device_inventory.network.len(),
                    state.vm.device_inventory.connected_endpoints.len()
                ))
                .size(10.5)
                .color(C_MUTED),
            );
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .id_salt("endpoints_scroll")
                .max_height(260.0)
                .show(ui, |ui| {
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

    // ── HUD panel ──────────────────────────────────────────────────────────────

    fn hud_panel(ui: &mut egui::Ui, state: &mut LiteState) {
        Self::panel_card(ui, |ui| {
            Self::section_title(
                ui,
                "HUD natif",
                "Mini vue toujours visible pour les signaux utiles pendant l'usage.",
            );
            ui.checkbox(&mut state.vm.show_hud, "Afficher le HUD compact");
            ui.label(
                RichText::new("Mode lite: HUD compact natif intégré, sans WebView.")
                    .size(11.0)
                    .color(C_MUTED),
            );
        });
    }

    // ── HUD overlay ───────────────────────────────────────────────────────────

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
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(6, 16, 25, 220))
                    .stroke(Stroke::new(1.0, C_BORDER))
                    .corner_radius(14.0)
                    .inner_margin(egui::Margin::same(12)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    Self::status_chip(ui, "Dome", vm.dome_active);
                    Self::status_chip(ui, "SoulRAM", vm.soulram_active);
                    let (pw_str, pw_col) = match power {
                        Some(w) => (format!("{:.1} W", w), C_CYAN),
                        None => ("— W".to_string(), C_MUTED),
                    };
                    ui.label(RichText::new(pw_str).size(13.0).strong().color(pw_col));
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "CPU {}  RAM {}",
                        fmt::pct(vm.metrics.raw.cpu_pct),
                        fmt::gib_pair(vm.metrics.raw.mem_used_mb, vm.metrics.raw.mem_total_mb)
                    ))
                    .size(11.0)
                    .color(C_TEXT),
                );
                {
                    let kpi_color = Self::kpi_color(&vm.kpi.label);
                    let kpi_str = vm
                        .kpi
                        .kpi_penalized
                        .map(|k| format!("KPI {:.1} W/%  [{}]", k, vm.kpi.label.as_str()))
                        .unwrap_or_else(|| "KPI —".to_string());
                    ui.label(RichText::new(kpi_str).size(11.0).strong().color(kpi_color));
                }
                if vm.metrics.raw.gpu_pct.map_or(false, |g| g > 0.5) {
                    ui.label(
                        RichText::new(format!(
                            "GPU {}  I/O R{:.1}/W{:.1} MB/s",
                            fmt::opt_pct(vm.metrics.raw.gpu_pct),
                            vm.metrics.raw.io_read_mb_s.unwrap_or(0.0),
                            vm.metrics.raw.io_write_mb_s.unwrap_or(0.0)
                        ))
                        .size(10.5)
                        .color(C_MUTED),
                    );
                }
            });
    }

    // ── Pilotage panel ─────────────────────────────────────────────────────────

    fn pilotage_panel(
        ui: &mut egui::Ui,
        state: &mut LiteState,
        error: &mut Option<String>,
        info: &mut Option<String>,
    ) {
        Self::panel_card(ui, |ui| {
            Self::section_title(
                ui,
                "Commandes",
                "Choisir une cible, activer le dôme, libérer la mémoire.",
            );
            let action_busy = state.is_action_in_flight();
            if action_busy {
                egui::Frame::new()
                    .fill(C_YELLOW.gamma_multiply(0.08))
                    .stroke(Stroke::new(1.0, C_YELLOW.gamma_multiply(0.4)))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("↻ action système en cours — mise à jour différée")
                                .size(11.0)
                                .color(C_YELLOW),
                        );
                    });
                ui.add_space(6.0);
            }

            Self::action_summary(ui, &state.vm);
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // ── Cible ─────────────────────────────────────────────────────────
            Self::eyebrow(ui, "Cible");
            ui.add_space(4.0);
            ui.checkbox(&mut state.vm.auto_target, "Cible automatique");
            if !state.vm.auto_target {
                let selected_label = state
                    .vm
                    .manual_target_pid
                    .map(|pid| {
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
                    })
                    .unwrap_or_else(|| "aucune".to_string());

                egui::ComboBox::from_label("Cible manuelle")
                    .selected_text(selected_label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.vm.manual_target_pid, None, "aucune");
                        let multi_groups: Vec<_> = state
                            .vm
                            .process_report
                            .groups
                            .iter()
                            .filter(|g| g.instance_count > 1 && g.total_cpu_pct >= 0.1)
                            .collect();
                        if !multi_groups.is_empty() {
                            ui.separator();
                            ui.label(
                                RichText::new("— Applications (plusieurs instances) —")
                                    .size(10.0)
                                    .color(C_MUTED),
                            );
                            for g in multi_groups {
                                ui.selectable_value(
                                    &mut state.vm.manual_target_pid,
                                    Some(g.top_pid),
                                    format!(
                                        "{} ×{}  {:.1}%  {}",
                                        g.name,
                                        g.instance_count,
                                        g.total_cpu_pct,
                                        fmt::mib_from_kb(g.total_memory_kb)
                                    ),
                                );
                            }
                            ui.separator();
                            ui.label(
                                RichText::new("— Processus individuels —")
                                    .size(10.0)
                                    .color(C_MUTED),
                            );
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

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            // ── Dôme ─────────────────────────────────────────────────────────
            Self::eyebrow(ui, "Dôme");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                Self::status_chip(
                    ui,
                    if state.vm.dome_active {
                        "ACTIF"
                    } else {
                        "inactif"
                    },
                    state.vm.dome_active,
                );
                if ui
                    .add_enabled(
                        !state.vm.dome_active && !action_busy,
                        egui::Button::new(RichText::new("⚡ Activer").size(12.0))
                            .fill(C_GREEN.gamma_multiply(0.2))
                            .stroke(Stroke::new(1.0, C_GREEN.gamma_multiply(0.5)))
                            .corner_radius(8.0),
                    )
                    .clicked()
                {
                    match state.activate_dome() {
                        Ok(()) => *info = Some("Activation du dôme lancée".to_string()),
                        Err(err) => *error = Some(err),
                    }
                }
                if ui
                    .add_enabled(
                        state.vm.dome_active && !action_busy,
                        egui::Button::new(RichText::new("↩ Annuler").size(12.0))
                            .fill(C_RED.gamma_multiply(0.15))
                            .stroke(Stroke::new(1.0, C_RED.gamma_multiply(0.4)))
                            .corner_radius(8.0),
                    )
                    .clicked()
                {
                    match state.rollback_dome() {
                        Ok(()) => *info = Some("Rollback du dôme lancé".to_string()),
                        Err(err) => *error = Some(err),
                    }
                }
                ui.checkbox(&mut state.vm.auto_dome, "Auto (KPI)");
            });
            if state.vm.auto_dome {
                let (auto_label, auto_color) = if state.vm.kpi.self_overload {
                    (
                        format!(
                            "⚠ suspendu — SoulKernel {:.0}% CPU",
                            state.vm.kpi.cpu_self_pct
                        ),
                        C_RED,
                    )
                } else if state.vm.dome_active {
                    (
                        format!(
                            "actif — KPI {:.1} W/% [{}]",
                            state.vm.kpi.kpi_penalized.unwrap_or(0.0),
                            state.vm.kpi.label.as_str()
                        ),
                        C_GREEN,
                    )
                } else {
                    match state.vm.auto_dome_next_eval_s {
                        Some(s) if s > 0 => (
                            format!("cooldown {}s — KPI {}", s, state.vm.kpi.label.as_str()),
                            C_MUTED,
                        ),
                        _ => (
                            format!(
                                "prêt — KPI {} / garde {:.0}%",
                                state.vm.kpi.label.as_str(),
                                state.vm.formula.advanced_guard * 100.0
                            ),
                            if state
                                .vm
                                .kpi
                                .should_act_with_profile(&state.vm.device_profile)
                                && state.vm.formula.advanced_guard
                                    >= state.vm.device_profile.auto_dome_guard_min
                            {
                                C_YELLOW
                            } else {
                                C_MUTED
                            },
                        ),
                    }
                };
                ui.label(
                    RichText::new(format!("↳ {auto_label}"))
                        .size(10.5)
                        .color(auto_color),
                );
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            // ── SoulRAM ───────────────────────────────────────────────────────
            Self::eyebrow(ui, "SoulRAM");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if ui
                    .add_enabled(
                        !action_busy,
                        egui::Button::new(RichText::new("🧠 Activer").size(12.0))
                            .fill(C_CYAN.gamma_multiply(0.15))
                            .stroke(Stroke::new(1.0, C_CYAN.gamma_multiply(0.4)))
                            .corner_radius(8.0),
                    )
                    .clicked()
                {
                    if let Err(err) = state.enable_soulram() {
                        *error = Some(err);
                    } else {
                        *info = Some("Activation SoulRAM lancée".to_string());
                    }
                }
                if ui
                    .add_enabled(
                        !action_busy,
                        egui::Button::new(RichText::new("🧠 Désactiver").size(12.0))
                            .fill(C_PANEL3)
                            .stroke(Stroke::new(1.0, C_BORDER))
                            .corner_radius(8.0),
                    )
                    .clicked()
                {
                    if let Err(err) = state.disable_soulram() {
                        *error = Some(err);
                    } else {
                        *info = Some("Désactivation SoulRAM lancée".to_string());
                    }
                }
            });
            ui.checkbox(
                &mut state.vm.auto_cycle_soulram,
                "Auto-cycle (re-application automatique)",
            );
            if state.vm.auto_cycle_soulram {
                let mode_hint =
                    if soulkernel_core::workload_catalog::is_burst(&state.vm.selected_workload) {
                        "Burst : cycle toutes les ~3 min"
                    } else {
                        "Sustain : cycle toutes les ~15 min"
                    };
                ui.label(RichText::new(mode_hint).size(10.5).color(C_MUTED));
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
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
            ui.add_space(6.0);
            ui.collapsing("Réglages avancés", |ui| {
                egui::ComboBox::from_label("Profil appareil")
                    .selected_text(state.vm.device_profile.label)
                    .show_ui(ui, |ui| {
                        for p in soulkernel_core::device_profile::DeviceProfile::list_all() {
                            let label = p.label;
                            let id = p.id;
                            let selected = state.vm.device_profile.id == id;
                            if ui
                                .selectable_label(
                                    selected,
                                    format!(
                                        "{} — {}",
                                        label,
                                        if p.can_act {
                                            "actions activées"
                                        } else {
                                            "monitoring seul"
                                        }
                                    ),
                                )
                                .clicked()
                            {
                                state.vm.device_profile = p;
                                state.reset_adaptive_tuning_for_profile();
                            }
                        }
                    });
                if !state.vm.device_profile.can_act {
                    ui.colored_label(C_YELLOW, "Monitoring seul — dôme et SoulRAM désactivés.");
                }
                ui.separator();
                let mut adaptive_enabled = state.vm.adaptive_tuning.enabled;
                if ui
                    .checkbox(&mut adaptive_enabled, "Ajustement dynamique de la formule")
                    .changed()
                {
                    state.set_adaptive_tuning_enabled(adaptive_enabled);
                }
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
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("Audit  {}", state.vm.audit_path))
                    .size(9.5)
                    .color(C_MUTED),
            );
            ui.label(
                RichText::new(format!("Observabilité  {}", state.vm.observability_path))
                    .size(9.5)
                    .color(C_MUTED),
            );
            ui.label(
                RichText::new(format!(
                    "Journal time-series auto  fichier courant .jsonl + archives .jsonl.gz  rotation à partir de {:.0} MiB  archives conservées: 8",
                    crate::export::observability_rotation_bytes() as f64 / (1024.0 * 1024.0)
                )).size(9.5).color(C_MUTED),
            );
            if cfg!(target_os = "windows") {
                ui.label(
                    RichText::new(
                        "Windows  AppData/Roaming/SoulKernel/telemetry/observability_samples.jsonl",
                    )
                    .size(9.5)
                    .color(C_MUTED),
                );
            } else {
                ui.label(RichText::new("Linux/macOS  XDG_DATA_HOME ou ~/.local/share/SoulKernel/telemetry/observability_samples.jsonl").size(9.5).color(C_MUTED));
            }
        });
    }

    // ── Remote supervisor panel ────────────────────────────────────────────────

    fn remote_supervisor_panel(
        ui: &mut egui::Ui,
        state: &mut LiteState,
        error: &mut Option<String>,
        info: &mut Option<String>,
    ) {
        Self::panel_card(ui, |ui| {
            Self::section_title(
                ui,
                "Superviseur distant",
                "Pousse les ticks d'observabilité vers SoulKernel-Supervisor pour la supervision live distante.",
            );

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                let status = &state.vm.remote_supervisor_status;
                let enrolled = !state.vm.remote_supervisor_config.api_key.trim().is_empty();
                let color = if status.connected {
                    C_GREEN
                } else if status.last_error.is_some() {
                    C_RED
                } else if !enrolled {
                    C_YELLOW
                } else {
                    C_MUTED
                };
                Self::metric_badge(
                    ui,
                    "Statut",
                    if status.connected {
                        "connecté".to_string()
                    } else if status.last_error.is_some() {
                        status
                            .last_error_kind
                            .as_deref()
                            .map(|k| match k {
                                "network" => "erreur réseau".to_string(),
                                "http" => "erreur HTTP".to_string(),
                                "runtime" => "erreur interne".to_string(),
                                _ => "erreur".to_string(),
                            })
                            .unwrap_or_else(|| "erreur".to_string())
                    } else if !enrolled {
                        "non enrôlé".to_string()
                    } else {
                        "inactif".to_string()
                    },
                    color,
                );
                Self::metric_badge(
                    ui,
                    "Enrôlement",
                    if enrolled {
                        "clé présente".to_string()
                    } else {
                        "clé absente".to_string()
                    },
                    if enrolled { C_GREEN } else { C_YELLOW },
                );
                if let Some(code) = status.last_success_http_status {
                    Self::metric_badge(
                        ui,
                        "Dernier HTTP",
                        code.to_string(),
                        Color32::from_rgb(92, 124, 250),
                    );
                }
                if let Some(ts_ms) = status.last_success_ms {
                    Self::metric_badge(
                        ui,
                        "Dernier succès",
                        fmt::ago_ms(state.vm.now_ms.saturating_sub(ts_ms)),
                        C_MUTED,
                    );
                }
                if let Some(ts_ms) = status.last_attempt_ms {
                    Self::metric_badge(
                        ui,
                        "Dernière tentative",
                        fmt::ago_ms(state.vm.now_ms.saturating_sub(ts_ms)),
                        C_MUTED,
                    );
                }
                if let Some(ts_ms) = status.last_registration_ms {
                    Self::metric_badge(
                        ui,
                        "Clé reçue",
                        fmt::ago_ms(state.vm.now_ms.saturating_sub(ts_ms)),
                        C_CYAN,
                    );
                }
            });

            if let Some(target) = &state.vm.remote_supervisor_status.last_target_url {
                ui.label(
                    RichText::new(format!("Cible  {target}"))
                    .size(10.0)
                    .color(C_MUTED),
                );
            }
            if let Some(machine_id) = &state.vm.remote_supervisor_status.last_registered_machine_id
            {
                ui.label(
                    RichText::new(format!("Machine enrôlée  {machine_id}"))
                        .size(10.0)
                        .color(C_CYAN),
                );
            }
            if let Some(ingest_url) = &state.vm.remote_supervisor_status.last_issued_ingest_url {
                ui.label(
                    RichText::new(format!("URL ingest  {ingest_url}"))
                        .size(10.0)
                        .color(C_MUTED),
                );
            }
            if let Some(reused) = state.vm.remote_supervisor_status.last_registration_reused_key {
                ui.label(
                    RichText::new(if reused {
                        "Le serveur a renvoyé la clé API existante pour cette machine."
                    } else {
                        "Le serveur a délivré une nouvelle clé API pour cette machine."
                    })
                    .size(10.0)
                    .color(C_MUTED),
                );
            }
            if let Some(err_msg) = &state.vm.remote_supervisor_status.last_error {
                ui.label(
                    RichText::new(format!("Erreur  {err_msg}"))
                        .size(10.5)
                        .color(C_RED),
                );
            }
            if let Some(ts_ms) = state.vm.remote_supervisor_status.last_error_ms {
                ui.label(
                    RichText::new(format!(
                        "Dernière erreur  {}",
                        fmt::ago_ms(state.vm.now_ms.saturating_sub(ts_ms))
                    ))
                    .size(10.0)
                    .color(C_MUTED),
                );
            }

            ui.add_space(8.0);
            ui.checkbox(
                &mut state.vm.remote_supervisor_config.enabled,
                "Activer la supervision distante",
            );
            ui.label(
                RichText::new(
                    "Flux attendu: renseigner l'URL du supervisor et le token d'enrôlement, puis cliquer sur “Demander la clé API”. Le serveur répond avec machine_id + api_key + ingest_url.",
                )
                .size(10.0)
                .color(C_MUTED),
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new("URL supervisor").size(11.0).color(C_MUTED));
                ui.text_edit_singleline(&mut state.vm.remote_supervisor_config.server_url);
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Token d'enrôlement").size(11.0).color(C_MUTED));
                ui.add(
                    egui::TextEdit::singleline(&mut state.vm.remote_supervisor_config.enroll_token)
                        .password(true),
                );
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Clé API").size(11.0).color(C_MUTED));
                ui.add(
                    egui::TextEdit::singleline(&mut state.vm.remote_supervisor_config.api_key)
                        .password(true),
                );
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Machine ID").size(11.0).color(C_MUTED));
                ui.text_edit_singleline(&mut state.vm.remote_supervisor_config.machine_id);
            });
            ui.add(
                egui::Slider::new(
                    &mut state.vm.remote_supervisor_config.push_interval_s,
                    1..=60,
                )
                .text("Intervalle push (s)"),
            );
            ui.label(
                RichText::new(
                    "Accepte une URL de base (ex. http://supervisor:8787) ou directement /api/ingest. Si URL + token sont présents, le bouton d’enrôlement demande la clé API au serveur via POST /api/register.",
                ).size(10.0).color(C_MUTED),
            );
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let can_request_api_key = !state.vm.remote_supervisor_config.server_url.trim().is_empty();
                let request_button = ui.add_enabled(
                    can_request_api_key,
                    egui::Button::new("Demander la clé API"),
                );
                if request_button.clicked() {
                    match state.register_remote_supervisor() {
                        Ok(msg) => *info = Some(msg),
                        Err(err) => *error = Some(err),
                    }
                }
                if ui.button("Tester la connexion").clicked() {
                    match state.test_remote_supervisor_connection() {
                        Ok(msg) => *info = Some(msg),
                        Err(err) => *error = Some(err),
                    }
                }
                if ui.button("Enregistrer").clicked() {
                    match state.save_remote_supervisor_config() {
                        Ok(()) => *info = Some("Config superviseur enregistrée".to_string()),
                        Err(err) => *error = Some(err),
                    }
                }
                if ui.button("Envoyer maintenant").clicked() {
                    match state.push_remote_supervisor_now() {
                        Ok(()) => *info = Some("Push superviseur lancé".to_string()),
                        Err(err) => *error = Some(err),
                    }
                }
            });
        });
    }

    // ── Processes panel ────────────────────────────────────────────────────────

    fn processes_panel(ui: &mut egui::Ui, state: &LiteState) {
        let summary = &state.vm.process_report.summary;
        Self::panel_card(ui, |ui| {
            Self::section_title(
                ui,
                "Processus observés",
                "Actifs en ce moment — groupés par application, puis détail.",
            );

            if summary.bridge_python_count > 1 {
                egui::Frame::new()
                    .fill(C_YELLOW.gamma_multiply(0.08))
                    .stroke(Stroke::new(1.0, C_YELLOW.gamma_multiply(0.4)))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.label(RichText::new(format!(
                            "⚠ {} processus Python bridge actifs — accumulation probable. Arrêter et redémarrer le bridge.",
                            summary.bridge_python_count
                        )).size(11.0).color(C_YELLOW));
                    });
                ui.add_space(6.0);
            }
            if summary.memory_compression_active {
                ui.label(
                    RichText::new("Memory Compression actif (SoulRAM opérationnel)")
                        .size(10.5)
                        .color(C_GREEN),
                );
            }

            ui.label(
                RichText::new(format!(
                    "{} processus  ·  SoulKernel {:.1}% / {}  ·  UI {:.1}% / {}",
                    summary.process_count,
                    summary.self_cpu_usage_pct,
                    fmt::mib_from_kb(summary.self_memory_kb),
                    summary.webview_cpu_usage_pct,
                    fmt::mib_from_kb(summary.webview_memory_kb),
                ))
                .size(10.5)
                .color(C_MUTED),
            );
            ui.add_space(6.0);

            // Notable process groups
            let notable_groups: Vec<_> = state
                .vm
                .process_report
                .groups
                .iter()
                .filter(|g| g.instance_count > 1 || g.total_cpu_pct >= 0.5)
                .take(10)
                .collect();
            if !notable_groups.is_empty() {
                Self::eyebrow(ui, "Groupes");
                ui.add_space(4.0);
                for g in &notable_groups {
                    Self::section_card(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&g.name).size(12.0).strong());
                            if g.instance_count > 1 {
                                egui::Frame::new()
                                    .fill(C_YELLOW.gamma_multiply(0.2))
                                    .corner_radius(999.0)
                                    .inner_margin(egui::Margin::symmetric(6, 2))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(format!("×{}", g.instance_count))
                                                .size(10.0)
                                                .color(C_YELLOW),
                                        );
                                    });
                            }
                            ui.label(
                                RichText::new(format!("{:.1}%", g.total_cpu_pct))
                                    .size(11.0)
                                    .color(C_TEXT),
                            );
                            ui.label(
                                RichText::new(fmt::mib_from_kb(g.total_memory_kb))
                                    .size(11.0)
                                    .color(C_MUTED),
                            );
                        });
                    });
                    ui.add_space(4.0);
                }
                ui.add_space(4.0);
            }

            // Individual process list
            Self::eyebrow(ui, "Processus");
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .id_salt("processes_scroll")
                .max_height(220.0)
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
                                Some(ProcessClass::SystemKernel) => C_MUTED,
                                Some(ProcessClass::OverheadCritical) => C_YELLOW,
                                Some(ProcessClass::OverheadSoft) => Color32::from_rgb(170, 130, 60),
                                _ if proc_.is_self_process || proc_.is_embedded_webview => C_MUTED,
                                _ => C_TEXT,
                            };
                            ui.label(
                                RichText::new(&proc_.name)
                                    .size(12.0)
                                    .strong()
                                    .color(name_color),
                            );
                            ui.label(
                                RichText::new(format!("#{}", proc_.pid))
                                    .size(9.5)
                                    .color(C_MUTED),
                            );
                            ui.label(RichText::new(fmt::pct(proc_.cpu_usage_pct)).size(11.0));
                            ui.label(
                                RichText::new(fmt::mib_from_kb(proc_.memory_kb))
                                    .size(11.0)
                                    .color(C_MUTED),
                            );
                            if proc_.disk_read_bytes > 0 || proc_.disk_written_bytes > 0 {
                                ui.label(
                                    RichText::new(fmt::io_pair(
                                        proc_.disk_read_bytes,
                                        proc_.disk_written_bytes,
                                    ))
                                    .size(10.0)
                                    .color(C_MUTED),
                                );
                            }
                            ui.label(
                                RichText::new(fmt::runtime_short(proc_.run_time_s))
                                    .size(9.5)
                                    .color(C_MUTED),
                            );
                            match class {
                                Some(ProcessClass::OverheadCritical) => {
                                    ui.label(
                                        RichText::new("overhead-sec").size(9.5).color(C_YELLOW),
                                    );
                                }
                                Some(ProcessClass::OverheadSoft) => {
                                    ui.label(
                                        RichText::new("overhead")
                                            .size(9.5)
                                            .color(Color32::from_rgb(170, 130, 60)),
                                    );
                                }
                                Some(ProcessClass::SystemKernel) => {
                                    ui.label(RichText::new("sys").size(9.5).color(C_MUTED));
                                }
                                _ if proc_.is_self_process => {
                                    ui.label(RichText::new("SoulKernel").size(9.5).color(C_MUTED));
                                }
                                _ if proc_.is_embedded_webview => {
                                    ui.label(RichText::new("UI").size(9.5).color(C_MUTED));
                                }
                                _ => {}
                            }
                        });
                    }
                });
        });
    }

    // ── Home dashboard ─────────────────────────────────────────────────────────

    fn home_dashboard(
        ui: &mut egui::Ui,
        state: &mut LiteState,
        error: &mut Option<String>,
        info: &mut Option<String>,
    ) {
        let wide = ui.available_width() >= 1200.0;
        if wide {
            ui.columns(2, |columns| {
                Self::host_impact_panel(&mut columns[0], &state.vm);
                columns[0].add_space(8.0);
                Self::material_overview_panel(&mut columns[0], &state.vm);
                columns[0].add_space(8.0);
                Self::decision_panel(&mut columns[0], &state.vm);
                columns[0].add_space(8.0);
                Self::telemetry_panel(&mut columns[0], &state.vm);

                Self::gains_panel(&mut columns[1], &state.vm);
                columns[1].add_space(8.0);
                Self::pilotage_panel(&mut columns[1], state, error, info);
                columns[1].add_space(8.0);
                Self::remote_supervisor_panel(&mut columns[1], state, error, info);
            });
        } else {
            Self::host_impact_panel(ui, &state.vm);
            ui.add_space(8.0);
            Self::material_overview_panel(ui, &state.vm);
            ui.add_space(8.0);
            Self::decision_panel(ui, &state.vm);
            ui.add_space(8.0);
            Self::gains_panel(ui, &state.vm);
            ui.add_space(8.0);
            Self::telemetry_panel(ui, &state.vm);
        }
    }
}

// ── eframe::App ────────────────────────────────────────────────────────────────

impl eframe::App for LiteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply theme once (idempotent in egui, no observable cost per frame)
        if !self.visuals_configured {
            Self::configure_visuals(ctx);
            self.visuals_configured = true;
        }

        let Some(state) = self.state.as_mut() else {
            ctx.request_repaint_after(std::time::Duration::from_millis(1000));
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new("SoulKernel Lite")
                            .size(24.0)
                            .strong()
                            .color(C_CYAN),
                    );
                    if let Some(err) = &self.error {
                        ui.add_space(12.0);
                        ui.label(RichText::new(err).size(13.0).color(C_RED));
                    }
                });
            });
            return;
        };

        let repaint_ms =
            if state.vm.show_hud || state.is_refresh_in_flight() || state.is_action_in_flight() {
                500
            } else {
                2000
            };
        ctx.request_repaint_after(std::time::Duration::from_millis(repaint_ms));

        if let Err(err) = state.refresh_if_needed() {
            self.error = Some(err);
        }

        if matches!(self.info.as_deref(), Some("Dôme activé")) && !state.vm.dome_active {
            self.info = None;
        }
        if matches!(self.info.as_deref(), Some("Dôme annulé")) && state.vm.dome_active {
            self.info = None;
        }

        // ── Top panel ──────────────────────────────────────────────────────────
        let state_vm = &state.vm;
        egui::TopBottomPanel::top("top")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(6, 16, 25, 240))
                    .stroke(Stroke::new(1.0, C_BORDER))
                    .inner_margin(egui::Margin {
                        left: 16,
                        right: 16,
                        top: 10,
                        bottom: 10,
                    }),
            )
            .show(ctx, |ui| {
                Self::top_bar(ui, state_vm);
                ui.add_space(6.0);
                Self::metrics_strip(ui, state_vm);

                // Feedback strip
                if self.info.is_some() || self.error.is_some() {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if let Some(info) = &self.info {
                            egui::Frame::new()
                                .fill(C_GREEN.gamma_multiply(0.12))
                                .stroke(Stroke::new(1.0, C_GREEN.gamma_multiply(0.4)))
                                .corner_radius(6.0)
                                .inner_margin(egui::Margin::symmetric(10, 4))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(info).size(11.0).color(C_GREEN));
                                });
                        }
                        if let Some(err) = &self.error {
                            egui::Frame::new()
                                .fill(C_RED.gamma_multiply(0.12))
                                .stroke(Stroke::new(1.0, C_RED.gamma_multiply(0.4)))
                                .corner_radius(6.0)
                                .inner_margin(egui::Margin::symmetric(10, 4))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(err).size(11.0).color(C_RED));
                                });
                        }
                    });
                }
            });

        // ── Central panel ──────────────────────────────────────────────────────
        let active_tab = &mut self.active_tab;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(C_BG)
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("central_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        Self::dashboard_tabs(ui, active_tab);
                        ui.add_space(12.0);
                        match *active_tab {
                            DashboardTab::Home => {
                                Self::home_dashboard(ui, state, &mut self.error, &mut self.info);
                            }
                            DashboardTab::Actions => {
                                Self::pilotage_panel(ui, state, &mut self.error, &mut self.info);
                                ui.add_space(8.0);
                                Self::external_power_panel(
                                    ui,
                                    state,
                                    &mut self.error,
                                    &mut self.info,
                                );
                                ui.add_space(8.0);
                                Self::remote_supervisor_panel(
                                    ui,
                                    state,
                                    &mut self.error,
                                    &mut self.info,
                                );
                                ui.add_space(8.0);
                                Self::benchmark_panel(ui, state, &mut self.error, &mut self.info);
                                ui.add_space(8.0);
                                Self::hud_panel(ui, state);
                            }
                            DashboardTab::Processes => {
                                Self::processes_panel(ui, state);
                            }
                            DashboardTab::Energy => {
                                Self::telemetry_panel(ui, &state.vm);
                                ui.add_space(8.0);
                                Self::gains_panel(ui, &state.vm);
                            }
                            DashboardTab::Hardware => {
                                Self::material_overview_panel(ui, &state.vm);
                                ui.add_space(8.0);
                                Self::inventory_panel(ui, state);
                            }
                        }
                    });
            });

        if state.vm.show_hud {
            Self::hud_overlay(ctx, &state.vm);
        }
    }
}
