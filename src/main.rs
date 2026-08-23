mod app;
mod platform;

fn main() -> eframe::Result {
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([420.0, 620.0])
            .with_min_inner_size([360.0, 480.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    #[cfg(target_os = "linux")]
    {
        use winit::platform::x11::EventLoopBuilderExtX11;
        options.event_loop_builder = Some(Box::new(|event_loop| {
            event_loop.with_x11();
        }));
    }

    eframe::run_native(
        "Crosshair",
        options,
        Box::new(|creation_context| Ok(Box::new(app::CrosshairApp::new(creation_context)))),
    )
}
