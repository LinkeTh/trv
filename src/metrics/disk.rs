/// Disk IO throughput metrics.
use std::time::Duration;

use sysinfo::Disks;

const BYTES_PER_KILOBYTE: f64 = 1_000.0;

/// Aggregate disk read throughput across all disks in KB/s.
pub fn disk_read_kb_per_s(disks: &Disks, elapsed: Duration) -> Option<f64> {
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return None;
    }

    let bytes: u64 = disks
        .list()
        .iter()
        .map(|disk| disk.usage().read_bytes)
        .fold(0_u64, |acc, v| acc.saturating_add(v));

    Some((bytes as f64 / BYTES_PER_KILOBYTE) / seconds)
}

/// Aggregate disk write throughput across all disks in KB/s.
pub fn disk_write_kb_per_s(disks: &Disks, elapsed: Duration) -> Option<f64> {
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return None;
    }

    let bytes: u64 = disks
        .list()
        .iter()
        .map(|disk| disk.usage().written_bytes)
        .fold(0_u64, |acc, v| acc.saturating_add(v));

    Some((bytes as f64 / BYTES_PER_KILOBYTE) / seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_elapsed_returns_none() {
        let disks = Disks::new();
        assert!(disk_read_kb_per_s(&disks, Duration::from_secs(0)).is_none());
        assert!(disk_write_kb_per_s(&disks, Duration::from_secs(0)).is_none());
    }

    #[test]
    fn test_empty_disks_returns_zero_rate() {
        let disks = Disks::new();
        let read = disk_read_kb_per_s(&disks, Duration::from_secs(1)).unwrap();
        let write = disk_write_kb_per_s(&disks, Duration::from_secs(1)).unwrap();
        assert_eq!(read, 0.0);
        assert_eq!(write, 0.0);
    }
}
