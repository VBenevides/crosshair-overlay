use std::{
    fs,
    path::{Path, PathBuf},
};

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

#[derive(Clone, Debug, PartialEq)]
struct AppConfig {
    state: RuntimeState,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            state: RuntimeState::default(),
        }
    }
}

impl AppConfig {
    fn load() -> (Self, Option<String>) {
        config_path()
            .and_then(|path| fs::read_to_string(path).ok())
            .map_or_else(
                || (Self::default(), None),
                |text| (Self::from_text(&text), Self::active_profile(&text)),
            )
    }

    fn active_profile(text: &str) -> Option<String> {
        text.lines().find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key == "active_profile" && valid_profile_name(value)).then(|| value.to_owned())
        })
    }

    fn from_text(text: &str) -> Self {
        let mut config = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key {
                "size" => set_non_negative_float(&mut config.state.crosshair.size, value),
                "gap" => set_float(&mut config.state.crosshair.gap, value),
                "thickness" => set_non_negative_float(&mut config.state.crosshair.thickness, value),
                "outline_thickness" => {
                    set_non_negative_float(&mut config.state.crosshair.outline_thickness, value)
                }
                "red" => set_u8(&mut config.state.crosshair.color.red, value),
                "green" => set_u8(&mut config.state.crosshair.color.green, value),
                "blue" => set_u8(&mut config.state.crosshair.color.blue, value),
                "alpha" => set_u8(&mut config.state.crosshair.alpha, value),
                "dot" => set_bool(&mut config.state.crosshair.dot, value),
                "draw_outline" => set_bool(&mut config.state.crosshair.draw_outline, value),
                "visible" => set_bool(&mut config.state.crosshair.visible, value),
                "offset_x" => set_i32(&mut config.state.crosshair.offset_x, value),
                "offset_y" => set_i32(&mut config.state.crosshair.offset_y, value),
                "display_index" => {
                    if let Ok(index) = value.parse() {
                        config.state.display_index = index;
                    }
                }
                _ => {}
            }
        }
        config
    }

    fn text(&self) -> String {
        let crosshair = &self.state.crosshair;
        format!(
            "size={}\ngap={}\nthickness={}\noutline_thickness={}\nred={}\ngreen={}\nblue={}\nalpha={}\ndot={}\ndraw_outline={}\nvisible={}\noffset_x={}\noffset_y={}\ndisplay_index={}\n",
            crosshair.size,
            crosshair.gap,
            crosshair.thickness,
            crosshair.outline_thickness,
            crosshair.color.red,
            crosshair.color.green,
            crosshair.color.blue,
            crosshair.alpha,
            crosshair.dot as u8,
            crosshair.draw_outline as u8,
            crosshair.visible as u8,
            crosshair.offset_x,
            crosshair.offset_y,
            self.state.display_index,
        )
    }
}

fn set_float(target: &mut f32, value: &str) {
    if let Ok(value) = value.parse::<f32>()
        && value.is_finite()
    {
        *target = value;
    }
}

fn set_non_negative_float(target: &mut f32, value: &str) {
    if let Ok(value) = value.parse::<f32>()
        && value.is_finite()
        && value >= 0.0
    {
        *target = value;
    }
}

fn set_u8(target: &mut u8, value: &str) {
    if let Ok(value) = value.parse() {
        *target = value;
    }
}

fn set_i32(target: &mut i32, value: &str) {
    if let Ok(value) = value.parse() {
        *target = value;
    }
}

fn set_bool(target: &mut bool, value: &str) {
    match value {
        "0" => *target = false,
        "1" => *target = true,
        _ => {}
    }
}

fn config_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("APPDATA").map(PathBuf::from);

    #[cfg(target_os = "linux")]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    let root: Option<PathBuf> = None;

    root.map(|root| root.join("crosshair").join("config"))
}

fn profiles_dir() -> Option<PathBuf> {
    config_path().map(|path| path.with_file_name("profiles"))
}

fn valid_profile_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

pub struct CrosshairApp {
    state: RuntimeState,
    displays: Vec<DisplayInfo>,
    command: String,
    profile_name: String,
    profile_file: String,
    profiles_open: bool,
    message: String,
    last_display_size: Option<DisplaySize>,
    #[cfg(target_os = "linux")]
    backend: platform::OverlayBackend,
}

