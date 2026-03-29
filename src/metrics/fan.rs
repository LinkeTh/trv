/// Fan speed and liquid temperature metrics.
use std::path::Path;

/// Read first plausible fan speed (RPM) from sysfs hwmon.
pub fn fan_speed_rpm() -> Option<f64> {
    let hwmon_base = Path::new("/sys/class/hwmon");
    if !hwmon_base.exists() {
        return None;
    }

    let entries = std::fs::read_dir(hwmon_base).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let sensors = match std::fs::read_dir(&path) {
            Ok(v) => v,
            Err(_) => continue,
        };
        for sensor in sensors.flatten() {
            let sensor_path = sensor.path();
            let Some(name) = sensor_path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.starts_with("fan") || !name.ends_with("_input") {
                continue;
            }
            if let Ok(txt) = std::fs::read_to_string(&sensor_path)
                && let Ok(v) = txt.trim().parse::<f64>()
                && (0.0..=20_000.0).contains(&v)
            {
                return Some(v);
            }
        }
    }
    None
}

/// Read liquid/water temperature in Celsius from sysfs hwmon.
///
/// Uses label hints (`liquid`, `water`) to avoid returning unrelated sensors.
pub fn liquid_temp_c() -> Option<f64> {
    let hwmon_base = Path::new("/sys/class/hwmon");
    if !hwmon_base.exists() {
        return None;
    }

    let entries = std::fs::read_dir(hwmon_base).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let sensors = match std::fs::read_dir(&path) {
            Ok(v) => v,
            Err(_) => continue,
        };
        for sensor in sensors.flatten() {
            let sensor_path = sensor.path();
            let Some(name) = sensor_path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.starts_with("temp") || !name.ends_with("_label") {
                continue;
            }

            let label = match std::fs::read_to_string(&sensor_path) {
                Ok(v) => v.trim().to_ascii_lowercase(),
                Err(_) => continue,
            };
            if !(label.contains("liquid") || label.contains("water")) {
                continue;
            }

            let input_name = name.replace("_label", "_input");
            let input_path = path.join(input_name);
            if let Ok(raw) = std::fs::read_to_string(input_path)
                && let Some(temp) = parse_millideg(raw.trim())
            {
                return Some(temp);
            }
        }
    }

    None
}

fn parse_millideg(raw: &str) -> Option<f64> {
    let value: f64 = raw.parse().ok()?;
    let celsius = value / 1000.0;
    if (0.0..=130.0).contains(&celsius) {
        Some(celsius)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_millideg_valid() {
        assert_eq!(parse_millideg("42000"), Some(42.0));
        assert_eq!(parse_millideg("75500"), Some(75.5));
    }

    #[test]
    fn test_parse_millideg_invalid() {
        assert_eq!(parse_millideg("-1000"), None);
        assert_eq!(parse_millideg("200000"), None);
        assert_eq!(parse_millideg("abc"), None);
    }
}
