mod app;
mod platform;

fn main() -> eframe::Result {
    let title = format!("Crosshair - v{}", include_str!("../VERSION").trim());

    #[cfg(target_os = "linux")]
    let backend = platform::select_backend()
        .map_err(|error| eframe::Error::AppCreation(Box::new(std::io::Error::other(error))))?;

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([420.0, 620.0])
            .with_min_inner_size([360.0, 480.0])
            .with_transparent(true),
        renderer: eframe::Renderer::Glow,
        #[cfg(target_os = "linux")]
        event_loop_builder: if matches!(&backend, platform::OverlayBackend::X11 { .. }) {
            Some(Box::new(|builder| {
                use winit::platform::x11::EventLoopBuilderExtX11;
                builder.with_x11();
            }))
        } else {
            None
        },
        ..Default::default()
    };

    eframe::run_native(
        &title,
        options,
        Box::new(move |creation_context| {
            #[cfg(target_os = "linux")]
            return Ok(Box::new(app::CrosshairApp::new(creation_context, backend)));

            #[cfg(not(target_os = "linux"))]
            Ok(Box::new(app::CrosshairApp::new(creation_context)))
        }),
    )
}
