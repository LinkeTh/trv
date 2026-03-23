---
name: trv-theme-creation
description: "Design, polish, and validate TRV LCD themes with custom assets, metric layouts, and on-device screenshot iteration."
compatibility: opencode
metadata:
  project: trv
  domain: thermalright-lcd-theme
  format: toml
---

# TRV Theme Creation Skill

Create high-quality `trv` themes for Thermalright/Frozen Vision LCD devices, including background assets, widget layout, and real-device validation.

## When To Use This Skill

Use this skill when the user asks to:

- Create a new theme (`.toml`) for `trv`
- Redesign or polish an existing theme
- Build style-specific themes (anime, cyberpunk, minimal, cozy, terminal, etc.)
- Add/update image or video backgrounds for a theme
- Validate theme appearance with device screenshots

---

## TOML Schema Reference

This section is the complete authoritative reference. Use it when writing or editing any theme file.

### File Structure

```toml
[meta]
name = "My Theme"           # optional, display name
description = "..."         # optional

[[widget]]                  # note: "widget" not "widgets" -- serde key is singular
type = "..."                # REQUIRED on every widget (see types below)
# ... fields ...

[[widget]]
type = "..."
# ... fields ...
```

> IMPORTANT: The TOML array table key is `[[widget]]` (singular). Using `[[widgets]]` will silently produce an empty theme.

---

### Common Fields (All Widget Types)

All fields below are **optional** and fall back to the listed defaults when omitted.

| Field | Type | Default | Notes |
|---|---|---|---|
| `x` | integer (u16) | `0` | Pixel X origin. Practical range: 0–483 |
| `y` | integer (u16) | `0` | Pixel Y origin. Practical range: 0–479 |
| `width` | integer (u16) | `0` | Widget width in pixels |
| `height` | integer (u16) | `0` | Widget height in pixels |
| `text_size` | integer (u16) | `40` | Font size in pixels |
| `color` | string | `"FFFFFF"` | `"#RRGGBB"` or `"RRGGBB"`. Must be exactly 6 hex digits. |
| `alpha` | float | `1.0` | Opacity 0.0–1.0. Encoded as 0–10 on device (alpha × 10 rounded). |
| `bold` | bool | `false` | |
| `italic` | bool | `false` | |
| `underline` | bool | `false` | |
| `strikethrough` | bool | `false` | |
| `font` | string | `""` | Empty string = device default. See font list below. |

**Color format:** both `"#00DDFF"` and `"00DDFF"` are accepted; stored/sent as uppercase without `#`. Invalid format (wrong length, non-hex) causes a hard error.

**Alpha note:** `0.95` rounds to `1.0` on device (10/10). Use `0.9` for 90% or `1.0` for full opacity.

---

### Widget Types and Their Fields

#### `type = "clock"`

| Field | Type | Default | Valid values |
|---|---|---|---|
| `time_format` | string | `"hh:mm:ss"` | `"hh:mm:ss"`, `"date"`, `"weekday"` |

#### `type = "metric"`

| Field | Type | Default | Notes |
|---|---|---|---|
| `source` | string | **REQUIRED** | See metric sources table below |
| `unit` | string | `""` | Max **5 UTF-8 bytes** (`"°C"` = 3 bytes, `"%"` = 1 byte) |
| `label` | string | `""` | Max **32 bytes** |
| `show_label` | bool | `false` | `true` = show `label value unit`; `false` = show `value unit` only |

**Metric sources and precision:**

| `source` value | Metric | Display precision | Suggested unit |
|---|---|---|---|
| `"cpu_temp"` | CPU temperature | 1 decimal (tenths) | `"°C"` |
| `"cpu_usage"` | CPU utilization | 1 decimal (tenths) | `"%"` |
| `"gpu_temp"` | GPU temperature | integer | `"°C"` |
| `"gpu_usage"` | GPU utilization | integer | `"%"` |
| `"mem_usage"` | Memory usage | integer | `"%"` |

Decimal precision is fixed per source by the firmware protocol; there is no per-widget decimal override.

#### `type = "image"`

| Field | Type | Default | Notes |
|---|---|---|---|
| `path` | string | `""` | Local file path. Only the **basename** is used on device. Basename max **150 bytes**. |

#### `type = "video"`

| Field | Type | Default | Notes |
|---|---|---|---|
| `path` | string | `""` | Local file path. Only the **basename** is used on device. Basename max **150 bytes**. |

Video is pushed to `/sdcard/` on the device. Playback loops automatically (`play_num = 1` is set internally).

#### `type = "text"`

| Field | Type | Default | Notes |
|---|---|---|---|
| `content` | string | `""` | Static text to display. Max **150 bytes** (multi-byte UTF-8 chars reduce limit). |

