/// GPU temperature and usage metrics.
///
/// Strategy:
///   - Try nvidia-smi first (NVIDIA GPUs)
///   - Fall back to /sys/class/drm/card*/device/hwmon/hwmon*/temp*_input
use std::process::Command;

/// Read GPU temperature in °C.
/// Returns `None` if no GPU or all methods fail.
pub fn gpu_temp() -> Option<f64> {
    // Try nvidia-smi first
    if let Some(v) = nvidia_smi_query("temperature.gpu") {
        if (5.0..=130.0).contains(&v) {
            return Some(v);
        }
    }

    // Fallback: max of all DRM card hwmon temp sensors
    let mut values: Vec<f64> = Vec::new();
    if let Ok(paths) = glob_drm_temps() {
        for p in paths {
            if let Ok(txt) = std::fs::read_to_string(&p) {
                if let Some(v) = parse_millideg(txt.trim()) {
                    values.push(v);
                }
            }
        }
    }
    values.into_iter().reduce(f64::max)
}

/// Read GPU utilization percentage (0–100).
/// Returns `None` if no GPU or nvidia-smi not available.
pub fn gpu_usage() -> Option<f64> {
    let v = nvidia_smi_query("utilization.gpu")?;
    if (0.0..=100.0).contains(&v) {
        Some(v)
    } else {
        None
    }
}

/// Run `nvidia-smi --query-gpu=<field> --format=csv,noheader,nounits`
/// and return the parsed f64 value, or `None` on any failure.
///
/// NOTE: This is a blocking subprocess call (~50-200ms). It is called from the
/// synchronous `MetricCollector::collect()`, which is invoked from the async daemon
/// loop. The Tokio runtime is configured with `rt-multi-thread`, so this blocks one
/// worker thread but will not stall the whole runtime. Acceptable for 1 Hz metrics.
fn nvidia_smi_query(field: &str) -> Option<f64> {
    let output = Command::new("nvidia-smi")
        .args([
            &format!("--query-gpu={}", field),
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.trim().lines().next()?.trim();
    line.parse::<f64>().ok()
}

/// Collect paths matching `/sys/class/drm/card*/device/hwmon/hwmon*/temp*_input`.
fn glob_drm_temps() -> Result<Vec<std::path::PathBuf>, std::io::Error> {
    let mut paths = Vec::new();
    // Walk /sys/class/drm/
    let drm_base = std::path::Path::new("/sys/class/drm");
    if !drm_base.exists() {
        return Ok(paths);
    }
    for card_entry in std::fs::read_dir(drm_base)? {
        let card = card_entry?.path();
        let hwmon_dir = card.join("device/hwmon");
        if !hwmon_dir.exists() {
            continue;
        }
        for hwmon_entry in std::fs::read_dir(&hwmon_dir)? {
            let hwmon = hwmon_entry?.path();
            for sensor_entry in std::fs::read_dir(&hwmon)? {
                let sensor = sensor_entry?.path();
                if let Some(name) = sensor.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("temp") && name.ends_with("_input") {
                        paths.push(sensor);
                    }
                }
            }
        }
    }
    Ok(paths)
}

/// Parse a sysfs temperature value (may be in millidegrees or degrees).
fn parse_millideg(raw: &str) -> Option<f64> {
    let v: f64 = raw.parse().ok()?;
    // Values > 1000 are millidegrees
    let celsius = if v > 1000.0 { v / 1000.0 } else { v };
    if (5.0..=130.0).contains(&celsius) {
        Some(celsius)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_millideg_normal() {
        // sysfs value in millidegrees
        assert_eq!(parse_millideg("46000"), Some(46.0));
        assert_eq!(parse_millideg("75500"), Some(75.5));
    }

    #[test]
    fn test_parse_millideg_direct() {
        // Direct degrees (rare but possible)
        assert_eq!(parse_millideg("46"), Some(46.0));
    }

    #[test]
    fn test_parse_millideg_out_of_range() {
        // Too cold or too hot
        assert_eq!(parse_millideg("1000"), None); // 1°C if direct, or 1.0°C if millideg — borderline
        assert_eq!(parse_millideg("200000"), None); // 200°C — too hot
        assert_eq!(parse_millideg("1000"), None);
    }
}
