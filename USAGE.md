# trv — Usage Guide

`trv` is a single binary with four subcommands: `tui`, `daemon`, `list`, `export`.

---

## Quick start

### 1. See available presets

```
trv list
```

Output:
```
  dashboard        Dashboard — 4-metric 2x2 grid: CPU temp, CPU usage, GPU temp, GPU usage
  minimal          Minimal — CPU temperature and CPU usage only
  clock_metrics    Clock + Metrics — Digital clock at top, CPU temp and CPU usage below
  cpu_gpu          CPU + GPU — 5 metrics: CPU temp, CPU usage, GPU temp, GPU usage, RAM usage
  all_metrics      All Metrics — Clock + CPU temp/usage + GPU temp/usage + RAM usage
  video            Video — Full-screen looping video widget using matrix.mp4
```

### 2. Open the TUI with a preset

```
trv tui --preset dashboard
```

This loads the built-in preset directly — no file needed.

### 3. Save the preset to a file so you can customise it

```
trv export dashboard > ~/.config/trv/themes/dashboard.toml
trv tui --theme ~/.config/trv/themes/dashboard.toml
```

Changes made in the TUI can then be saved back with **Ctrl+s**.

---

## TUI keybindings

### Navigation

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Cycle focus: Sidebar → Canvas → Properties |
| `↑` / `k` | Previous widget (sidebar) or previous field (properties) |
| `↓` / `j` | Next widget (sidebar) or next field (properties) |
| `Enter` | Select widget (sidebar) or begin editing field (properties) |
| `Esc` | Cancel edit / return to sidebar |
| `q` / `Ctrl+c` | Quit |
| `F1` / `?` | Toggle help overlay |
| `PageUp` / `PageDown` | Scroll log panel history |

Bottom strip panels:

- **Metrics panel** (left, 25% width): live CPU/GPU/MEM preview values.
- **Log panel** (right, 75% width): activity history (5 rows visible).

### Sidebar panel (widget list)

| Key | Action |
|-----|--------|
| `a` | Add a new widget (type picker popup) |
| `d` | Delete selected widget (confirm popup) |
| `Ctrl+↑` | Move widget up in the list |
| `Ctrl+↓` | Move widget down in the list |

### Canvas panel (device preview)

| Key                      | Action |
|--------------------------|--------|
| `←` / `↑` / `↓` / `→`          | Move selected widget 1 px (non-video widgets) |
| `Shift`+`←` / `↑` / `↓` / `→`  | Move selected widget 10 px (non-video widgets) |
| `j` / `k`                | Scroll widget selection without moving | 

### Properties panel (field editor)

| Key | Action |
|-----|--------|
| `↑` / `k` | Previous field |
| `↓` / `j` | Next field |
| `Enter` | Start editing highlighted field |
| `Esc` | Cancel the current edit |

For image/video widget `path` fields, `Enter` opens a file chooser instead of inline text editing.

While a field is being edited:

| Key | Action |
|-----|--------|
| `←` / `→` | Move cursor within text |
| `Home` / `Ctrl+a` | Cursor to start |
| `End` / `Ctrl+e` | Cursor to end |
| `Backspace` / `Delete` | Delete character |
| `Ctrl+u` | Clear to start |
| `Ctrl+k` | Clear to end |
| `Enter` | Confirm (validates and applies) |
| `Esc` | Cancel (original value restored) |

Validation errors appear in red at the bottom of the Properties panel.

### File and device operations

| Key | Action |
|-----|--------|
| `Ctrl+n` | Create a new empty theme (filename + meta dialog) |
| `Ctrl+s` | Save theme — opens save explorer overlay |
| `Ctrl+o` | Open theme — opens file explorer overlay |
| `p` | Push current theme to device |
| `r` | Cycle raw rotation code (`00 → 01 → 02 → 03`) via cmd38 |
| `Ctrl+r` | Re-enable device auto-rotation (ADB system setting) |

Inside the **New theme** dialog:

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Next / previous field |
| `↑` / `↓` | Previous / next field |
| `Enter` | Next field, then create on the last field |
| `Esc` | Cancel |

Inside the **Open theme** explorer overlay:

| Key | Action |
|-----|--------|
| `↑` / `k` /  `↓` / `j` | Move selection |
| `Home` / `End` | Jump to first / last entry |
| `PageUp` / `PageDown` | Scroll by page |
| `Enter` | Enter directory or open selected `.toml` file |
| `Backspace` / `←` / `h` | Go to parent directory |
| `.` | Toggle hidden files |
| `Esc` | Cancel open dialog |

Inside the **Save theme** explorer overlay:

| Key | Action |
|-----|--------|
| `↑` / `k` /  `↓` / `j` | Move selection |
| `Home` / `End` | Jump to first / last entry |
| `PageUp` / `PageDown` | Scroll by page |
| `Enter` (list mode) | Enter directory or copy selected file path into path field |
| `Tab` | Toggle between file list and path input |
| `Enter` (path mode) | Confirm and save to current path |
| `Backspace` / `←` / `h` | Go to parent directory |
| `.` | Toggle hidden files |
| `Esc` | Cancel save dialog |

Inside the **Media path** chooser overlay (image/video widget `path`):