**Firmware quirk:** text content is transmitted via the `image_path` protocol field (not `num_text`). This is handled transparently by the library. The 150-byte limit (not 32) applies.

---

### Font Names

The `font` field accepts only these canonical values. Unknown values cause a hard error.

| `font` value | Description |
|---|---|
| `""` or `"default"` | Device default font |
| `"msyh"` | Microsoft YaHei |
| `"arial"` | Arial |
| `"impact"` | Impact |
| `"calibri"` | Calibri |
| `"georgia"` | Georgia |
| `"ni7seg"` | 7-segment numeric display font |
| `"harmonyos_black"` | HarmonyOS Sans Black |
| `"harmonyos_bold"` | HarmonyOS Sans Bold |
| `"harmonyos_light"` | HarmonyOS Sans Light |
| `"harmonyos_medium"` | HarmonyOS Sans Medium |
| `"harmonyos_thin"` | HarmonyOS Sans Thin |

Legacy TTF filenames (e.g. `"NI7SEG.TTF"`) are accepted and normalized automatically.

> Firmware note: `"harmonyos_bold"` maps to the firmware token `"harmonyos_blod"` (intentional typo in firmware). The library handles this transparently.

---

### Validation Constraints (Enforced at Runtime)

| Field | Limit | Error message |
|---|---|---|
| `color` | Exactly 6 hex digits | `"Invalid color: <value>"` |
| `alpha` | 0.0–1.0 | `"alpha must be 0.0–1.0"` |
| `x`, `y`, `width`, `height`, `text_size` | 0–65535 (u16) | `"'<field>' must be an integer 0–65535"` |
| `font` | Must be in canonical list | `"unsupported font '<name>': choose from fixed font selectors"` |
| `metric.unit` | ≤ 5 UTF-8 bytes | `"metric unit too long (N bytes, max 5): '<unit>'"` |
| `metric.label` | ≤ 32 bytes | `"metric label too long (N bytes, max 32): '<label>'"` |
| `image.path` basename | ≤ 150 bytes | `"image path basename too long (N bytes, max 150)"` |
| `video.path` basename | ≤ 150 bytes | `"video path basename too long (N bytes, max 150)"` |
| `text.content` | ≤ 150 bytes | `"text content too long (N bytes, max 150)"` |

---

### Complete Annotated Example

```toml
[meta]
name = "Dashboard"
description = "4-metric grid with video background"

# Full-screen video background
[[widget]]
type = "video"
path = "/home/user/.config/trv/themes/assets/bg.mp4"
x = 0
y = 0
width = 484
height = 480
alpha = 1.0

# Clock in top strip
[[widget]]
type = "clock"
time_format = "hh:mm:ss"    # "hh:mm:ss" | "date" | "weekday"
x = 128
y = 10
width = 228
height = 50
text_size = 40
color = "#E8FFE8"
alpha = 0.9
bold = true
font = "ni7seg"

# CPU temp -- top-left quadrant
[[widget]]
type = "metric"
source = "cpu_temp"         # REQUIRED; no default
unit = "°C"                 # max 5 UTF-8 bytes
label = "CPU"               # max 32 bytes
show_label = true
x = 12
y = 70
width = 230
height = 190
text_size = 64
color = "#00DDFF"
alpha = 1.0
bold = true

# GPU usage -- top-right quadrant
[[widget]]
type = "metric"
source = "gpu_usage"
unit = "%"
label = "GPU"
show_label = true
x = 250
y = 70
width = 222
height = 190
text_size = 64
color = "#FF4488"
alpha = 1.0
bold = true

# Static label text
[[widget]]
type = "text"
content = "SYSTEM MONITOR"   # max 150 bytes
x = 150
y = 440
width = 184
height = 34
text_size = 22
color = "#AAAAAA"
alpha = 0.8
font = "harmonyos_light"
```

---

## TRV Constraints (Always Apply)

- Device canvas is dynamic, query the device via `adb`, if not provided by user.
- Theme files are TOML and typically live in `~/.config/trv/themes/`.
- Image/video assets are usually in `~/.config/trv/themes/assets/`.
- If explicit metric labels are not desired, set `show_label = false` / `label = ""` and add dedicated `text` widgets as labels instead.

---

## Core Workflow

### 1) Understand Style + Scope

Extract:

- Theme name and visual direction
- Required metrics (exact set)
- Clock requirement
- Background type (image vs video)
- Font preference (for example `ni7seg`)
- Special constraints (contrast, no labels, grouping, color family)

If not provided, default to:

- readable high-contrast typography
- balanced composition
- one refinement pass after first screenshot

### 2) Prepare Background Asset

If user provides an asset, use it directly.

If creating one:

- Generate with Python/Pillow into `themes/assets/`.
- Keep native output at the detected resolution where possible.

