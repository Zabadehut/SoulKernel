mod app;
mod export;
mod fmt;
mod state;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "SoulKernel Lite",
        options,
        Box::new(|_cc| Ok(Box::<app::LiteApp>::default())),
    )
}
