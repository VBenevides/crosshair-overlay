use std::{
    num::NonZeroU32,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

#[cfg(test)]
use crosshair::Color;
use crosshair::CrosshairState;
use smithay_client_toolkit::reexports::client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_shm, wl_surface},
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{
        Shm, ShmHandler,
        slot::{Buffer, SlotPool},
    },
};

#[derive(Clone)]
struct SharedState {
    crosshair: CrosshairState,
    display_index: usize,
    output_name: Option<String>,
    generation: u64,
    error: Option<String>,
}

pub struct WaylandOverlay {
    state: Arc<Mutex<SharedState>>,
}

impl WaylandOverlay {
    pub fn new() -> Result<Self, String> {
        let state = Arc::new(Mutex::new(SharedState {
            crosshair: CrosshairState::default(),
            display_index: 0,
            output_name: None,
            generation: 0,
            error: None,
        }));
        let worker_state = Arc::clone(&state);
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);

        thread::spawn(move || {
            let result = run(worker_state, &ready_tx);
            if let Err(error) = result {
                eprintln!("Wayland layer overlay stopped: {error}");
            }
        });

        match ready_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => Ok(Self { state }),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(String::from("timed out starting the Wayland layer overlay")),
        }
    }

    pub fn update(
        &self,
        crosshair: CrosshairState,
        display_index: usize,
        output_name: Option<String>,
    ) {
        let mut state = self.state.lock().unwrap();
        if state.crosshair == crosshair
            && state.display_index == display_index
            && state.output_name == output_name
        {
            return;
        }
        let generation = state.generation.wrapping_add(1);
        let error = state.error.clone();
        *state = SharedState {
            crosshair,
            display_index,
            output_name,
            generation,
            error,
        };
    }

    pub fn error(&self) -> Option<String> {
        self.state.lock().unwrap().error.clone()
    }
}

fn set_error(state: &Arc<Mutex<SharedState>>, error: impl Into<String>) {
    state.lock().unwrap().error = Some(error.into());
}

fn clear_error(state: &Arc<Mutex<SharedState>>) {
    state.lock().unwrap().error = None;
}

fn run(
    state: Arc<Mutex<SharedState>>,
    ready_tx: &std::sync::mpsc::SyncSender<Result<(), String>>,
) -> Result<(), String> {
    let error_state = Arc::clone(&state);
    let result = run_inner(state, ready_tx);
    if let Err(error) = &result {
        set_error(&error_state, error.clone());
        let _ = ready_tx.send(Err(error.clone()));
    }
    result
}

fn run_inner(
    state: Arc<Mutex<SharedState>>,
    ready_tx: &std::sync::mpsc::SyncSender<Result<(), String>>,
) -> Result<(), String> {
    let connection = Connection::connect_to_env().map_err(|error| error.to_string())?;
    let (globals, mut event_queue) =
        registry_queue_init(&connection).map_err(|error| error.to_string())?;
    let queue_handle = event_queue.handle();
    let compositor = CompositorState::bind(&globals, &queue_handle)
        .map_err(|_| String::from("Wayland compositor is unavailable"))?;
    let layer_shell = LayerShell::bind(&globals, &queue_handle)
        .map_err(|_| String::from("wlr-layer-shell is unavailable in this compositor"))?;
    let shm = Shm::bind(&globals, &queue_handle)
        .map_err(|_| String::from("Wayland shared-memory buffers are unavailable"))?;

    let mut overlay = LayerState {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &queue_handle),
        compositor,
        layer_shell,
        shm,
        buffers: None,
        layer: None,
        width: 1,
        height: 1,
        first_configure: true,
        display_index: 0,
        rendered_generation: u64::MAX,
        bound_output: None,
        scale_factor: 1,
        unsupported_transform: false,
        initialized: false,
        state,
    };

    event_queue
        .roundtrip(&mut overlay)
        .map_err(|error| error.to_string())?;
    overlay.create_layer(&queue_handle)?;
    overlay.initialized = true;
    ready_tx
        .send(Ok(()))
        .map_err(|_| String::from("Wayland overlay initialization was cancelled"))?;

    loop {
        event_queue
            .blocking_dispatch(&mut overlay)
            .map_err(|error| error.to_string())?;
    }
}

