#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod export;
mod fmt;
mod state;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SoulKernel Lite")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([800.0, 540.0]),
        ..Default::default()
    };
    eframe::run_native(
        "SoulKernel Lite",
        options,
        Box::new(|_cc| Ok(Box::<app::LiteApp>::default())),
    )
}