impl CrosshairApp {
    #[cfg(target_os = "linux")]
    pub fn new(_creation_context: &CreationContext<'_>, backend: platform::OverlayBackend) -> Self {
        let message = backend.startup_message();
        let (config, profile_name) = AppConfig::load();
        Self {
            state: config.state,
            displays: Vec::new(),
            command: String::new(),
            profile_name: profile_name.unwrap_or_else(|| String::from("default")),
            profile_file: String::new(),
            profiles_open: false,
            message,
            last_display_size: None,
            backend,
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn new(_creation_context: &CreationContext<'_>) -> Self {
        let (config, profile_name) = AppConfig::load();
        Self {
            state: config.state,
            displays: Vec::new(),
            command: String::new(),
            profile_name: profile_name.unwrap_or_else(|| String::from("default")),
            profile_file: String::new(),
            profiles_open: false,
            message: String::from("Waiting for display information"),
            last_display_size: None,
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
        let state_before_refresh = self.state.clone();
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
        if self.state != state_before_refresh {
            self.save_config();
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
                self.save_config();
            }
            Err(error) => self.message = error.to_string(),
        }
    }

    fn save_config(&self) {
        let Some(path) = config_path() else {
            return;
        };
        let profile_name = self.profile_name.trim();
        self.save_state(
            &path,
            valid_profile_name(profile_name).then_some(profile_name),
        );
    }

    fn profile_path(&self) -> Option<PathBuf> {
        Self::profile_path_for(&self.profile_name)
    }

    fn profile_path_for(name: &str) -> Option<PathBuf> {
        let name = name.trim();
        if !valid_profile_name(name) {
            return None;
        }
        profiles_dir().map(|directory| directory.join(format!("{name}.config")))
    }

    fn profile_names() -> Vec<String> {
        let Some(directory) = profiles_dir() else {
            return Vec::new();
        };
        let Ok(entries) = fs::read_dir(directory) else {
            return Vec::new();
        };
        let mut names = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                (path.extension().and_then(|extension| extension.to_str()) == Some("config"))
                    .then(|| path.file_stem()?.to_str().map(str::to_owned))
                    .flatten()
            })
            .collect::<Vec<_>>();
        names.sort_by_cached_key(|name| name.to_ascii_lowercase());
        names
    }

    fn save_profile(&mut self) {
        let Some(path) = self.profile_path() else {
            self.message = String::from("Profile name must use letters, numbers, '-' or '_'");
            return;
        };
        if self.save_state(&path, None) {
            self.save_config();
            self.message = format!("Saved profile: {}", self.profile_name.trim());
        } else {
            self.message = String::from("Could not save profile");
        }
    }

    fn load_profile(&mut self) {
        let Some(path) = self.profile_path() else {
            self.message = String::from("Profile name must use letters, numbers, '-' or '_'");
            return;
        };
        match fs::read_to_string(path) {
            Ok(text) => {
                self.state = AppConfig::from_text(&text).state;
                self.last_display_size = None;
                self.save_config();
                self.message = format!("Loaded profile: {}", self.profile_name.trim());
            }
            Err(error) => self.message = format!("Could not load profile: {error}"),
        }
    }

    fn delete_profile(&mut self, name: &str) {
        let Some(path) = Self::profile_path_for(name) else {
            self.message = String::from("Invalid profile name");
            return;
        };
        match fs::remove_file(&path) {
            Ok(()) => {
                if self.profile_name.trim() == name {
                    self.profile_name = String::from("default");
                }
                self.save_config();
                self.message = format!("Deleted profile: {name}");
            }
            Err(error) => self.message = format!("Could not delete profile: {error}"),
        }
    }

    fn import_profile(&mut self) {
        let path = PathBuf::from(self.profile_file.trim());
        if path.as_os_str().is_empty() {
            self.message = String::from("Enter a profile file path to import");
            return;
        }
        match fs::read_to_string(&path) {
            Ok(text) => {
                self.state = AppConfig::from_text(&text).state;
                self.last_display_size = None;
                self.save_config();
                self.message = format!("Imported profile: {}", path.display());
            }
            Err(error) => self.message = format!("Could not import profile: {error}"),
        }
    }

    fn export_profile(&mut self) {
        let path = PathBuf::from(self.profile_file.trim());
        if path.as_os_str().is_empty() {
            self.message = String::from("Enter a profile file path to export");
            return;
        }
        if self.save_state(&path, None) {
            self.message = format!("Exported profile: {}", path.display());
        } else {
            self.message = String::from("Could not export profile");
        }
    }

