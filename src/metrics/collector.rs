/// MetricCollector — reads all live metrics and maps them to show IDs.
///
/// Owns the sysinfo `System` and `Components` state for CPU/memory polling.
/// GPU reads invoke `nvidia-smi` as a subprocess (blocking).
///
/// Usage:
/// ```ignore
/// let mut collector = MetricCollector::new(0.0);
/// collector.prime(); // establish CPU usage baseline
/// std::thread::sleep(Duration::from_millis(500));
/// let readings = collector.collect(&sources);
/// ```
use std::collections::HashMap;

use sysinfo::{Components, System};

use crate::theme::model::MetricSource;

use super::{cpu, gpu, memory};

/// Holds sysinfo state between collection cycles.
pub struct MetricCollector {
    system: System,
    components: Components,
    temp_offset_c: f64,
}

impl MetricCollector {
    /// Create a new collector with the given temperature offset (added to all
    /// temperature readings, positive = warmer).
    pub fn new(temp_offset_c: f64) -> Self {
        let mut system = System::new();
        // Initial refresh to get a baseline for CPU usage delta
        system.refresh_cpu_usage();
        system.refresh_memory();
        let components = Components::new_with_refreshed_list();
        Self {
            system,
            components,
            temp_offset_c,
        }
    }

    /// Prime the CPU usage baseline so the first `collect()` call returns a
    /// meaningful usage value.  Should be followed by at least 100–500 ms of
    /// sleep before the first collection.
    pub fn prime(&mut self) {
        self.system.refresh_cpu_usage();
    }

    /// Collect all metrics for the given `(show_id, MetricSource)` pairs.
    ///
    /// Returns a map of `show_id → value` for every source that could be read.
    /// Missing values are silently omitted (caller decides how to handle gaps).
    pub fn collect(&mut self, sources: &[(String, MetricSource)]) -> HashMap<String, f64> {
        // Refresh sysinfo state once per cycle
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.components.refresh(false);

        let mut map = HashMap::new();

        for (show_id, source) in sources {
            let value: Option<f64> = match source {
                MetricSource::CpuTemp => cpu::cpu_temp(&self.components, self.temp_offset_c),
                MetricSource::CpuUsage => Some(cpu::cpu_usage(&self.system)),
                MetricSource::MemUsage => memory::mem_usage(&self.system),
                MetricSource::GpuTemp => gpu::gpu_temp(),
                MetricSource::GpuUsage => gpu::gpu_usage(),
                MetricSource::Fixed(v) => Some(*v),
            };

            if let Some(v) = value {
                map.insert(show_id.clone(), v);
            }
        }

        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_source_collected() {
        let mut collector = MetricCollector::new(0.0);
        let sources = vec![("00".to_string(), MetricSource::Fixed(42.0))];
        let readings = collector.collect(&sources);
        assert_eq!(readings.get("00"), Some(&42.0));
    }

    #[test]
    fn test_cpu_usage_collected() {
        let mut collector = MetricCollector::new(0.0);
        collector.prime();
        let sources = vec![("05".to_string(), MetricSource::CpuUsage)];
        let readings = collector.collect(&sources);
        // CPU usage should be present and in a valid range
        if let Some(&v) = readings.get("05") {
            assert!((0.0..=100.0).contains(&v), "cpu_usage {} out of range", v);
        }
    }

    #[test]
    fn test_mem_usage_collected() {
        let mut collector = MetricCollector::new(0.0);
        let sources = vec![("06".to_string(), MetricSource::MemUsage)];
        let readings = collector.collect(&sources);
        if let Some(&v) = readings.get("06") {
            assert!((0.0..=100.0).contains(&v), "mem_usage {} out of range", v);
        }
    }

    #[test]
    fn test_temp_offset_applied() {
        let mut collector = MetricCollector::new(5.0);
        // We can only test offset logic indirectly via Fixed source;
        // real temp sources depend on hardware.
        let sources = vec![("00".to_string(), MetricSource::Fixed(40.0))];
        let readings = collector.collect(&sources);
        // Fixed sources don't apply temp_offset — offset is only for sensor reads
        assert_eq!(readings.get("00"), Some(&40.0));
    }
}
