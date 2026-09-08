mod app;
mod config;
mod deleted;
mod fileicons;
mod fileinfo;
mod fileops;
mod fonts;
mod gamepad;
mod iprolaunch;
mod mounts;
mod permissions;
mod places;
mod preview;

fn main() -> eframe::Result<()> {
    let config = config::Config::load();

    // `with_inner_size` is set unconditionally, even when requesting
    // fullscreen: winit's X11 backend (what gamescope's nested Xwayland
    // uses) falls back to a hardcoded 800x600 the instant no inner size is
    // given, *before* the fullscreen request gets negotiated — on a cold
    // launch under gamescope that race loses, leaving the app stuck at
    // 800x600 while gamescope stretches it to fill the real display,
    // rendering blurry/small. Giving it a real size up front sidesteps the
    // fallback entirely; the fullscreen request still takes over on top of
    // it once negotiated.
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([config.width as f32, config.height as f32]);
    if config.fullscreen {
        viewport = viewport.with_fullscreen(true);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "BrowDeck",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::BrowDeckApp::new(
                cc,
                config.show_header,
                config.fullscreen,
                config.width as f32,
                config.height as f32,
            )))
        }),
    )
}
