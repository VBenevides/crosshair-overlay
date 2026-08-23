use crosshair::{DisplaySize, OffsetLimits, RuntimeState};
use eframe::{App, CreationContext, Frame, egui};

use crate::platform;

#[derive(Clone, Debug)]
struct DisplayInfo {
    label: String,
    #[cfg(target_os = "linux")]
    name: Option<String>,
    size: DisplaySize,
    position: (i32, i32),
    scale_factor: f64,
}

pub struct CrosshairApp {
    state: RuntimeState,
    displays: Vec<DisplayInfo>,
    command: String,
    message: String,
    color_text: String,
    last_display_size: Option<DisplaySize>,
    #[cfg(target_os = "linux")]
    wayland_overlay: Option<platform::WaylandOverlay>,
}

impl CrosshairApp {
    pub fn new(_creation_context: &CreationContext<'_>) -> Self {
        #[allow(unused_mut)]
        let mut message = String::from("Waiting for display information");
        #[cfg(target_os = "linux")]
        let wayland_overlay = if platform::uses_wayland() {
            match platform::WaylandOverlay::new() {
                Ok(overlay) => Some(overlay),
                Err(error) => {
                    message = error;
                    None
                }
            }
        } else {
            None
        };

        Self {
            state: RuntimeState::default(),
            displays: Vec::new(),
            command: String::new(),
            message,
            color_text: String::from("#ff00ff"),
            last_display_size: None,
            #[cfg(target_os = "linux")]
            wayland_overlay,
        }
    }

    fn refresh_displays(&mut self, frame: &Frame) {
        let Some(window) = frame.winit_window() else {
            return;
        };
        let displays = window
            .available_monitors()
            .enumerate()
            .filter_map(|(index, monitor)| {
                let physical_size = monitor.size();
                let size = DisplaySize::new(physical_size.width, physical_size.height).ok()?;
                let name = monitor.name().unwrap_or_else(|| format!("Display {index}"));
                Some(DisplayInfo {
                    label: format!(
                        "{name} ({width}x{height})",
                        width = size.width,
                        height = size.height
                    ),
                    #[cfg(target_os = "linux")]
                    name: monitor.name(),
                    size,
                    position: (monitor.position().x, monitor.position().y),
                    scale_factor: monitor.scale_factor(),
                })
            })
            .collect::<Vec<_>>();

        if displays.is_empty() {
            self.displays.clear();
            self.state.display_index = 0;
            self.message = String::from("No usable display found");
            return;
        }
        let previous_index = self.state.display_index;
        self.displays = displays;
        if self.state.display_index >= self.displays.len() {
            self.state.display_index = 0;
            self.message =
                format!("Display {previous_index} is unavailable; switched to display 0");
        }
        let display_size = self.displays[self.state.display_index].size;
        if self.last_display_size != Some(display_size) {
            self.last_display_size = Some(display_size);
            if !display_size
                .contains_offset(self.state.crosshair.offset_x, self.state.crosshair.offset_y)
            {
                self.state.crosshair.offset_x = 0;
                self.state.crosshair.offset_y = 0;
                self.message = String::from("Display changed; offset reset to (0, 0)");
            }
        }
    }

    fn selected_display(&self) -> Option<&DisplayInfo> {
        self.displays.get(self.state.display_index)
    }

    fn selected_size(&self) -> Option<DisplaySize> {
        self.selected_display().map(|display| display.size)
    }

    fn limits(&self) -> Option<OffsetLimits> {
        self.selected_size().map(DisplaySize::offset_limits)
    }

    fn run_command(&mut self, command: String) {
        let Some(display) = self.selected_size() else {
            self.message = String::from("Cannot apply command without a display");
            return;
        };
        match self.state.apply(&command, display) {
            Ok(message) => {
                self.message = format!("{message}: {command} | {}", self.state.status(display));
            }
            Err(error) => self.message = error.to_string(),
        }
    }

