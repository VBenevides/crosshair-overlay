use eframe::egui;

pub fn availability() -> Result<(), &'static str> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_none() {
        return Err("X11/XWayland is unavailable: DISPLAY is not set");
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    return Err("this build supports only Linux X11/XWayland and Windows");

    Ok(())
}

pub fn overlay_viewport(display_index: usize, visible: bool) -> egui::ViewportBuilder {
    let builder = egui::ViewportBuilder::default()
        .with_title("Crosshair Overlay")
        .with_monitor(display_index)
        .with_visible(visible)
        .with_transparent(true)
        .with_decorations(false)
        .with_resizable(false)
        .with_always_on_top()
        .with_mouse_passthrough(true)
        .with_active(false)
        .with_taskbar(false)
        .with_clamp_size_to_monitor_size(false);

    #[cfg(target_os = "windows")]
    let builder = builder
        .with_window_level(egui::WindowLevel::AlwaysOnTop)
        .with_active(false)
        .with_taskbar(false);

    #[cfg(not(target_os = "windows"))]
    let builder = builder;

    #[cfg(target_os = "linux")]
    let builder = builder
        .with_window_type(egui::X11WindowType::Utility)
        .with_override_redirect(false);

    builder
}
