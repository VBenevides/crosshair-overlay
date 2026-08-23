use eframe::egui;

#[cfg(target_os = "linux")]
#[path = "wayland_layer.rs"]
mod wayland_layer;

#[cfg(target_os = "linux")]
pub use wayland_layer::WaylandOverlay;

#[cfg(target_os = "linux")]
pub enum OverlayBackend {
    Wayland(WaylandOverlay),
    X11 { fallback_error: Option<String> },
}

#[cfg(target_os = "linux")]
impl OverlayBackend {
    pub fn startup_message(&self) -> String {
        match self {
            Self::Wayland(_) => String::from("Wayland layer-shell overlay active"),
            Self::X11 {
                fallback_error: Some(error),
            } => format!("Using X11 fallback; Wayland unavailable: {error}"),
            Self::X11 {
                fallback_error: None,
            } => String::from("X11 overlay active"),
        }
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, PartialEq, Eq)]
enum BackendChoice {
    Wayland,
    X11,
}

#[cfg(target_os = "linux")]
fn choose_backend(
    wayland_present: bool,
    x11_present: bool,
    wayland_error: Option<&str>,
) -> Result<BackendChoice, String> {
    if wayland_present && wayland_error.is_none() {
        return Ok(BackendChoice::Wayland);
    }
    if x11_present {
        return Ok(BackendChoice::X11);
    }
    Err(wayland_error.map_or_else(
        || String::from("neither X11/XWayland nor Wayland is available"),
        |error| format!("Wayland failed and X11/XWayland is unavailable: {error}"),
    ))
}

#[cfg(target_os = "linux")]
pub fn select_backend() -> Result<OverlayBackend, String> {
    let wayland_present = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let wayland_error = if wayland_present {
        match WaylandOverlay::new() {
            Ok(overlay) => return Ok(OverlayBackend::Wayland(overlay)),
            Err(error) => Some(error),
        }
    } else {
        None
    };
    let x11_present = std::env::var_os("DISPLAY").is_some();
    if choose_backend(wayland_present, x11_present, wayland_error.as_deref())
        == Ok(BackendChoice::X11)
    {
        if let Some(error) = &wayland_error {
            eprintln!("Wayland overlay unavailable; forcing X11: {error}");
        }
        return Ok(OverlayBackend::X11 {
            fallback_error: wayland_error,
        });
    }

    Err(
        choose_backend(wayland_present, x11_present, wayland_error.as_deref())
            .expect_err("Wayland success returns before backend selection"),
    )
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn backend_selection_matrix() {
        assert_eq!(choose_backend(true, true, None), Ok(BackendChoice::Wayland));
        assert_eq!(choose_backend(false, true, None), Ok(BackendChoice::X11));
        assert_eq!(
            choose_backend(true, true, Some("layer-shell missing")),
            Ok(BackendChoice::X11)
        );
        assert!(choose_backend(false, false, None).is_err());
    }
}

pub fn overlay_viewport(
    _display_index: usize,
    size: [f32; 2],
    position: [f32; 2],
    visible: bool,
) -> egui::ViewportBuilder {
    let builder = egui::ViewportBuilder::default().with_title("Crosshair Overlay");

    #[cfg(target_os = "linux")]
    let builder = builder
        .with_inner_size(size)
        .with_position(position)
        .with_window_type(egui::X11WindowType::Utility)
        .with_override_redirect(true);

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
