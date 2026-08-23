use eframe::egui;

#[cfg(target_os = "linux")]
#[path = "wayland_layer.rs"]
mod wayland_layer;

#[cfg(target_os = "linux")]
pub use wayland_layer::WaylandOverlay;

pub fn availability() -> Result<(), &'static str> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return Err("neither X11/XWayland nor Wayland is available");
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    return Err("this build supports only Linux X11/XWayland and Windows");

    Ok(())
}

pub fn overlay_viewport(
    _display_index: usize,
    size: [f32; 2],
    position: [f32; 2],
    visible: bool,
) -> egui::ViewportBuilder {
    let builder = egui::ViewportBuilder::default().with_title("Crosshair Overlay");

    #[cfg(target_os = "linux")]
    let builder = if uses_wayland() {
        builder.with_monitor(_display_index).with_fullscreen(true)
    } else {
        builder
            .with_inner_size(size)
            .with_position(position)
            .with_window_type(egui::X11WindowType::Utility)
            .with_override_redirect(true)
    };

    #[cfg(not(target_os = "linux"))]
    let builder = builder.with_inner_size(size).with_position(position);

    let builder = builder
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

    builder
}

#[cfg(target_os = "linux")]
pub fn uses_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}