struct LayerState {
    registry_state: RegistryState,
    output_state: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    buffers: Option<BufferSet>,
    layer: Option<LayerSurface>,
    width: u32,
    height: u32,
    first_configure: bool,
    display_index: usize,
    rendered_generation: u64,
    bound_output: Option<wl_output::WlOutput>,
    scale_factor: u32,
    unsupported_transform: bool,
    initialized: bool,
    state: Arc<Mutex<SharedState>>,
}

struct BufferSet {
    width: u32,
    height: u32,
    pool: SlotPool,
    buffers: Vec<Buffer>,
    front: Option<usize>,
}

impl BufferSet {
    fn new(width: u32, height: u32, shm: &Shm) -> Result<Self, String> {
        let stride = width
            .checked_mul(4)
            .and_then(|stride| i32::try_from(stride).ok())
            .ok_or_else(|| String::from("Wayland buffer stride is too large"))?;
        let bytes = (height as usize)
            .checked_mul(stride as usize)
            .ok_or_else(|| String::from("Wayland buffer size is too large"))?;
        let pool_size = bytes
            .checked_mul(2)
            .and_then(|size| size.checked_add(128))
            .ok_or_else(|| String::from("Wayland buffer pool size is too large"))?;
        let mut pool = SlotPool::new(pool_size, shm)
            .map_err(|error| format!("shared-memory pool creation failed: {error}"))?;
        let mut buffers = Vec::with_capacity(2);
        for _ in 0..2 {
            let slot = pool
                .new_slot(bytes)
                .map_err(|error| format!("shared-memory slot creation failed: {error}"))?;
            let buffer = pool
                .create_buffer_in(
                    &slot,
                    width as i32,
                    height as i32,
                    stride,
                    wl_shm::Format::Argb8888,
                )
                .map_err(|error| format!("buffer creation failed: {error}"))?;
            buffers.push(buffer);
        }
        Ok(Self {
            width,
            height,
            pool,
            buffers,
            front: None,
        })
    }

    fn next_index(&self) -> Option<usize> {
        (0..self.buffers.len()).find(|&index| {
            Some(index) != self.front && !self.buffers[index].slot().has_active_buffers()
        })
    }
}

impl LayerState {
    fn selected_output(
        &self,
        requested_name: Option<&str>,
        index: usize,
    ) -> Result<wl_output::WlOutput, String> {
        let outputs = self.output_state.outputs().collect::<Vec<_>>();
        if let Some(name) = requested_name {
            if let Some(output) = outputs.iter().find(|output| {
                self.output_state
                    .info(output)
                    .and_then(|info| info.name)
                    .as_deref()
                    == Some(name)
            }) {
                return Ok(output.clone());
            }
            return Err(format!("selected Wayland display is unavailable: {name}"));
        }
        outputs
            .into_iter()
            .nth(index)
            .ok_or_else(|| format!("selected Wayland display index is unavailable: {index}"))
    }

    fn output_size(&self) -> Option<(u32, u32)> {
        let output = self.bound_output.as_ref()?;
        let info = self.output_state.info(output)?;
        if let Some((width, height)) = info.logical_size {
            return Some((u32::try_from(width).ok()?, u32::try_from(height).ok()?));
        }
        let mode = info
            .modes
            .iter()
            .find(|mode| mode.current)
            .or_else(|| info.modes.iter().find(|mode| mode.preferred))?;
        let scale = i64::from(info.scale_factor.max(1));
        let width = (i64::from(mode.dimensions.0) + scale - 1) / scale;
        let height = (i64::from(mode.dimensions.1) + scale - 1) / scale;
        Some((u32::try_from(width).ok()?, u32::try_from(height).ok()?))
    }

