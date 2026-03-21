/// Memory usage metrics.
use sysinfo::System;

/// Read system memory usage as a percentage (0–100).
///
/// Uses `total_memory - available_memory` (not `used_memory`) to match the
/// Linux `MemAvailable` semantics from `/proc/meminfo`.
pub fn mem_usage(system: &System) -> Option<f64> {
    let total = system.total_memory();
    if total == 0 {
        return None;
    }
    let available = system.available_memory();
    let used = total.saturating_sub(available);
    let pct = (used as f64 / total as f64) * 100.0;
    Some(pct.clamp(0.0, 100.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysinfo::System;

    #[test]
    fn test_mem_usage_returns_valid_range() {
        let mut system = System::new();
        system.refresh_memory();
        if let Some(pct) = mem_usage(&system) {
            assert!(
                (0.0..=100.0).contains(&pct),
                "memory usage {} out of range",
                pct
            );
        }
        // None is acceptable if sysinfo can't read memory on this system
    }
}
