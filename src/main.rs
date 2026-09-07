mod app;
mod config;
mod deleted;
mod fileops;
mod gamepad;
mod mounts;
mod places;

fn main() -> eframe::Result<()> {
    let config = config::Config::load();

    let viewport = if config.fullscreen {
        egui::ViewportBuilder::default().with_fullscreen(true)
    } else {
        egui::ViewportBuilder::default()
            .with_inner_size([config.width as f32, config.height as f32])
    };

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "BrowDeck",
        options,
        Box::new(|cc| Ok(Box::new(app::BrowDeckApp::new(cc)))),
    )
}
