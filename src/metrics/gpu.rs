/// GPU temperature and usage metrics.
///
/// Strategy:
///   - Try nvidia-smi first (NVIDIA GPUs)
///   - Fall back to /sys/class/drm/card*/device/hwmon/hwmon*/temp*_input (temp)
///   - Fall back to /sys/class/drm/card*/device/gpu_busy_percent (usage, AMD)
use std::process::Command;
use std::sync::OnceLock;

/// Logs the first nvidia-smi query failure at `warn` level; subsequent
/// failures are suppressed to avoid log spam on machines without NVIDIA GPUs.
static NVIDIA_SMI_FAILURE_LOGGED: OnceLock<()> = OnceLock::new();

/// Combined GPU readings from a single batched nvidia-smi invocation.
#[derive(Debug, Default, Clone)]
pub struct GpuReadings {
    pub temp: Option<f64>,
    pub usage: Option<f64>,
}

/// Read GPU temperature in °C.
/// Returns `None` if no GPU or all methods fail.
pub fn gpu_temp() -> Option<f64> {
    // Try nvidia-smi first
    if let Some(v) = nvidia_smi_query("temperature.gpu")
        && (5.0..=130.0).contains(&v)
    {
        return Some(v);
    }

    sysfs_gpu_temp()
}

/// Read GPU temperature from sysfs hwmon sensors (no nvidia-smi).
///
/// Walks `/sys/class/drm/card*/device/hwmon/hwmon*/temp*_input` and returns
/// the maximum plausible value found. Used as a direct fallback path so that
/// `gpu_query_all` does not re-invoke `nvidia_smi_query` when it already
/// knows nvidia-smi is unavailable.
fn sysfs_gpu_temp() -> Option<f64> {
    let mut values: Vec<f64> = Vec::new();
    if let Ok(paths) = glob_drm_temps() {
        for p in paths {
            if let Ok(txt) = std::fs::read_to_string(&p)
                && let Some(v) = parse_millideg(txt.trim())
            {
                values.push(v);
            }
        }
    }
    values.into_iter().reduce(f64::max)
}

/// Read GPU utilization percentage (0–100).
///
/// Tries nvidia-smi first; falls back to the AMD sysfs
/// `gpu_busy_percent` interface for AMD GPUs.
pub fn gpu_usage() -> Option<f64> {
    if let Some(v) = nvidia_smi_query("utilization.gpu")
        && (0.0..=100.0).contains(&v)
    {
        return Some(v);
    }

    amd_gpu_usage()
}

/// AMD GPU utilization fallback via sysfs `gpu_busy_percent`.
///
/// Reads `/sys/class/drm/card*/device/gpu_busy_percent` and returns the
/// first plausible value found (0–100). Returns `None` if no file exists
/// or no valid value can be read.
fn amd_gpu_usage() -> Option<f64> {
    let drm_base = std::path::Path::new("/sys/class/drm");
    if !drm_base.exists() {
        return None;
    }
    let entries = std::fs::read_dir(drm_base).ok()?;
    for entry in entries.flatten() {
        let busy_path = entry.path().join("device/gpu_busy_percent");
        if let Ok(txt) = std::fs::read_to_string(&busy_path)
            && let Ok(v) = txt.trim().parse::<f64>()
            && (0.0..=100.0).contains(&v)
        {
            return Some(v);
        }
    }
    None
}

/// Query both GPU temperature and usage in a single nvidia-smi invocation.
///
/// Returns a `GpuReadings` struct with whichever fields could be read.
/// Avoids spawning two separate nvidia-smi processes when both are needed.
/// Falls back to sysfs for usage if nvidia-smi is unavailable.
pub fn gpu_query_all() -> GpuReadings {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=temperature.gpu,utilization.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        Ok(_) | Err(_) => {
            // nvidia-smi not available or failed — log once, then fall back.
            NVIDIA_SMI_FAILURE_LOGGED.get_or_init(|| {
                eprintln!("[trv] nvidia-smi unavailable for gpu_query_all — using sysfs fallbacks");
            });
            // Use sysfs_gpu_temp() directly to avoid re-invoking nvidia_smi_query.
            return GpuReadings {
                temp: sysfs_gpu_temp(),
                usage: amd_gpu_usage(),
            };
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = match stdout.trim().lines().next() {
        Some(l) => l.trim(),
        None => return GpuReadings::default(),
    };

    // Expected format: "temp, usage"  e.g. "46, 15"
    let mut parts = line.splitn(2, ',');
    let temp_str = parts.next().map(str::trim).unwrap_or("");
    let usage_str = parts.next().map(str::trim).unwrap_or("");

    let temp = temp_str
        .parse::<f64>()
        .ok()
        .filter(|v| (5.0..=130.0).contains(v));
    let usage = usage_str
        .parse::<f64>()
        .ok()
        .filter(|v| (0.0..=100.0).contains(v));

    GpuReadings { temp, usage }
}

/// Run `nvidia-smi --query-gpu=<field> --format=csv,noheader,nounits`
/// and return the parsed f64 value, or `None` on any failure.
///
/// The first failure is logged once via `NVIDIA_SMI_FAILURE_LOGGED`; subsequent
/// failures are silent to avoid log spam on machines without NVIDIA GPUs.
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
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        Ok(_) | Err(_) => {
            NVIDIA_SMI_FAILURE_LOGGED.get_or_init(|| {
                eprintln!("[trv] nvidia-smi unavailable — GPU metrics via nvidia-smi will not be collected");
            });
            return None;
        }
    };

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
                if let Some(name) = sensor.file_name().and_then(|n| n.to_str())
                    && name.starts_with("temp")
                    && name.ends_with("_input")
                {
                    paths.push(sensor);
                }
            }
        }
    }
    Ok(paths)
}

/// Parse a sysfs temperature value in millidegrees (thousandths of °C).
///
/// The Linux `hwmon` sysfs `temp*_input` files always report in millidegrees
/// (e.g. `46000` = 46°C). Values outside the plausible sensor range are
/// rejected.
fn parse_millideg(raw: &str) -> Option<f64> {
    let v: f64 = raw.parse().ok()?;
    let celsius = v / 1000.0;
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
    fn test_parse_millideg_out_of_range() {
        // 46 millidegrees = 0.046°C — too cold, rejected
        assert_eq!(parse_millideg("46"), None);
        // 200000 millidegrees = 200°C — too hot, rejected
        assert_eq!(parse_millideg("200000"), None);
        // 1000 millidegrees = 1°C — too cold, rejected
        assert_eq!(parse_millideg("1000"), None);
    }
}