    fn create_layer(&mut self, qh: &QueueHandle<Self>) -> Result<(), String> {
        let shared = self.state.lock().unwrap().clone();
        self.display_index = shared.display_index;
        let output = self.selected_output(shared.output_name.as_deref(), shared.display_index)?;
        let surface = self.compositor.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Overlay,
            Some("crosshair-overlay"),
            Some(&output),
        );
        layer.set_anchor(Anchor::TOP | Anchor::RIGHT | Anchor::BOTTOM | Anchor::LEFT);
        layer.set_exclusive_zone(0);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(0, 0);
        if let Ok(region) = Region::new(&self.compositor) {
            layer.set_input_region(Some(region.wl_region()));
        }
        layer.commit();
        self.layer = Some(layer);
        self.bound_output = Some(output);
        self.buffers = None;
        self.first_configure = true;
        self.rendered_generation = u64::MAX;
        Ok(())
    }

    fn reconcile_layer(&mut self, qh: &QueueHandle<Self>) -> bool {
        let shared = self.state.lock().unwrap().clone();
        let desired =
            match self.selected_output(shared.output_name.as_deref(), shared.display_index) {
                Ok(output) => output,
                Err(error) => {
                    self.layer = None;
                    self.bound_output = None;
                    set_error(&self.state, error);
                    return true;
                }
            };
        if self.layer.is_some() && self.bound_output.as_ref() == Some(&desired) {
            return false;
        }

        self.layer = None;
        self.bound_output = None;
        match self.create_layer(qh) {
            Ok(()) => clear_error(&self.state),
            Err(error) => set_error(&self.state, error),
        }
        true
    }

    fn draw(&mut self, qh: &QueueHandle<Self>) {
        if self.unsupported_transform {
            return;
        }
        let Some(layer) = self.layer.clone() else {
            return;
        };
        let Some(buffers) = self.buffers.as_ref() else {
            set_error(&self.state, "Wayland shared-memory buffers are unavailable");
            return;
        };
        let (width, height) = match scaled_dimensions(self.width, self.height, self.scale_factor) {
            Ok(dimensions) => dimensions,
            Err(error) => {
                set_error(&self.state, error);
                return;
            }
        };
        let shared = self.state.lock().unwrap().clone();
        let Some(index) = buffers.next_index() else {
            self.schedule_frame(qh);
            return;
        };
        let result = {
            let buffers = self.buffers.as_mut().expect("buffer set checked above");
            let buffer = &buffers.buffers[index];
            {
                let Some(canvas) = buffer.canvas(&mut buffers.pool) else {
                    return self.schedule_frame(qh);
                };
                canvas.fill(0);
                draw_crosshair(canvas, width, height, &shared.crosshair);
            }
            match buffer.attach_to(layer.wl_surface()) {
                Ok(()) => {
                    buffers.front = Some(index);
                    Ok(shared.generation)
                }
                Err(error) => Err(format!("buffer attach failed: {error}")),
            }
        };
        let generation = match result {
            Ok(generation) => generation,
            Err(error) => {
                eprintln!("Wayland overlay attach error: {error}");
                set_error(&self.state, error);
                self.schedule_frame(qh);
                return;
            }
        };
        layer
            .wl_surface()
            .damage_buffer(0, 0, width as i32, height as i32);
        layer.wl_surface().frame(qh, layer.wl_surface().clone());
        clear_error(&self.state);
        layer.commit();
        self.rendered_generation = generation;
    }

    fn schedule_frame(&self, qh: &QueueHandle<Self>) {
        let Some(layer) = self.layer.as_ref() else {
            return;
        };
        layer.wl_surface().frame(qh, layer.wl_surface().clone());
        layer.commit();
    }
}

