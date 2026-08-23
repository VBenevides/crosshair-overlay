mod app;
mod platform;

fn main() -> eframe::Result {
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
