/// MetricCollector — reads all live metrics and maps them to show IDs.
///
/// Owns the sysinfo `System` and `Components` state for CPU/memory polling.
/// Also owns `Networks` and `Disks` state for throughput collection.
/// GPU reads invoke `nvidia-smi` as a subprocess (blocking).
///
/// When multiple GPU metrics are requested, a single batched
/// `gpu_query_all()` call is used instead of two separate invocations.
///
/// Usage:
/// ```ignore
/// let mut collector = MetricCollector::new(0.0);
/// collector.prime(); // establish CPU usage baseline
/// std::thread::sleep(Duration::from_millis(500));
/// let readings = collector.collect(&sources);
/// ```
use std::collections::HashMap;
use std::time::Instant;

use sysinfo::{Components, DiskRefreshKind, Disks, Networks, System};

use crate::theme::model::MetricSource;

use super::{cpu, disk, fan, gpu, memory, network};

/// Holds sysinfo state between collection cycles.
pub struct MetricCollector {
    system: System,
    components: Components,
    networks: Networks,
    disks: Disks,
    temp_offset_c: f64,
    last_collect_at: Instant,
}

impl MetricCollector {
    /// Create a new collector with the given temperature offset (added to all
    /// temperature readings, positive = warmer).
    pub fn new(temp_offset_c: f64) -> Self {
        let mut system = System::new();
        // Initial refresh to get a baseline for CPU usage delta
        system.refresh_cpu_usage();
        system.refresh_cpu_frequency();
        system.refresh_memory();
        let components = Components::new_with_refreshed_list();
        let mut networks = Networks::new_with_refreshed_list();
        networks.refresh(true);
        let mut disks = Disks::new_with_refreshed_list();
        disks.refresh_specifics(true, DiskRefreshKind::nothing().with_io_usage());
        Self {
            system,
            components,
            networks,
            disks,
            temp_offset_c,
            last_collect_at: Instant::now(),
        }
    }

    /// Prime the CPU usage baseline so the first `collect()` call returns a
    /// meaningful usage value.  Should be followed by at least 100–500 ms of
    /// sleep before the first collection.
    pub fn prime(&mut self) {
        self.system.refresh_cpu_usage();
        self.last_collect_at = Instant::now();
    }

    /// Collect all metrics for the given `(show_id, MetricSource)` pairs.
    ///
    /// Returns a map of `show_id → value` for every source that could be read.
    /// Missing values are silently omitted (caller decides how to handle gaps).
    ///
    /// When any two or more GPU metrics are requested, a single batched
    /// nvidia-smi call (`gpu_query_all`) is used to avoid spawning multiple
    /// processes per collection cycle.
    pub fn collect(&mut self, sources: &[(String, MetricSource)]) -> HashMap<String, f64> {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_collect_at);
        self.last_collect_at = now;

        // Refresh sysinfo state once per cycle
        self.system.refresh_cpu_usage();
        self.system.refresh_cpu_frequency();
        self.system.refresh_memory();
        self.components.refresh(false);

        let needs_network = sources
            .iter()
            .any(|(_, s)| matches!(s, MetricSource::NetDown | MetricSource::NetUp));
        if needs_network {
            self.networks.refresh(true);
        }

        let needs_disk = sources
            .iter()
            .any(|(_, s)| matches!(s, MetricSource::DiskRead | MetricSource::DiskWrite));
        if needs_disk {
            self.disks
                .refresh_specifics(true, DiskRefreshKind::nothing().with_io_usage());
        }

        // Determine whether to batch GPU queries.
        let needs_gpu_temp = sources.iter().any(|(_, s)| *s == MetricSource::GpuTemp);
        let needs_gpu_usage = sources.iter().any(|(_, s)| *s == MetricSource::GpuUsage);
        let needs_gpu_freq = sources.iter().any(|(_, s)| *s == MetricSource::GpuFreq);
        let gpu_metrics_requested = [needs_gpu_temp, needs_gpu_usage, needs_gpu_freq]
            .into_iter()
            .filter(|v| *v)
            .count();
        let gpu_readings = if gpu_metrics_requested >= 2 {
            Some(gpu::gpu_query_all())
        } else {
            None
        };

        let mut map = HashMap::new();

        for (show_id, source) in sources {
            let value: Option<f64> = match source {
                MetricSource::CpuTemp => cpu::cpu_temp(&self.components, self.temp_offset_c),
                MetricSource::CpuFreq => cpu::cpu_freq(&self.system),
                MetricSource::CpuUsage => Some(cpu::cpu_usage(&self.system)),
                MetricSource::MemUsage => memory::mem_usage(&self.system),
                MetricSource::GpuTemp => {
                    if let Some(ref r) = gpu_readings {
                        r.temp
                    } else {
                        gpu::gpu_temp()
                    }
                }
                MetricSource::GpuUsage => {
                    if let Some(ref r) = gpu_readings {
                        r.usage
                    } else {
                        gpu::gpu_usage()
                    }
                }
                MetricSource::GpuFreq => {
                    if let Some(ref r) = gpu_readings {
                        r.freq
                    } else {
                        gpu::gpu_freq()
                    }
                }
                MetricSource::FanSpeed => fan::fan_speed_rpm(),
                MetricSource::LiquidTemp => fan::liquid_temp_c().map(|v| v + self.temp_offset_c),
                MetricSource::NetDown => network::net_down_kb_per_s(&self.networks, elapsed),
                MetricSource::NetUp => network::net_up_kb_per_s(&self.networks, elapsed),
                MetricSource::DiskRead => disk::disk_read_kb_per_s(&self.disks, elapsed),
                MetricSource::DiskWrite => disk::disk_write_kb_per_s(&self.disks, elapsed),
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
        // Real temp sources depend on hardware; this just exercises the path.
        let sources = vec![("00".to_string(), MetricSource::CpuTemp)];
        let readings = collector.collect(&sources);
        if let Some(&v) = readings.get("00") {
            assert!((5.0..=135.0).contains(&v), "cpu_temp {} out of range", v);
        }
    }
}