    fn show_overlay(&mut self, context: &egui::Context) {
        let Some(display) = self.selected_display() else {
            return;
        };
        let display_size = display.size;
        if let Err(message) = platform::availability() {
            self.message = message.to_owned();
            return;
        }

        let state = self.state.crosshair.clone();
        let overlay_id = egui::ViewportId::from_hash_of("crosshair-overlay");

        #[cfg(target_os = "linux")]
        if platform::uses_wayland() {
            if let Some(overlay) = &self.wayland_overlay {
                overlay.update(state, self.state.display_index, display.name.clone());
            }
            return;
        }

        context.show_viewport_deferred(
            overlay_id,
            platform::overlay_viewport(
                self.state.display_index,
                [
                    display.size.width as f32 / display.scale_factor as f32,
                    display.size.height as f32 / display.scale_factor as f32,
                ],
                [
                    display.position.0 as f32 / display.scale_factor as f32,
                    display.position.1 as f32 / display.scale_factor as f32,
                ],
                state.visible,
            ),
            move |ui, _class| {
                if state.visible {
                    draw_crosshair(ui, &state, display_size);
                }
            },
        );
    }

    fn slider(
        ui: &mut egui::Ui,
        label: &str,
        value: &mut f32,
        range: std::ops::RangeInclusive<f32>,
    ) -> bool {
        ui.add(egui::Slider::new(value, range).text(label))
            .changed()
    }

    fn show_crosshair_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Crosshair");

        let mut size = self.state.crosshair.size;
        if Self::slider(ui, "Size", &mut size, 0.0..=64.0) {
            self.run_command(format!("cl_crosshairsize {size}"));
        }
        let mut gap = self.state.crosshair.gap;
        if Self::slider(ui, "Gap", &mut gap, 0.0..=64.0) {
            self.run_command(format!("cl_crosshairgap {gap}"));
        }
        let mut thickness = self.state.crosshair.thickness;
        if Self::slider(ui, "Thickness", &mut thickness, 0.0..=20.0) {
            self.run_command(format!("cl_crosshairthickness {thickness}"));
        }
        let mut outline_thickness = self.state.crosshair.outline_thickness;
        if Self::slider(ui, "Outline", &mut outline_thickness, 0.0..=20.0) {
            self.run_command(format!("cl_crosshair_outlinethickness {outline_thickness}"));
        }

        let mut alpha = self.state.crosshair.alpha;
        if ui
            .add(egui::Slider::new(&mut alpha, 0..=255).text("Alpha"))
            .changed()
        {
            self.run_command(format!("cl_crosshairalpha {alpha}"));
        }
        let mut dot = self.state.crosshair.dot;
        if ui.checkbox(&mut dot, "Center dot").changed() {
            self.run_command(format!("cl_crosshairdot {}", dot as u8));
        }
        let mut outline = self.state.crosshair.draw_outline;
        if ui.checkbox(&mut outline, "Draw outline").changed() {
            self.run_command(format!("cl_crosshair_drawoutline {}", outline as u8));
        }