If using video:

- Validate with `ffprobe`.
- For compatibility, transcode to safer profile when needed:

```bash
ffmpeg -y -i input.mp4 \
  -vf "scale=960:-2:flags=lanczos,fps=30" \
  -c:v libx264 -profile:v baseline -level 3.1 -pix_fmt yuv420p \
  -movflags +faststart -an output_compat.mp4
```

### 3) Build Theme TOML

Create/update a theme file under `themes/` with:

- one `image` or `video` background widget
- `clock` widget
- requested `metric` widgets
- optional `text` widgets for section headings/prompts

Practical layout rules:

- Keep key values out of busy/high-detail image zones.
- Avoid placing text over faces or bright highlights in character art.
- Align labels and values in clear columns/rows.
- Keep consistent text sizes for comparable metrics.

### 4) Validate Locally (Dry Run)

Always run:

```bash
cargo run -- daemon --theme themes/<name>.toml --dry-run --count 1
```

Confirm:

- theme parses without errors
- cmd3A frames are generated for each widget
- metric source list matches requested metrics

### 5) Validate On Device

Run full push + screenshot capture:

```bash
adb logcat -c && \
cargo run -- daemon --theme themes/<name>.toml --adb-forward --count 1 --log-level info && \
sleep 10 && \
adb exec-out screencap -p > /tmp/<theme>_v1.png && \
adb logcat -d -v time > /tmp/<theme>_v1.log
```

Then inspect screenshot and iterate.

### 6) Log Review Guidance

- Treat app-level transport failures as critical.
- For video themes, review logcat for:
  - `onError`
  - `Unsupported Video Resolution`
  - SELinux `avc: denied` entries
- `avc: denied` can be device-policy noise; only escalate if visual playback fails.

---

## `trv daemon` CLI Reference

Key flags for theme testing and iteration:

| Flag | Type | Default | Description |
|---|---|---|---|
| `--theme <path>` | path | config default | Path to theme TOML. Conflicts with `--preset`. |
| `--preset <name>` | string | — | Use a built-in preset (see `trv list`). Conflicts with `--theme`. |
| `--dry-run` | bool | false | Log frames without sending to device. Use for parse validation. |
| `--count <n>` | u32 | 0 | Metric update cycles (0 = infinite loop). Use `--count 1` for one-shot. |
| `--adb-forward` | bool | false | Run `adb forward tcp:PORT tcp:PORT` before connecting. |
| `--log-level <level>` | string | `"info"` | `error`, `warn`, `info`, `debug`, `trace` |
| `--host <ip>` | string | `"127.0.0.1"` | Device host (use with direct TCP, not ADB forward). |
| `--port <n>` | u16 | `22222` | Device TCP port. |
| `--interval <secs>` | float | `1.0` | Metric collection interval in seconds (min 0.1). |
| `--wake` | bool | false | Send cmd24 wake frame before theme push. |
| `--temp-offset <°C>` | float | `0.0` | Offset added to all temperature readings. |
| `--recv-timeout-ms <ms>` | u64 | `1000` | Per-frame receive timeout. |
| `--max-retries <n>` | u32 | 0 | Max consecutive errors before abort (0 = retry forever). |

---

## Protocol and Firmware Notes

These are handled by the library automatically. Useful for debugging unexpected device behavior.

- **Split-send:** one widget per AAF5 frame with ~50 ms delay between frames. First frame uses `theme_type=0x01` (clear + add); subsequent frames use `0x00` (append).
- **Text firmware quirk:** `type = "text"` content is encoded into the `image_path` protocol field (not `num_text`). The library handles this; the 150-byte content limit comes from `image_path`'s field size.
- **Color encoding:** stored as 6 ASCII hex characters in the protocol payload (not 3 binary bytes).
- **Metric precision:** fixed per `source` by the firmware's show-ID table. No per-widget decimal override exists.

---

## Polish Loop (Required For Creative Requests)

When user asks to "be creative" or "play around":

1. Produce first version.
2. Push + screenshot.
3. Refine color, spacing, and hierarchy.
4. Push + screenshot again.
5. Repeat until visually coherent and readable.

---

## Design Heuristics

- **Cozy / pastel themes:** low-contrast dark base + soft accent labels + warm value text.
- **Cyberpunk themes:** neon cyan/magenta/yellow with strong geometric alignment.
- **Anime/character themes:** avoid facial overlap; place metrics in side zones; use high contrast.
- **Terminal themes:** grid/column alignment, compact prompt labels, monospaced/segmented look (`ni7seg` font).

---

## Output Checklist

Before finishing, ensure:

- Theme file path is provided.
- Asset paths are provided.
- Screenshot paths are provided.
- Any known warnings are summarized with impact.
- Optional next refinement ideas are short and concrete.
