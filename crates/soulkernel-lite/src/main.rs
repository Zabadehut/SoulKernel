use eframe::egui;
use soulkernel_core::metrics::{self, ResourceState};
use std::time::{Duration, Instant};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "SoulKernel Lite",
        options,
        Box::new(|_cc| Ok(Box::<LiteApp>::default())),
    )
}

struct LiteApp {
    last_refresh: Instant,
    sample: Option<ResourceState>,
    last_error: Option<String>,
}

impl Default for LiteApp {
    fn default() -> Self {
        Self {
            last_refresh: Instant::now() - Duration::from_secs(10),
            sample: None,
            last_error: None,
        }
    }
}

impl LiteApp {
    fn refresh_if_needed(&mut self) {
        if self.last_refresh.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.last_refresh = Instant::now();
        match metrics::collect() {
            Ok(sample) => {
                self.sample = Some(sample);
                self.last_error = None;
            }
            Err(err) => {
                self.last_error = Some(err.to_string());
            }
        }
    }
}

impl eframe::App for LiteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh_if_needed();
        ctx.request_repaint_after(Duration::from_millis(400));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("SoulKernel Lite");
            ui.label("Mode natif quotidien, branché sur le core Rust.");
            ui.separator();

            if let Some(err) = &self.last_error {
                ui.colored_label(egui::Color32::RED, format!("Erreur métriques: {err}"));
                ui.separator();
            }

            if let Some(sample) = &self.sample {
                ui.horizontal(|ui| {
                    ui.label(format!("CPU {:.1} %", sample.raw.cpu_pct));
                    ui.separator();
                    ui.label(format!("RAM {:.1} %", sample.mem * 100.0));
                    ui.separator();
                    ui.label(format!(
                        "GPU {}",
                        sample
                            .raw
                            .gpu_pct
                            .map(|v| format!("{v:.1} %"))
                            .unwrap_or_else(|| "N/A".to_string())
                    ));
                    ui.separator();
                    ui.label(format!(
                        "Power {}",
                        sample
                            .raw
                            .power_watts
                            .map(|v| format!("{v:.1} W"))
                            .unwrap_or_else(|| "N/A".to_string())
                    ));
                });
                ui.separator();
                ui.label(format!(
                    "I/O lecture {} MB/s · écriture {} MB/s",
                    sample.raw.io_read_mb_s.unwrap_or(0.0),
                    sample.raw.io_write_mb_s.unwrap_or(0.0)
                ));
                ui.label(format!(
                    "SoulRAM backend: {}",
                    soulkernel_core::platform::soulram_backend_info().backend
                ));
                ui.label(format!(
                    "WebView host: {:.1}% CPU · {:.0} MiB",
                    sample.raw.webview_host_cpu_sum.unwrap_or(0.0),
                    sample.raw.webview_host_mem_mb.unwrap_or(0)
                ));
                ui.separator();
                ui.label("Surface lite initiale:");
                ui.label("- source énergie");
                ui.label("- CPU/RAM/GPU/I/O");
                ui.label("- SoulRAM backend");
                ui.label("- overhead WebView host");
                ui.label("- les actions dôme / exports seront branchées dans l’étape suivante");
            } else {
                ui.label("En attente du premier échantillon…");
            }
        });
    }
}