        ui.horizontal(|ui| {
            ui.label("Color");
            if ui.text_edit_singleline(&mut self.color_text).lost_focus()
                && ui.input(|input| input.key_pressed(egui::Key::Enter))
            {
                self.run_command(format!("cl_crosshaircolor {}", self.color_text.trim()));
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Show").clicked() {
                self.run_command(String::from("crosshair_show"));
            }
            if ui.button("Hide").clicked() {
                self.run_command(String::from("crosshair_hide"));
            }
            if ui.button("Reset").clicked() {
                self.run_command(String::from("crosshair_reset"));
                self.color_text = String::from("#ff00ff");
            }
        });
    }

    fn show_position_settings(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Position");
        if self.displays.len() > 1 {
            let selected_label = self
                .selected_display()
                .map(|display| display.label.as_str())
                .unwrap_or("Unavailable");
            egui::ComboBox::from_label("Display")
                .selected_text(selected_label)
                .show_ui(ui, |ui| {
                    for (index, display) in self.displays.iter().enumerate() {
                        if ui
                            .selectable_value(&mut self.state.display_index, index, &display.label)
                            .changed()
                        {
                            self.message = format!("Selected {}", display.label);
                        }
                    }
                });
        } else if let Some(display) = self.selected_display() {
            ui.label(format!("Display: {}", display.label));
        }

        let Some(limits) = self.limits() else {
            ui.colored_label(egui::Color32::RED, "No display limits available");
            return;
        };
        ui.label(format!("Allowed X: {}..{}", limits.min_x, limits.max_x));
        ui.label(format!("Allowed Y: {}..{}", limits.min_y, limits.max_y));

        let mut x = self.state.crosshair.offset_x;
        let mut y = self.state.crosshair.offset_y;
        let x_changed = ui
            .add(
                egui::DragValue::new(&mut x)
                    .speed(1)
                    .range(limits.min_x..=limits.max_x)
                    .prefix("X: "),
            )
            .changed();
        let y_changed = ui
            .add(
                egui::DragValue::new(&mut y)
                    .speed(1)
                    .range(limits.min_y..=limits.max_y)
                    .prefix("Y: "),
            )
            .changed();
        if x_changed || y_changed {
            self.run_command(format!("crosshair_offset {x} {y}"));
        }
        ui.label("(0, 0) is the selected display center");
    }
}

impl App for CrosshairApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame) {
        self.refresh_displays(frame);
        egui::CentralPanel::default().show(ui, |ui| {
            self.show_crosshair_settings(ui);
            self.show_position_settings(ui);
            ui.separator();
            ui.heading("Command");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.command);
                if ui.button("Run").clicked() {
                    let command = std::mem::take(&mut self.command);
                    self.run_command(command);
                }
            });
            ui.label(&self.message);
            ui.small(
                "Wayland uses wlr-layer-shell; X11/Windows may need borderless-windowed mode.",
            );
            if let Some(display) = self.selected_size() {
                ui.monospace(self.state.status(display));
                if let Some(info) = self.selected_display() {
                    ui.small(format!("Scale factor: {:.2}", info.scale_factor));
                }
            }
        });
        self.show_overlay(ui.ctx());
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

fn draw_crosshair(ui: &mut egui::Ui, state: &crosshair::CrosshairState, _display: DisplaySize) {
    let pixels_per_point = ui.ctx().pixels_per_point();
    let to_points = |pixels: f32| pixels / pixels_per_point;
    let center = ui.max_rect().center()
        + egui::vec2(
            state.offset_x as f32 / pixels_per_point,
            state.offset_y as f32 / pixels_per_point,
        );
    let color = egui::Color32::from_rgba_unmultiplied(
        state.color.red,
        state.color.green,
        state.color.blue,
        state.alpha,
    );
    let outline_color = egui::Color32::from_rgba_unmultiplied(0, 0, 0, state.alpha);
    let stroke_width = to_points(state.thickness);
    let outline_width = to_points(state.outline_thickness * 2.0);
    let painter = ui.painter();
    let gap = to_points(state.gap);
    let size = to_points(state.size);

    if state.size > 0.0 {
        for direction in [
            egui::vec2(1.0, 0.0),
            egui::vec2(-1.0, 0.0),
            egui::vec2(0.0, 1.0),
            egui::vec2(0.0, -1.0),
        ] {
            let start = center + direction * gap;
            let end = center + direction * (gap + size);
            if state.draw_outline {
                painter.line_segment(
                    [start, end],
                    egui::Stroke::new(stroke_width + outline_width, outline_color),
                );
            }
            painter.line_segment([start, end], egui::Stroke::new(stroke_width, color));
        }
    }

    if state.dot {
        let radius = to_points(state.thickness.max(1.0)) / 2.0;
        if state.draw_outline {
            painter.circle_filled(
                center,
                radius + to_points(state.outline_thickness),
                outline_color,
            );
        }
        painter.circle_filled(center, radius, color);
    }
}