| Key | Action |
|-----|--------|
| `↑` / `k` / `↓` / `j` | Move selection |
| `Home` / `End` | Jump to first / last entry |
| `PageUp` / `PageDown` | Scroll by page |
| `Enter` | Enter directory or select file path |
| `Backspace` / `←` / `h` | Go to parent directory |
| `.` | Toggle hidden files |
| `Esc` | Cancel picker |

When a media file is selected, the widget `path` is stored as an absolute host path.

When pushing from the TUI, `trv` auto-pushes local image assets first:

- each image/video widget `path` (local file) → `/sdcard/<basename(path)>`

Note: device-side theme activation can lag by up to ~10 seconds after push.

Rotation notes:

- `r` sends protocol cmd38 with raw orientation codes in a simple cycle.
- Raw code meaning can vary by device model/platform (matching vendor behavior).
- `Ctrl+r` uses `adb shell settings put system accelerometer_rotation 1` to re-enable auto-rotation.

---

## Editable fields

Most widget types (metric/clock/text/image) have these common fields:

| Field | Format |
|-------|--------|
| `x`, `y` | Integer pixels (0–483 / 0–479) |
| `width`, `height` | Integer pixels |
| `text_size` | Integer (font size in px) |
| `color` | `#RRGGBB` hex (e.g. `#00DDFF`) |
| `alpha` | `0.00` – `1.00` |
| `bold`, `italic`, `underline`, `strike` | `true` or `false` |
| `font` | Fixed font selector dropdown (`default` for built-in fallback) |

Kind-specific fields:

**Metric:**  `source`, `unit`, `label`, `show_label`

Valid `source` values: `cpu_temp`, `gpu_temp`, `cpu_usage`, `gpu_usage`, `mem_usage`

Valid `font` selector values:
`default`, `msyh`, `arial`, `impact`, `calibri`, `georgia`, `ni7seg`,
`harmonyos_black`, `harmonyos_bold`, `harmonyos_light`,
`harmonyos_medium`, `harmonyos_thin`

Notes:
- These map to firmware/app built-in font assets (not arbitrary `/sdcard` font files).
- `harmonyos_bold` is normalized to the firmware token with a typo (`blod`) internally.

**Clock:**  `time_format`

Valid values: `hh:mm:ss`, `date`, `weekday`

**Text:**  `content`

**Image:**  `path` (picked from file chooser; stored as absolute local host path; daemon/TUI push to `/sdcard/` automatically)

**Video:**  `path` (picked from file chooser; stored as absolute local host path; daemon/TUI push to `/sdcard/` automatically)

### Video widget notes

- The device app currently plays custom-theme video widgets as fullscreen overlays.
- Video `x`, `y`, `width`, and `height` are ignored on-device, so the TUI only exposes `path` for video widgets.
- Canvas preview also renders video widgets as fullscreen to match device behavior.
- Video playback support is firmware/decoder dependent.
- Very high-res videos (for example 4K) may fail to play on-device.
- If video does not play, transcode to a smaller H.264 baseline stream.

Example conversion command:

```bash
ffmpeg -i input.mp4 \
  -vf "scale=960:-2:flags=lanczos" \
  -c:v libx264 -profile:v baseline -level 3.1 -pix_fmt yuv420p \
  -an -movflags +faststart \
  output_trv.mp4
```

---

## Running the daemon

```bash
# Use a preset (no file needed)
trv daemon --preset dashboard --adb-forward

# Use your own theme file
trv daemon --theme ~/.config/trv/themes/my-theme.toml --adb-forward

# Dry-run (no device required, logs frame bytes)
trv daemon --preset minimal --dry-run

# Custom interval, temp offset
trv daemon --preset cpu_gpu --interval 0.5 --temp-offset -5.0
```

---

## Exporting and customising presets

```bash
# Dump a preset to a file
trv export minimal > ~/minimal.toml

# Edit it in the TUI
trv tui --theme ~/minimal.toml

# Save changes with Ctrl+s inside the TUI, then run the daemon
trv daemon --theme ~/minimal.toml --adb-forward
```

---

## Device connection

By default `trv` connects to `127.0.0.1:22222`.  The device communicates over
TCP, which is normally forwarded from USB via ADB:

```bash
# Manually forward before running (or use --adb-forward flag)
adb forward tcp:22222 tcp:22222

trv daemon --preset dashboard
# equivalent:
trv daemon --preset dashboard --adb-forward
```

The `--host` and `--port` flags override the defaults if your device is
accessible over the network directly.

---

## Theme TOML format

Themes are stored as TOML.  Use `trv export <slug>` to see a full example:

```toml
[meta]
name = "My Theme"
description = "Custom layout"

[[widget]]
type = "metric"
source = "cpu_temp"
unit = "°C"
label = "CPU"
show_label = true
x = 12
y = 12
width = 230
height = 220
text_size = 64
color = "#00DDFF"
alpha = 1.0
bold = true

[[widget]]
type = "clock"
time_format = "hh:mm:ss"
x = 52
y = 400
width = 380
height = 70
text_size = 48
color = "#FFFFFF"
alpha = 1.0

[[widget]]
type = "image"
path = "/home/user/Pictures/logo.png" # local file; sent as /sdcard/logo.png
x = 360
y = 20
width = 100
height = 100
alpha = 1.0
```

Valid `type` values: `metric`, `clock`, `text`, `image`, `video`
