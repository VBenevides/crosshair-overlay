#[cfg(not(target_os = "windows"))]
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

    #[test]
    fn overlay_leaves_visibility_unset() {
        assert_eq!(
            overlay_viewport(0, [1920.0, 1080.0], [0.0, 0.0]).visible,
            None
        );
    }
}

#[cfg(target_os = "windows")]
mod windows_overlay {
    use crosshair::{CrosshairState, DisplaySize};
    use std::{ffi::c_void, mem::size_of, ptr, slice};

    type Handle = *mut c_void;
    type WndProc = unsafe extern "system" fn(Handle, u32, usize, isize) -> isize;

    const CLASS_NAME: &str = "CrosshairNativeOverlay";
    const WS_POPUP: u32 = 0x8000_0000;
    const WS_EX_LAYERED: u32 = 0x0008_0000;
    const WS_EX_TRANSPARENT: u32 = 0x0000_0020;
    const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
    const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
    const CS_HREDRAW: u32 = 0x0002;
    const CS_VREDRAW: u32 = 0x0001;
    const WM_NCHITTEST: u32 = 0x0084;
    const WM_MOUSEACTIVATE: u32 = 0x0021;
    const WM_ERASEBKGND: u32 = 0x0014;
    const MA_NOACTIVATE: isize = 3;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const SW_HIDE: i32 = 0;
    const SW_SHOWNOACTIVATE: i32 = 4;
    const DIB_RGB_COLORS: u32 = 0;
    const BI_RGB: u32 = 0;
    const ULW_ALPHA: u32 = 0x0002;
    const AC_SRC_OVER: u8 = 0;
    const AC_SRC_ALPHA: u8 = 1;