impl CompositorHandler for LayerState {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        if factor < 1 {
            set_error(
                &self.state,
                format!("unsupported Wayland fractional scale: {factor}"),
            );
            return;
        }
        surface.set_buffer_scale(factor);
        let factor = factor as u32;
        if self.scale_factor != factor {
            self.scale_factor = factor;
            self.buffers = None;
            self.rendered_generation = u64::MAX;
        }
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        transform: wl_output::Transform,
    ) {
        self.unsupported_transform = transform != wl_output::Transform::Normal;
        if self.unsupported_transform {
            set_error(&self.state, "unsupported Wayland output transform");
        } else {
            self.buffers = None;
            self.rendered_generation = u64::MAX;
            clear_error(&self.state);
        }
    }

    fn frame(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if self
            .layer
            .as_ref()
            .is_some_and(|layer| layer.wl_surface() == surface)
        {
            if self.reconcile_layer(qh) {
                return;
            }
            let shared = self.state.lock().unwrap().clone();
            if shared.generation != self.rendered_generation {
                self.draw(qh);
            } else {
                self.schedule_frame(qh);
            }
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for LayerState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: wl_output::WlOutput) {
        if self.initialized {
            self.reconcile_layer(qh);
        }
    }
    fn update_output(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: wl_output::WlOutput) {
        if self.initialized {
            self.reconcile_layer(qh);
        }
    }
    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        if !self.initialized {
            return;
        }
        let selected_by_name = self.state.lock().unwrap().output_name.is_some();
        if !selected_by_name || self.bound_output.as_ref() == Some(&output) {
            self.layer = None;
            self.bound_output = None;
            set_error(&self.state, "selected Wayland display is unavailable");
        }
    }
}

impl LayerShellHandler for LayerState {
    fn closed(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: &LayerSurface) {
        self.layer = None;
        self.bound_output = None;
        if self.initialized {
            self.reconcile_layer(qh);
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        if self.layer.as_ref() != Some(layer) {
            return;
        }
        let output_size = self.output_size();
        let (output_width, output_height) = output_size.unwrap_or((1, 1));
        let width = configured_dimension(configure.new_size.0, output_width);
        let height = configured_dimension(configure.new_size.1, output_height);
        if self.width != width || self.height != height {
            self.buffers = None;
            self.rendered_generation = u64::MAX;
        }
        self.width = width;
        self.height = height;
        let dimensions = scaled_dimensions(self.width, self.height, self.scale_factor);
        let needs_buffers = match (&self.buffers, dimensions.as_ref()) {
            (Some(buffers), Ok(&(width, height))) => {
                buffers.width != width || buffers.height != height
            }
            _ => true,
        };
        if needs_buffers {
            match dimensions.and_then(|(width, height)| BufferSet::new(width, height, &self.shm)) {
                Ok(buffers) => self.buffers = Some(buffers),
                Err(error) => {
                    eprintln!("Wayland overlay buffer error: {error}");
                    self.buffers = None;
                    set_error(&self.state, error);
                }
            }
        }
        if self.first_configure {
            self.first_configure = false;
            self.draw(qh);
        }
    }
}

fn configured_dimension(suggested: u32, output: u32) -> u32 {
    NonZeroU32::new(suggested).map_or(output.max(1), NonZeroU32::get)
}

fn scaled_dimensions(width: u32, height: u32, scale: u32) -> Result<(u32, u32), String> {
    if scale == 0 {
        return Err(String::from("invalid Wayland buffer scale: 0"));
    }
    let width = width.max(1);
    let height = height.max(1);
    let width = width
        .checked_mul(scale)
        .ok_or_else(|| String::from("Wayland scaled width is too large"))?;
    let height = height
        .checked_mul(scale)
        .ok_or_else(|| String::from("Wayland scaled height is too large"))?;
    if width > (i32::MAX as u32 / 4) || height > i32::MAX as u32 {
        return Err(String::from(
            "Wayland scaled buffer dimensions are too large",
        ));
    }
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(canvas: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * width + x) * 4) as usize;
        canvas[offset..offset + 4].try_into().unwrap()
    }

    #[test]
    fn runtime_error_can_be_reported_and_cleared() {
        let state = Arc::new(Mutex::new(SharedState {
            crosshair: CrosshairState::default(),
            display_index: 0,
            output_name: None,
            generation: 0,
            error: None,
        }));

        set_error(&state, "test failure");
        assert_eq!(state.lock().unwrap().error.as_deref(), Some("test failure"));
        clear_error(&state);
        assert_eq!(state.lock().unwrap().error, None);
    }

    #[test]
    fn integer_scale_uses_physical_buffer_dimensions() {
        assert_eq!(scaled_dimensions(1920, 1080, 1), Ok((1920, 1080)));
        assert_eq!(scaled_dimensions(1920, 1080, 2), Ok((3840, 2160)));
        assert!(scaled_dimensions(u32::MAX, 1, 2).is_err());
    }

    #[test]
    fn zero_configure_dimension_uses_output_size() {
        assert_eq!(configured_dimension(0, 1920), 1920);
        assert_eq!(configured_dimension(1080, 1920), 1080);
    }

    #[test]
    fn renderer_draws_default_dot_only() {
        let state = CrosshairState::default();
        let mut canvas = vec![0; 11 * 11 * 4];
        draw_crosshair(&mut canvas, 11, 11, &state);

        assert_eq!(pixel(&canvas, 11, 5, 5), [255, 0, 255, 255]);
        assert!(canvas.chunks_exact(4).filter(|pixel| pixel[3] != 0).count() > 1);
    }

    #[test]
    fn renderer_keeps_hidden_buffer_transparent() {
        let state = CrosshairState {
            visible: false,
            ..CrosshairState::default()
        };
        let mut canvas = vec![0; 11 * 11 * 4];
        draw_crosshair(&mut canvas, 11, 11, &state);
        assert!(canvas.iter().all(|channel| *channel == 0));
    }

    #[test]
    fn renderer_preserves_argb_byte_order_and_alpha() {
        let state = CrosshairState {
            color: Color {
                red: 255,
                green: 0,
                blue: 0,
            },
            alpha: 128,
            thickness: 1.0,
            ..CrosshairState::default()
        };
        let mut canvas = vec![0; 3 * 3 * 4];
        draw_crosshair(&mut canvas, 3, 3, &state);
        assert_eq!(pixel(&canvas, 3, 1, 1), [0, 0, 128, 128]);
    }

    #[test]
    fn renderer_clips_outlined_arms_to_buffer() {
        let state = CrosshairState {
            size: 8.0,
            thickness: 3.0,
            draw_outline: true,
            outline_thickness: 2.0,
            offset_x: -2,
            ..CrosshairState::default()
        };
        let mut canvas = vec![0; 5 * 5 * 4];
        draw_crosshair(&mut canvas, 5, 5, &state);
        assert!(canvas.chunks_exact(4).any(|pixel| pixel[3] != 0));
    }
}

