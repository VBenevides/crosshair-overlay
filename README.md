# Crosshair

A lightweight, transparent crosshair overlay for desktop displays. Configure the crosshair in the GUI, preview it on several backgrounds, select a display, and position it relative to that display's center.

## Features

- Size, gap, thickness, color, alpha, center dot, and outline controls
- Multi-display selection and pixel offsets
- Wayland `wlr-layer-shell` overlay when available on Linux
- X11/XWayland fallback on Linux
- Settings saved automatically
- Named profiles with Ctrl+S and Ctrl+O

## Requirements

- Rust 1.95 or newer
- A desktop display session
- On Linux Wayland, a compositor with `wlr-layer-shell`; X11/XWayland is used as a fallback

## Build and run

```sh
cargo run --release
```

Run the tests with:

```sh
cargo test
```

## Commands

The command field accepts:

```text
cl_crosshairsize <number>
cl_crosshairgap <number> (including negative values)
cl_crosshairthickness <number>
cl_crosshaircolor <name|#rrggbb>
cl_crosshaircolor_r|g|b <0-255>
cl_crosshairalpha <0-255>
cl_crosshairdot <0|1|true|false>
cl_crosshair_drawoutline <0|1|true|false>
cl_crosshair_outlinethickness <number>
crosshair_offset <x> <y>
crosshair_show | crosshair_hide | crosshair_toggle | crosshair_reset
```

Commands may be separated by semicolons, so CS2 crosshair config strings can be pasted directly.

Offsets are relative to the selected display center and must remain inside the display.

## Configuration

Settings are saved automatically at:

- Linux: `$XDG_CONFIG_HOME/crosshair/config`, or `$HOME/.config/crosshair/config`
- Windows: `%APPDATA%\crosshair\config`

Profiles are saved in a `profiles` directory beside that file. Open the Profiles panel to
select existing profiles, or enter a file path to import/export a profile. The last selected
profile is restored when the application starts again.

## Security boundary

Crosshair is a display-only overlay. It does not inspect or modify game processes or files, inject or intercept input, use a network control endpoint, or require administrator/root privileges. Game-specific anti-cheat systems may still prohibit or detect overlays.
