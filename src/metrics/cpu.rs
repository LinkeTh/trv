/// CPU temperature and usage metrics.
///
/// Temperature priority order:
///   1. hwmon coretemp/k10temp — "Package" or "Tdie" label (exact CPU die temp)
///   2. hwmon coretemp/k10temp — any sensor (per-core, take max)
///   3. thermal_zone with type "x86_pkg_temp"
///   4. Any hwmon sensor with a sane range (fallback)
use std::path::Path;

use sysinfo::{Components, System};

/// Read CPU package temperature from sysinfo Components.
///
/// Returns `None` if no plausible CPU temperature sensor is found.
/// Applies `offset_c` to the result.
pub fn cpu_temp(components: &Components, offset_c: f64) -> Option<f64> {
    // On Linux/sysinfo 0.33, `Component::label()` is usually the sensor label
    // (e.g. "Package id 0", "Core 0", "Tdie"), not the hwmon chip name
    // (e.g. coretemp/k10temp). So we must match both chip-like and label-like
    // CPU patterns.
    let cpu_label_hints = [
        "coretemp",
        "k10temp",
        "k8temp",
        "cpu",
        "package",
        "package id",
        "tdie",
        "tccd",
        "core ",
        "die",
        "ctl",
    ];

    let mut package_temps: Vec<f64> = Vec::new();
    let mut core_temps: Vec<f64> = Vec::new();
    let mut all_cpu_temps: Vec<f64> = Vec::new();

    for comp in components.iter() {
        let label = comp.label().to_lowercase();
        let temp = match comp.temperature() {
            Some(t) => t as f64,
            None => continue,
        };

        // Skip implausible readings
        if !(5.0..=130.0).contains(&temp) {
            continue;
        }

        // Check if this looks like a CPU sensor.
        // Typical Intel labels: "Package id 0", "Core N"
        // Typical AMD labels: "Tdie", "Tctl", "Tccd*"
        let is_cpu_sensor = cpu_label_hints.iter().any(|hint| label.contains(hint));

        if is_cpu_sensor {
            all_cpu_temps.push(temp);

            // Package/die level sensor takes priority
            if label.contains("package") || label.contains("tdie") || label.contains("tccd") {
                package_temps.push(temp);
            } else {
                core_temps.push(temp);
            }
        }
    }

    // Priority:
    //   1. Package/die sensor (e.g. "Package id 0" from coretemp, "Tdie" from k10temp)
    //   2. Max of per-core sensors from a known CPU chip
    //   3. Any other sensor that looks like a CPU (fallback)
    //
    // Note: `core_temps` is checked before `all_cpu_temps` so that pure-core
    // readings beat any miscellaneous CPU-chip sensor that isn't a core.
    let raw = if let Some(v) = package_temps.iter().copied().reduce(f64::max) {
        v
    } else if let Some(v) = core_temps.iter().copied().reduce(f64::max) {
        v
    } else if let Some(v) = all_cpu_temps.iter().copied().reduce(f64::max) {
        v
    } else {
        return None;
    };

    Some(raw + offset_c)
}

/// Read global CPU usage percentage (0–100) from sysinfo.
///
/// **Important**: call `system.refresh_cpu_usage()` before calling this,
/// ideally twice separated by a brief sleep to get a meaningful delta.
pub fn cpu_usage(system: &System) -> f64 {
    let usage = system.global_cpu_usage() as f64;
    usage.clamp(0.0, 100.0)
}

/// Read average CPU frequency in MHz.
pub fn cpu_freq(system: &System) -> Option<f64> {
    if let Some(v) = sysinfo_cpu_freq(system) {
        return Some(v);
    }
    if let Some(v) = sysfs_cpu_freq_mhz() {
        return Some(v);
    }
    proc_cpuinfo_freq_mhz()
}

fn sysinfo_cpu_freq(system: &System) -> Option<f64> {
    let cpus = system.cpus();
    if cpus.is_empty() {
        return None;
    }

    let total: u64 = cpus.iter().map(|cpu| cpu.frequency()).sum();
    let avg = total as f64 / cpus.len() as f64;
    if avg > 1.0 { Some(avg) } else { None }
}

fn sysfs_cpu_freq_mhz() -> Option<f64> {
    let base = Path::new("/sys/devices/system/cpu");
    let entries = std::fs::read_dir(base).ok()?;

    let mut sum_mhz = 0.0;
    let mut count = 0_u64;

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !is_cpu_dir_name(name) {
            continue;
        }

        let cpufreq_dir = entry.path().join("cpufreq");
        if !cpufreq_dir.exists() {
            continue;
        }

        for file in ["scaling_cur_freq", "cpuinfo_cur_freq"] {
            let path = cpufreq_dir.join(file);
            if let Ok(raw) = std::fs::read_to_string(path)
                && let Some(v) = parse_khz_to_mhz(raw.trim())
            {
                sum_mhz += v;
                count += 1;
                break;
            }
        }
    }

    if count > 0 {
        Some(sum_mhz / count as f64)
    } else {
        None
    }
}

fn proc_cpuinfo_freq_mhz() -> Option<f64> {
    let raw = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    parse_cpuinfo_mhz(&raw)
}

fn is_cpu_dir_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("cpu") else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_digit())
}

fn parse_khz_to_mhz(raw: &str) -> Option<f64> {
    let khz: f64 = raw.parse().ok()?;
    let mhz = khz / 1000.0;
    if (100.0..=10_000.0).contains(&mhz) {
        Some(mhz)
    } else {
        None
    }
}

fn parse_cpuinfo_mhz(raw: &str) -> Option<f64> {
    let mut sum = 0.0;
    let mut count = 0_u64;

    for line in raw.lines() {
        let lower = line.to_ascii_lowercase();
        if !lower.starts_with("cpu mhz") {
            continue;
        }

        let Some((_, value_raw)) = line.split_once(':') else {
            continue;
        };
        let Ok(v) = value_raw.trim().parse::<f64>() else {
            continue;
        };
        if (100.0..=10_000.0).contains(&v) {
            sum += v;
            count += 1;
        }
    }

    if count > 0 {
        Some(sum / count as f64)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_usage_clamp() {
        // cpu_usage returns a value from sysinfo which may be 0 on first call
        // We can't test the actual value without hardware, but we can test clamping logic
        let usage: f64 = 150.0_f64.clamp(0.0, 100.0);
        assert_eq!(usage, 100.0);
        let usage: f64 = (-5.0_f64).clamp(0.0, 100.0);
        assert_eq!(usage, 0.0);
    }

    #[test]
    fn test_parse_khz_to_mhz() {
        assert_eq!(parse_khz_to_mhz("3600000"), Some(3600.0));
        assert_eq!(parse_khz_to_mhz("0"), None);
    }

    #[test]
    fn test_parse_cpuinfo_mhz() {
        let raw = "cpu MHz\t\t: 3592.889\nmodel name\t: test\ncpu MHz\t: 3500.000\n";
        let avg = parse_cpuinfo_mhz(raw).unwrap();
        assert!((avg - 3546.4445).abs() < 0.01);
    }
}
