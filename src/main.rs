mod app;
mod platform;

fn main() -> eframe::Result {
    #[cfg(target_os = "linux")]
    // The MVP targets X11/XWayland; select it when both desktop variables exist.
    unsafe {
        std::env::set_var("WINIT_UNIX_BACKEND", "x11")
    };

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([420.0, 620.0])
            .with_min_inner_size([360.0, 480.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Crosshair",
        options,
        Box::new(|creation_context| Ok(Box::new(app::CrosshairApp::new(creation_context)))),
    )
}