impl ShmHandler for LayerState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

fn draw_crosshair(canvas: &mut [u8], width: u32, height: u32, state: &CrosshairState) {
    let center_x = width as f32 / 2.0 + state.offset_x as f32;
    let center_y = height as f32 / 2.0 + state.offset_y as f32;
    let color = [
        state.color.red,
        state.color.green,
        state.color.blue,
        state.alpha,
    ];
    let outline = [0, 0, 0, state.alpha];

    if state.visible && state.size > 0.0 {
        let half = state.thickness.max(0.0) / 2.0;
        let outline_half = half + state.outline_thickness.max(0.0);
        for (x0, y0, x1, y1) in [
            (
                center_x + state.gap,
                center_y - outline_half,
                center_x + state.gap + state.size,
                center_y + outline_half,
            ),
            (
                center_x - state.gap - state.size,
                center_y - outline_half,
                center_x - state.gap,
                center_y + outline_half,
            ),
            (
                center_x - outline_half,
                center_y + state.gap,
                center_x + outline_half,
                center_y + state.gap + state.size,
            ),
            (
                center_x - outline_half,
                center_y - state.gap - state.size,
                center_x + outline_half,
                center_y - state.gap,
            ),
        ] {
            if state.draw_outline {
                fill_rect(canvas, width, height, x0, y0, x1, y1, outline);
            }
        }
        if state.draw_outline {
            for (x0, y0, x1, y1) in [
                (
                    center_x + state.gap,
                    center_y - half,
                    center_x + state.gap + state.size,
                    center_y + half,
                ),
                (
                    center_x - state.gap - state.size,
                    center_y - half,
                    center_x - state.gap,
                    center_y + half,
                ),
                (
                    center_x - half,
                    center_y + state.gap,
                    center_x + half,
                    center_y + state.gap + state.size,
                ),
                (
                    center_x - half,
                    center_y - state.gap - state.size,
                    center_x + half,
                    center_y - state.gap,
                ),
            ] {
                fill_rect(canvas, width, height, x0, y0, x1, y1, color);
            }
        } else {
            fill_rect(
                canvas,
                width,
                height,
                center_x + state.gap,
                center_y - half,
                center_x + state.gap + state.size,
                center_y + half,
                color,
            );
            fill_rect(
                canvas,
                width,
                height,
                center_x - state.gap - state.size,
                center_y - half,
                center_x - state.gap,
                center_y + half,
                color,
            );
            fill_rect(
                canvas,
                width,
                height,
                center_x - half,
                center_y + state.gap,
                center_x + half,
                center_y + state.gap + state.size,
                color,
            );
            fill_rect(
                canvas,
                width,
                height,
                center_x - half,
                center_y - state.gap - state.size,
                center_x + half,
                center_y - state.gap,
                color,
            );
        }
    }

    if state.visible && state.dot {
        let radius = state.thickness.max(1.0) / 2.0;
        if state.draw_outline {
            fill_circle(
                canvas,
                width,
                height,
                center_x,
                center_y,
                radius + state.outline_thickness,
                outline,
            );
        }
        fill_circle(canvas, width, height, center_x, center_y, radius, color);
    }
}

