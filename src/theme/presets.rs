/// Built-in theme presets, embedded at compile time via `include_str!`.
///
/// Each preset is a TOML string that can be parsed into a [`Theme`].
/// The `ALL_PRESETS` slice maps human-readable names to TOML strings.
///
/// [`Theme`]: crate::theme::model::Theme

/// Raw TOML strings for each preset.
const DASHBOARD_TOML: &str = include_str!("../../presets/dashboard.toml");
const MINIMAL_TOML: &str = include_str!("../../presets/minimal.toml");
const CLOCK_METRICS_TOML: &str = include_str!("../../presets/clock_metrics.toml");
const CPU_GPU_TOML: &str = include_str!("../../presets/cpu_gpu.toml");
const ALL_METRICS_TOML: &str = include_str!("../../presets/all_metrics.toml");
const VIDEO_TOML: &str = include_str!("../../presets/video.toml");

/// All bundled presets as `(slug, toml_str)` pairs.
///
/// The slug is a lowercase identifier suitable for use on the command line
/// (e.g. `trv list` / `trv tui --preset dashboard`).
pub const ALL_PRESETS: &[(&str, &str)] = &[
    ("dashboard", DASHBOARD_TOML),
    ("minimal", MINIMAL_TOML),
    ("clock_metrics", CLOCK_METRICS_TOML),
    ("cpu_gpu", CPU_GPU_TOML),
    ("all_metrics", ALL_METRICS_TOML),
    ("video", VIDEO_TOML),
];

/// Return the TOML string for a preset by slug (case-insensitive), or `None`.
pub fn find_preset(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    ALL_PRESETS
        .iter()
        .find(|(slug, _)| *slug == lower)
        .map(|(_, toml)| *toml)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::toml::parse_theme_toml;

    #[test]
    fn all_presets_parse() {
        for (slug, toml) in ALL_PRESETS {
            parse_theme_toml(toml)
                .unwrap_or_else(|e| panic!("preset '{}' failed to parse: {}", slug, e));
        }
    }

    #[test]
    fn find_preset_case_insensitive() {
        assert!(find_preset("Dashboard").is_some());
        assert!(find_preset("MINIMAL").is_some());
        assert!(find_preset("nonexistent").is_none());
    }
}