    fn save_state(&self, path: &Path, active_profile: Option<&str>) -> bool {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            if fs::create_dir_all(parent).is_err() {
                return false;
            }
        }
        let temporary = path.with_extension("tmp");
        let mut text = AppConfig {
            state: self.state.clone(),
        }
        .text();
        if let Some(profile) = active_profile {
            text.push_str(&format!("active_profile={profile}\n"));
        }
        let _ = fs::remove_file(&temporary);
        if fs::write(&temporary, &text).is_err() {
            return false;
        }
        if fs::rename(&temporary, path).is_ok() {
            return true;
        }
        let saved = fs::write(path, text).is_ok();
        let _ = fs::remove_file(temporary);
        saved
    }

    fn show_top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top_bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Profiles").clicked() {
                    self.profiles_open = !self.profiles_open;
                }
                if ui.button("Save Profile  (Ctrl+S)").clicked() {
                    self.save_profile();
                }
                if ui.button("Load Profile  (Ctrl+O)").clicked() {
                    self.load_profile();
                }
            });
            if self.profiles_open {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut self.profile_name);
                });
                ui.label("Saved profiles");
                let profiles = Self::profile_names();
                if profiles.is_empty() {
                    ui.small("No saved profiles");
                } else {
                    egui::ScrollArea::horizontal()
                        .max_height(80.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let column_count = profiles.len().div_ceil(3);
                                for (index, column) in profiles.chunks(3).enumerate() {
                                    ui.vertical(|ui| {
                                        for profile in column {
                                            ui.horizontal(|ui| {
                                                if ui
                                                    .selectable_label(
                                                        self.profile_name.trim() == profile,
                                                        profile,
                                                    )
                                                    .clicked()
                                                {
                                                    self.profile_name = profile.clone();
                                                }
                                                if ui.small_button("X").clicked() {
                                                    self.delete_profile(profile);
                                                }
                                            });
                                        }
                                    });
                                    if index + 1 < column_count {
                                        ui.separator();
                                    }
                                }
                            });
                        });
                }
                ui.horizontal(|ui| {
                    ui.label("File");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.profile_file)
                            .hint_text("path/to/profile.config"),
                    );
                    if ui.button("Import").clicked() {
                        self.import_profile();
                    }
                    if ui.button("Export").clicked() {
                        self.export_profile();
                    }
                });
            }
        });
    }

    fn show_overlay(&mut self, context: &egui::Context) {
        let Some(display) = self.selected_display() else {
            return;
        };
        let display_size = display.size;

        let state = self.state.crosshair.clone();
        let overlay_id = egui::ViewportId::from_hash_of("crosshair-overlay");

        #[cfg(target_os = "linux")]
        if let platform::OverlayBackend::Wayland(overlay) = &self.backend {
            if display.scale_factor.fract() != 0.0 {
                self.message = format!(
                    "Wayland fractional display scale {:.2} is unsupported",
                    display.scale_factor
                );
                return;
            }
            overlay.update(state, self.state.display_index, display.name.clone());
            if let Some(error) = overlay.error() {
                self.message = format!("Wayland overlay: {error}");
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
        if Self::slider(ui, "Gap", &mut gap, -64.0..=64.0) {
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
            let mut color = [
                self.state.crosshair.color.red,
                self.state.crosshair.color.green,
                self.state.crosshair.color.blue,
            ];
            if ui.color_edit_button_srgb(&mut color).changed() {
                self.run_command(format!(
                    "cl_crosshaircolor #{:02x}{:02x}{:02x}",
                    color[0], color[1], color[2]
                ));
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
            }
        });
    }

    fn show_preview(&self, ui: &mut egui::Ui) {
        ui.heading("Preview");
        let state = &self.state.crosshair;
        egui::Grid::new("crosshair_preview_grid")
            .num_columns(2)
            .spacing(egui::vec2(6.0, 6.0))
            .show(ui, |ui| {
                for (index, background) in [
                    egui::Color32::WHITE,
                    egui::Color32::from_gray(192),
                    egui::Color32::from_gray(64),
                    egui::Color32::BLACK,
                ]
                .into_iter()
                .enumerate()
                {
                    preview_cell(ui, state, background);
                    if index % 2 == 1 {
                        ui.end_row();
                    }
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
                            self.save_config();
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
        let (save_profile, load_profile) = ui.ctx().input(|input| {
            (
                input.modifiers.ctrl && input.key_pressed(egui::Key::S),
                input.modifiers.ctrl && input.key_pressed(egui::Key::O),
            )
        });
        if save_profile {
            self.save_profile();
        }
        if load_profile {
            self.load_profile();
        }
        self.refresh_displays(frame);
        self.show_top_bar(ui);
        egui::CentralPanel::default().show(ui, |ui| {
            ui.columns(2, |columns| {
                self.show_crosshair_settings(&mut columns[0]);
                self.show_preview(&mut columns[1]);
            });
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

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_config();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}

fn preview_cell(ui: &mut egui::Ui, state: &crosshair::CrosshairState, background: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(120.0, 72.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, background);
    let mut preview_state = state.clone();
    preview_state.offset_x = 0;
    preview_state.offset_y = 0;
    let mut preview_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    draw_crosshair(
        &mut preview_ui,
        &preview_state,
        DisplaySize {
            width: 1,
            height: 1,
        },
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crosshair::Color;

    #[test]
    fn config_round_trips_current_state() {
        let mut state = RuntimeState::default();
        state.display_index = 2;
        state.crosshair.size = 8.0;
        state.crosshair.offset_x = -30;
        state.crosshair.color = Color {
            red: 12,
            green: 34,
            blue: 56,
        };
        state.crosshair.visible = false;

        let text = AppConfig {
            state: state.clone(),
        }
        .text();
        assert_eq!(AppConfig::from_text(&text).state, state);
    }

    #[test]
    fn invalid_config_values_keep_defaults() {
        let config = AppConfig::from_text("size=-1\nalpha=999\ndot=maybe\noffset_x=nope\n");
        assert_eq!(config.state, RuntimeState::default());
    }

    #[test]
    fn active_profile_round_trips_only_valid_names() {
        assert_eq!(
            AppConfig::active_profile("active_profile=Competitive\n"),
            Some(String::from("Competitive"))
        );
        assert_eq!(
            AppConfig::active_profile("active_profile=../config\n"),
            None
        );
    }
}
