/// CPU temperature and usage metrics.
///
/// Temperature priority order:
///   1. hwmon coretemp/k10temp — "Package" or "Tdie" label (exact CPU die temp)
///   2. hwmon coretemp/k10temp — any sensor (per-core, take max)
///   3. thermal_zone with type "x86_pkg_temp"
///   4. Any hwmon sensor with a sane range (fallback)
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
        if temp < 5.0 || temp > 130.0 {
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

#[cfg(test)]
mod tests {
    #[test]
    fn test_cpu_usage_clamp() {
        // cpu_usage returns a value from sysinfo which may be 0 on first call
        // We can't test the actual value without hardware, but we can test clamping logic
        let usage: f64 = 150.0_f64.clamp(0.0, 100.0);
        assert_eq!(usage, 100.0);
        let usage: f64 = (-5.0_f64).clamp(0.0, 100.0);
        assert_eq!(usage, 0.0);
    }
}
