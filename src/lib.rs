use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Color {
    pub const YELLOW: Self = Self {
        red: 255,
        green: 255,
        blue: 0,
    };

    fn parse(value: &str) -> Result<Self, CommandError> {
        let named = match value.to_ascii_lowercase().as_str() {
            "black" => Some(Self {
                red: 0,
                green: 0,
                blue: 0,
            }),
            "blue" => Some(Self {
                red: 0,
                green: 0,
                blue: 255,
            }),
            "cyan" => Some(Self {
                red: 0,
                green: 255,
                blue: 255,
            }),
            "green" => Some(Self {
                red: 0,
                green: 255,
                blue: 0,
            }),
            "magenta" => Some(Self {
                red: 255,
                green: 0,
                blue: 255,
            }),
            "red" => Some(Self {
                red: 255,
                green: 0,
                blue: 0,
            }),
            "white" => Some(Self {
                red: 255,
                green: 255,
                blue: 255,
            }),
            "yellow" => Some(Self::YELLOW),
            _ => None,
        };
        if let Some(color) = named {
            return Ok(color);
        }

        let hex = value.strip_prefix('#').unwrap_or(value);
        if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(CommandError::InvalidValue("color"));
        }
        Ok(Self {
            red: u8::from_str_radix(&hex[0..2], 16).unwrap(),
            green: u8::from_str_radix(&hex[2..4], 16).unwrap(),
            blue: u8::from_str_radix(&hex[4..6], 16).unwrap(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplaySize {
    pub width: u32,
    pub height: u32,
}

impl DisplaySize {
    pub fn new(width: u32, height: u32) -> Result<Self, CommandError> {
        if width == 0 || height == 0 {
            return Err(CommandError::InvalidDisplay);
        }
        Ok(Self { width, height })
    }

    pub fn center(self) -> (i32, i32) {
        ((self.width / 2) as i32, (self.height / 2) as i32)
    }

    pub fn contains_offset(self, x: i32, y: i32) -> bool {
        let (center_x, center_y) = self.center();
        let position_x = center_x + x;
        let position_y = center_y + y;
        position_x >= 0
            && position_y >= 0
            && position_x < self.width as i32
            && position_y < self.height as i32
    }

    pub fn position(self, x: i32, y: i32) -> Option<(i32, i32)> {
        self.contains_offset(x, y).then(|| {
            let (center_x, center_y) = self.center();
            (center_x + x, center_y + y)
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CrosshairState {
    pub visible: bool,
    pub size: f32,
    pub gap: f32,
    pub thickness: f32,
    pub color: Color,
    pub alpha: u8,
    pub dot: bool,
    pub draw_outline: bool,
    pub outline_thickness: f32,
    pub offset_x: i32,
    pub offset_y: i32,
}

impl Default for CrosshairState {
    fn default() -> Self {
        Self {
            visible: true,
            size: 5.0,
            gap: 2.0,
            thickness: 1.0,
            color: Color::YELLOW,
            alpha: 255,
            dot: true,
            draw_outline: false,
            outline_thickness: 1.0,
            offset_x: 0,
            offset_y: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeState {
    pub crosshair: CrosshairState,
    pub display_index: usize,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            crosshair: CrosshairState::default(),
            display_index: 0,
        }
    }
}

impl RuntimeState {
    pub fn apply(&mut self, line: &str, display: DisplaySize) -> Result<String, CommandError> {
        let command = Command::from_str(line)?;
        let mut next = self.clone();
        match command {
            Command::SetSize(value) => next.crosshair.size = value,
            Command::SetGap(value) => next.crosshair.gap = value,
            Command::SetThickness(value) => next.crosshair.thickness = value,
            Command::SetColor(value) => next.crosshair.color = value,
            Command::SetAlpha(value) => next.crosshair.alpha = value,
            Command::SetDot(value) => next.crosshair.dot = value,
            Command::SetOutline(value) => next.crosshair.draw_outline = value,
            Command::SetOutlineThickness(value) => next.crosshair.outline_thickness = value,
            Command::SetOffset(x, y) => {
                if !display.contains_offset(x, y) {
                    return Err(CommandError::OffsetOutsideDisplay);
                }
                next.crosshair.offset_x = x;
                next.crosshair.offset_y = y;
            }
            Command::Show => next.crosshair.visible = true,
            Command::Hide => next.crosshair.visible = false,
            Command::Toggle => next.crosshair.visible = !next.crosshair.visible,
            Command::Reset => next.crosshair = CrosshairState::default(),
        }
        *self = next;
        Ok(String::from("ok"))
    }

    pub fn position(&self, display: DisplaySize) -> Option<(i32, i32)> {
        display.position(self.crosshair.offset_x, self.crosshair.offset_y)
    }

    pub fn status(&self, display: DisplaySize) -> String {
        let position = self.position(display);
        format!(
            "visible={} display={} position={:?} size={} gap={} thickness={} color=#{:02x}{:02x}{:02x} alpha={} dot={} outline={} outline_thickness={}",
            self.crosshair.visible,
            self.display_index,
            position,
            self.crosshair.size,
            self.crosshair.gap,
            self.crosshair.thickness,
            self.crosshair.color.red,
            self.crosshair.color.green,
            self.crosshair.color.blue,
            self.crosshair.alpha,
            self.crosshair.dot,
            self.crosshair.draw_outline,
            self.crosshair.outline_thickness,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Command {
    SetSize(f32),
    SetGap(f32),
    SetThickness(f32),
    SetColor(Color),
    SetAlpha(u8),
    SetDot(bool),
    SetOutline(bool),
    SetOutlineThickness(f32),
    SetOffset(i32, i32),
    Show,
    Hide,
    Toggle,
    Reset,
}

impl FromStr for Command {
    type Err = CommandError;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let mut parts = line.split_whitespace();
        let name = parts.next().ok_or(CommandError::Empty)?;
        let command = match name {
            "cl_crosshairsize" => Self::SetSize(non_negative_float(&mut parts, "size")?),
            "cl_crosshairgap" => Self::SetGap(non_negative_float(&mut parts, "gap")?),
            "cl_crosshairthickness" => {
                Self::SetThickness(non_negative_float(&mut parts, "thickness")?)
            }
            "cl_crosshaircolor" => Self::SetColor(Color::parse(argument(&mut parts, "color")?)?),
            "cl_crosshairalpha" => Self::SetAlpha(
                argument(&mut parts, "alpha")?
                    .parse()
                    .map_err(|_| CommandError::InvalidValue("alpha"))?,
            ),
            "cl_crosshairdot" => Self::SetDot(boolean(argument(&mut parts, "dot")?)?),
            "cl_crosshair_drawoutline" => {
                Self::SetOutline(boolean(argument(&mut parts, "outline")?)?)
            }
            "cl_crosshair_outlinethickness" => {
                Self::SetOutlineThickness(non_negative_float(&mut parts, "outline_thickness")?)
            }
            "crosshair_offset" => Self::SetOffset(
                argument(&mut parts, "x")?
                    .parse()
                    .map_err(|_| CommandError::InvalidValue("x"))?,
                argument(&mut parts, "y")?
                    .parse()
                    .map_err(|_| CommandError::InvalidValue("y"))?,
            ),
            "crosshair_show" => Self::Show,
            "crosshair_hide" => Self::Hide,
            "crosshair_toggle" => Self::Toggle,
            "crosshair_reset" => Self::Reset,
            _ => return Err(CommandError::UnknownCommand(name.to_owned())),
        };
        if parts.next().is_some() {
            return Err(CommandError::WrongArgumentCount);
        }
        Ok(command)
    }
}

fn argument<'a>(
    parts: &mut impl Iterator<Item = &'a str>,
    name: &'static str,
) -> Result<&'a str, CommandError> {
    parts.next().ok_or(CommandError::MissingArgument(name))
}

fn non_negative_float<'a>(
    parts: &mut impl Iterator<Item = &'a str>,
    name: &'static str,
) -> Result<f32, CommandError> {
    let value = argument(parts, name)?
        .parse::<f32>()
        .map_err(|_| CommandError::InvalidValue(name))?;
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(CommandError::InvalidValue(name))
    }
}

fn boolean(value: &str) -> Result<bool, CommandError> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(CommandError::InvalidValue("boolean")),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandError {
    Empty,
    UnknownCommand(String),
    MissingArgument(&'static str),
    WrongArgumentCount,
    InvalidValue(&'static str),
    InvalidDisplay,
    OffsetOutsideDisplay,
}

impl fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(formatter, "command is empty"),
            Self::UnknownCommand(name) => write!(formatter, "unknown command: {name}"),
            Self::MissingArgument(name) => write!(formatter, "missing argument: {name}"),
            Self::WrongArgumentCount => write!(formatter, "wrong argument count"),
            Self::InvalidValue(name) => write!(formatter, "invalid value: {name}"),
            Self::InvalidDisplay => write!(formatter, "display dimensions must be non-zero"),
            Self::OffsetOutsideDisplay => {
                write!(formatter, "offset places the crosshair outside the display")
            }
        }
    }
}

impl std::error::Error for CommandError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn display() -> DisplaySize {
        DisplaySize::new(1920, 1080).unwrap()
    }

    #[test]
    fn defaults_are_centered_yellow_dot() {
        let state = RuntimeState::default();
        assert!(state.crosshair.visible);
        assert_eq!(state.crosshair.color, Color::YELLOW);
        assert!(state.crosshair.dot);
        assert_eq!(state.position(display()), Some((960, 540)));
    }

    #[test]
    fn commands_update_state() {
        let mut state = RuntimeState::default();
        state.apply("cl_crosshairsize 8", display()).unwrap();
        state.apply("cl_crosshaircolor #00ff80", display()).unwrap();
        state.apply("cl_crosshairdot 0", display()).unwrap();
        state.apply("crosshair_offset -30 0", display()).unwrap();
        assert_eq!(state.crosshair.size, 8.0);
        assert_eq!(
            state.crosshair.color,
            Color {
                red: 0,
                green: 255,
                blue: 128
            }
        );
        assert!(!state.crosshair.dot);
        assert_eq!(state.position(display()), Some((930, 540)));
    }

    #[test]
    fn invalid_commands_preserve_state() {
        let mut state = RuntimeState::default();
        let before = state.clone();
        assert!(state.apply("cl_crosshairsize -1", display()).is_err());
        assert!(state.apply("crosshair_offset -961 0", display()).is_err());
        assert_eq!(state, before);
    }

    #[test]
    fn odd_display_center_uses_floor() {
        let display = DisplaySize::new(3, 5).unwrap();
        assert_eq!(display.center(), (1, 2));
        assert_eq!(display.position(0, 0), Some((1, 2)));
    }
}
