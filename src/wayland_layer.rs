use std::{
    num::NonZeroU32,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

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
    shm::{Shm, ShmHandler, slot::SlotPool},
};

#[derive(Clone)]
struct SharedState {
    crosshair: CrosshairState,
    display_index: usize,
    output_name: Option<String>,
    generation: u64,
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
        *state = SharedState {
            crosshair,
            display_index,
            output_name,
            generation,
        };
    }
}

fn run(
    state: Arc<Mutex<SharedState>>,
    ready_tx: &std::sync::mpsc::SyncSender<Result<(), String>>,
) -> Result<(), String> {
    let result = run_inner(state, ready_tx);
    if let Err(error) = &result {
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
        pool: None,
        layer: None,
        width: 1,
        height: 1,
        first_configure: true,
        display_index: 0,
        rendered_generation: u64::MAX,
        state,
    };

    event_queue
        .roundtrip(&mut overlay)
        .map_err(|error| error.to_string())?;
    overlay.create_layer(&queue_handle);
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
    pool: Option<SlotPool>,
    layer: Option<LayerSurface>,
    width: u32,
    height: u32,
    first_configure: bool,
    display_index: usize,
    rendered_generation: u64,
    state: Arc<Mutex<SharedState>>,
}

impl LayerState {
    fn selected_output(
        &self,
        requested_name: Option<&str>,
        index: usize,
    ) -> Option<wl_output::WlOutput> {
        let outputs = self.output_state.outputs().collect::<Vec<_>>();
        if let Some(name) = requested_name {
            if let Some(output) = outputs.iter().find(|output| {
                self.output_state
                    .info(output)
                    .and_then(|info| info.name)
                    .as_deref()
                    == Some(name)
            }) {
                return Some(output.clone());
            }
        }
        outputs.into_iter().nth(index)
    }

    fn create_layer(&mut self, qh: &QueueHandle<Self>) {
        let shared = self.state.lock().unwrap().clone();
        self.display_index = shared.display_index;
        let output = self.selected_output(shared.output_name.as_deref(), shared.display_index);
        let surface = self.compositor.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Overlay,
            Some("crosshair-overlay"),
            output.as_ref(),
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
        self.first_configure = true;
    }

    fn draw(&mut self, qh: &QueueHandle<Self>) {
        let Some(layer) = self.layer.clone() else {
            return;
        };
        let Some(pool) = self.pool.as_mut() else {
            return;
        };
        let width = self.width.max(1);
        let height = self.height.max(1);
        let stride = width as i32 * 4;
        let (buffer, canvas) = match pool.create_buffer(
            width as i32,
            height as i32,
            stride,
            wl_shm::Format::Argb8888,
        ) {
            Ok(buffer) => buffer,
            Err(error) => {
                eprintln!("Wayland overlay buffer error: {error}");
                return;
            }
        };
        canvas.fill(0);
        let shared = self.state.lock().unwrap().clone();
        draw_crosshair(canvas, width, height, &shared.crosshair);
        layer
            .wl_surface()
            .damage_buffer(0, 0, width as i32, height as i32);
        layer.wl_surface().frame(qh, layer.wl_surface().clone());
        if let Err(error) = buffer.attach_to(layer.wl_surface()) {
            eprintln!("Wayland overlay attach error: {error}");
            return;
        }
        layer.commit();
        self.rendered_generation = shared.generation;
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
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
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
            let shared = self.state.lock().unwrap().clone();
            let desired = shared.display_index;
            if desired != self.display_index {
                self.layer = None;
                self.create_layer(qh);
            } else if shared.generation != self.rendered_generation {
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
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for LayerState {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.layer = None;
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
        self.width = NonZeroU32::new(configure.new_size.0).map_or(1, NonZeroU32::get);
        self.height = NonZeroU32::new(configure.new_size.1).map_or(1, NonZeroU32::get);
        if self.pool.is_none() {
            self.pool = SlotPool::new(4 * 1024 * 1024, &self.shm).ok();
        }
        if self.first_configure {
            self.first_configure = false;
            self.draw(qh);
        }
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