    #[repr(C)]
    struct WndClassExW {
        cb_size: u32,
        style: u32,
        wnd_proc: WndProc,
        cb_cls_extra: i32,
        cb_wnd_extra: i32,
        instance: Handle,
        icon: Handle,
        cursor: Handle,
        background: Handle,
        menu_name: *const u16,
        class_name: *const u16,
        icon_small: Handle,
    }

    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bit_count: u16,
        compression: u32,
        image_size: u32,
        x_pixels_per_meter: i32,
        y_pixels_per_meter: i32,
        colors_used: u32,
        colors_important: u32,
    }

    #[repr(C)]
    struct BitmapInfo {
        header: BitmapInfoHeader,
        colors: [u32; 1],
    }

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct Size {
        width: i32,
        height: i32,
    }

    #[repr(C)]
    struct BlendFunction {
        operation: u8,
        flags: u8,
        source_constant_alpha: u8,
        alpha_format: u8,
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        #[link_name = "CreateCompatibleDC"]
        fn create_compatible_dc(dc: Handle) -> Handle;
        #[link_name = "CreateDIBSection"]
        fn create_dib_section(
            dc: Handle,
            info: *const BitmapInfo,
            usage: u32,
            bits: *mut *mut c_void,
            section: Handle,
            offset: u32,
        ) -> Handle;
        #[link_name = "DeleteDC"]
        fn delete_dc(dc: Handle) -> i32;
        #[link_name = "DeleteObject"]
        fn delete_object(object: Handle) -> i32;
        #[link_name = "SelectObject"]
        fn select_object(dc: Handle, object: Handle) -> Handle;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetModuleHandleW"]
        fn get_module_handle_w(module_name: *const u16) -> Handle;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        #[link_name = "CreateWindowExW"]
        fn create_window_ex_w(
            extended_style: u32,
            class_name: *const u16,
            window_name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: Handle,
            menu: Handle,
            instance: Handle,
            param: *mut c_void,
        ) -> Handle;
        #[link_name = "DefWindowProcW"]
        fn def_window_proc_w(hwnd: Handle, message: u32, wparam: usize, lparam: isize) -> isize;
        #[link_name = "DestroyWindow"]
        fn destroy_window(hwnd: Handle) -> i32;
        #[link_name = "GetDC"]
        fn get_dc(hwnd: Handle) -> Handle;
        #[link_name = "RegisterClassExW"]
        fn register_class_ex_w(class: *const WndClassExW) -> u16;
        #[link_name = "ReleaseDC"]
        fn release_dc(hwnd: Handle, dc: Handle) -> i32;
        #[link_name = "SetWindowPos"]
        fn set_window_pos(
            hwnd: Handle,
            insert_after: Handle,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> i32;
        #[link_name = "ShowWindow"]
        fn show_window(hwnd: Handle, command: i32) -> i32;
        #[link_name = "UpdateLayeredWindow"]
        fn update_layered_window(
            hwnd: Handle,
            destination_dc: Handle,
            destination_point: *const Point,
            size: *const Size,
            source_dc: Handle,
            source_point: *const Point,
            color_key: u32,
            blend: *const BlendFunction,
            flags: u32,
        ) -> i32;
    }

    unsafe extern "system" fn window_proc(
        hwnd: Handle,
        message: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize {
        match message {
            WM_NCHITTEST => -1,
            WM_MOUSEACTIVATE => MA_NOACTIVATE,
            WM_ERASEBKGND => 1,
            _ => unsafe { def_window_proc_w(hwnd, message, wparam, lparam) },
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub struct WindowsOverlay {
        hwnd: Handle,
        dc: Handle,
        bitmap: Handle,
        old_bitmap: Handle,
        pixels: *mut u8,
        width: i32,
        height: i32,
    }

    impl WindowsOverlay {
        pub fn new() -> Self {
            let class_name = wide(CLASS_NAME);
            let title = wide("Crosshair Overlay");
            let hwnd = unsafe {
                let instance = get_module_handle_w(ptr::null());
                let class = WndClassExW {
                    cb_size: size_of::<WndClassExW>() as u32,
                    style: CS_HREDRAW | CS_VREDRAW,
                    wnd_proc: window_proc,
                    cb_cls_extra: 0,
                    cb_wnd_extra: 0,
                    instance,
                    icon: ptr::null_mut(),
                    cursor: ptr::null_mut(),
                    background: ptr::null_mut(),
                    menu_name: ptr::null(),
                    class_name: class_name.as_ptr(),
                    icon_small: ptr::null_mut(),
                };
                register_class_ex_w(&class);
                create_window_ex_w(
                    WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    class_name.as_ptr(),
                    title.as_ptr(),
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    instance,
                    ptr::null_mut(),
                )
            };
            Self {
                hwnd,
                dc: ptr::null_mut(),
                bitmap: ptr::null_mut(),
                old_bitmap: ptr::null_mut(),
                pixels: ptr::null_mut(),
                width: 0,
                height: 0,
            }
        }

        pub fn update(
            &mut self,
            display_position: (i32, i32),
            display_size: DisplaySize,
            state: &CrosshairState,
        ) {
            if self.hwnd.is_null() {
                return;
            }
            if !state.visible {
                unsafe { show_window(self.hwnd, SW_HIDE) };
                return;
            }

            let extent = crosshair_extent(state);
            let width = (extent * 2.0).ceil().max(1.0) as i32;
            let height = width;
            if !self.resize(width, height) {
                return;
            }
            self.paint(state);

            let (center_x, center_y) = display_size.center();
            let x = i64::from(display_position.0) + i64::from(center_x) + i64::from(state.offset_x)
                - i64::from(width / 2);
            let y = i64::from(display_position.1) + i64::from(center_y) + i64::from(state.offset_y)
                - i64::from(height / 2);
            let destination = Point {
                x: x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
                y: y.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            };
            let size = Size { width, height };
            let source = Point { x: 0, y: 0 };
            let blend = BlendFunction {
                operation: AC_SRC_OVER,
                flags: 0,
                source_constant_alpha: 255,
                alpha_format: AC_SRC_ALPHA,
            };
            unsafe {
                let screen_dc = get_dc(ptr::null_mut());
                if !screen_dc.is_null() {
                    update_layered_window(
                        self.hwnd,
                        screen_dc,
                        &destination,
                        &size,
                        self.dc,
                        &source,
                        0,
                        &blend,
                        ULW_ALPHA,
                    );
                    release_dc(ptr::null_mut(), screen_dc);
                }
                set_window_pos(
                    self.hwnd,
                    (-1isize) as Handle,
                    destination.x,
                    destination.y,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
                show_window(self.hwnd, SW_SHOWNOACTIVATE);
            }
        }

        pub fn hide(&self) {
            if !self.hwnd.is_null() {
                unsafe { show_window(self.hwnd, SW_HIDE) };
            }
        }

        fn resize(&mut self, width: i32, height: i32) -> bool {
            if self.width == width && self.height == height && !self.pixels.is_null() {
                return true;
            }
            unsafe {
                self.release_bitmap();
                let dc = create_compatible_dc(ptr::null_mut());
                if dc.is_null() {
                    return false;
                }
                let info = BitmapInfo {
                    header: BitmapInfoHeader {
                        size: size_of::<BitmapInfoHeader>() as u32,
                        width,
                        height: -height,
                        planes: 1,
                        bit_count: 32,
                        compression: BI_RGB,
                        image_size: 0,
                        x_pixels_per_meter: 0,
                        y_pixels_per_meter: 0,
                        colors_used: 0,
                        colors_important: 0,
                    },
                    colors: [0],
                };
                let mut bits = ptr::null_mut();
                let bitmap =
                    create_dib_section(dc, &info, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0);
                if bitmap.is_null() || bits.is_null() {
                    delete_dc(dc);
                    return false;
                }
                let old_bitmap = select_object(dc, bitmap);
                self.dc = dc;
                self.bitmap = bitmap;
                self.old_bitmap = old_bitmap;
                self.pixels = bits.cast();
                self.width = width;
                self.height = height;
                true
            }
        }

        fn release_bitmap(&mut self) {
            unsafe {
                if !self.dc.is_null() && !self.old_bitmap.is_null() {
                    select_object(self.dc, self.old_bitmap);
                }
                if !self.bitmap.is_null() {
                    delete_object(self.bitmap);
                }
                if !self.dc.is_null() {
                    delete_dc(self.dc);
                }
            }
            self.dc = ptr::null_mut();
            self.bitmap = ptr::null_mut();
            self.old_bitmap = ptr::null_mut();
            self.pixels = ptr::null_mut();
            self.width = 0;
            self.height = 0;
        }

        fn paint(&mut self, state: &CrosshairState) {
            let width = self.width as usize;
            let height = self.height as usize;
            let pixels = unsafe { slice::from_raw_parts_mut(self.pixels, width * height * 4) };
            pixels.fill(0);
            let center = self.width as f32 / 2.0;
            for y in 0..height {
                for x in 0..width {
                    let dx = x as f32 + 0.5 - center;
                    let dy = y as f32 + 0.5 - center;
                    let mut pixel = [0_u8; 4];
                    if state.draw_outline && covers(dx, dy, state, state.outline_thickness) {
                        blend(&mut pixel, [0, 0, 0], state.alpha);
                    }
                    if covers(dx, dy, state, 0.0) {
                        blend(
                            &mut pixel,
                            [state.color.blue, state.color.green, state.color.red],
                            state.alpha,
                        );
                    }
                    let offset = (y * width + x) * 4;
                    pixels[offset..offset + 4].copy_from_slice(&pixel);
                }
            }
        }
    }

    impl Drop for WindowsOverlay {
        fn drop(&mut self) {
            self.release_bitmap();
            if !self.hwnd.is_null() {
                unsafe { destroy_window(self.hwnd) };
            }
        }
    }

    fn crosshair_extent(state: &CrosshairState) -> f32 {
        let outline = if state.draw_outline {
            state.outline_thickness.max(0.0)
        } else {
            0.0
        };
        let arm = if state.size > 0.0 {
            state.gap.abs().max((state.gap + state.size).abs())
                + state.thickness.max(0.0) / 2.0
                + outline
        } else {
            0.0
        };
        let dot = if state.dot {
            state.thickness.max(1.0) / 2.0 + outline
        } else {
            0.0
        };
        arm.max(dot) + 1.0
    }

    fn covers(dx: f32, dy: f32, state: &CrosshairState, extra_width: f32) -> bool {
        let half_width = state.thickness.max(0.0) / 2.0 + extra_width;
        let arms = state.size > 0.0
            && ((dy.abs() <= half_width && between(dx, state.gap, state.gap + state.size))
                || (dy.abs() <= half_width && between(dx, -state.gap - state.size, -state.gap))
                || (dx.abs() <= half_width && between(dy, state.gap, state.gap + state.size))
                || (dx.abs() <= half_width && between(dy, -state.gap - state.size, -state.gap)));
        let dot = state.dot
            && dx * dx + dy * dy <= (state.thickness.max(1.0) / 2.0 + extra_width).powi(2);
        arms || dot
    }

    fn between(value: f32, start: f32, end: f32) -> bool {
        value >= start.min(end) && value <= start.max(end)
    }

    fn blend(pixel: &mut [u8; 4], color: [u8; 3], alpha: u8) {
        let source_alpha = u32::from(alpha);
        let inverse_alpha = 255 - source_alpha;
        for (index, channel) in color.into_iter().enumerate() {
            pixel[index] = ((u32::from(channel) * source_alpha
                + u32::from(pixel[index]) * inverse_alpha
                + 127)
                / 255) as u8;
        }
        pixel[3] = (source_alpha + u32::from(pixel[3]) * inverse_alpha / 255) as u8;
    }
}

#[cfg(target_os = "windows")]
pub use windows_overlay::WindowsOverlay;

#[cfg(not(target_os = "windows"))]
pub fn overlay_viewport(
    _display_index: usize,
    size: [f32; 2],
    position: [f32; 2],
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