fn fill_rect(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    color: [u8; 4],
) {
    let left = x0.min(x1).floor().max(0.0) as u32;
    let right = x0.max(x1).ceil().min(width as f32) as u32;
    let top = y0.min(y1).floor().max(0.0) as u32;
    let bottom = y0.max(y1).ceil().min(height as f32) as u32;
    for y in top..bottom {
        for x in left..right {
            blend(&mut canvas[((y * width + x) * 4) as usize..][..4], color);
        }
    }
}

fn fill_circle(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    cx: f32,
    cy: f32,
    radius: f32,
    color: [u8; 4],
) {
    let left = (cx - radius).floor().max(0.0) as u32;
    let right = (cx + radius).ceil().min(width as f32) as u32;
    let top = (cy - radius).floor().max(0.0) as u32;
    let bottom = (cy + radius).ceil().min(height as f32) as u32;
    let radius_squared = radius * radius;
    for y in top..bottom {
        for x in left..right {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            if dx * dx + dy * dy <= radius_squared {
                blend(&mut canvas[((y * width + x) * 4) as usize..][..4], color);
            }
        }
    }
}

fn blend(pixel: &mut [u8], color: [u8; 4]) {
    let source_alpha = color[3] as u32;
    if source_alpha == 0 {
        return;
    }
    let destination_alpha = pixel[3] as u32;
    let inverse = 255 - source_alpha;
    let output_alpha = source_alpha + (destination_alpha * inverse + 127) / 255;
    let source = [
        (color[0] as u32 * source_alpha + 127) / 255,
        (color[1] as u32 * source_alpha + 127) / 255,
        (color[2] as u32 * source_alpha + 127) / 255,
    ];
    let mut output = [0u8; 3];
    for (index, channel) in source.into_iter().enumerate() {
        output[index] = (channel + (pixel[2 - index] as u32 * inverse + 127) / 255).min(255) as u8;
    }
    pixel[0] = output[2];
    pixel[1] = output[1];
    pixel[2] = output[0];
    pixel[3] = output_alpha.min(255) as u8;
}

delegate_compositor!(LayerState);
delegate_output!(LayerState);
delegate_shm!(LayerState);
delegate_layer!(LayerState);
delegate_registry!(LayerState);

impl ProvidesRegistryState for LayerState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}
