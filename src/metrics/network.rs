/// Network throughput metrics.
use std::time::Duration;

use sysinfo::Networks;

const BYTES_PER_KILOBYTE: f64 = 1_000.0;

/// Aggregate download throughput across all interfaces in KB/s.
pub fn net_down_kb_per_s(networks: &Networks, elapsed: Duration) -> Option<f64> {
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return None;
    }

    let bytes: u64 = networks
        .values()
        .map(|data| data.received())
        .fold(0_u64, |acc, v| acc.saturating_add(v));

    let kb = (bytes as f64 / BYTES_PER_KILOBYTE) / seconds;
    // Clamp small positive rates to 1 KB/s so the device shows activity
    // instead of rounding to 0.
    Some(if kb > 0.0 && kb < 1.0 { 1.0 } else { kb })
}

/// Aggregate upload throughput across all interfaces in KB/s.
pub fn net_up_kb_per_s(networks: &Networks, elapsed: Duration) -> Option<f64> {
    let seconds = elapsed.as_secs_f64();
    if seconds <= 0.0 {
        return None;
    }

    let bytes: u64 = networks
        .values()
        .map(|data| data.transmitted())
        .fold(0_u64, |acc, v| acc.saturating_add(v));

    let kb = (bytes as f64 / BYTES_PER_KILOBYTE) / seconds;
    Some(if kb > 0.0 && kb < 1.0 { 1.0 } else { kb })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_elapsed_returns_none() {
        let networks = Networks::new();
        assert!(net_down_kb_per_s(&networks, Duration::from_secs(0)).is_none());
        assert!(net_up_kb_per_s(&networks, Duration::from_secs(0)).is_none());
    }

    #[test]
    fn test_empty_networks_returns_zero_rate() {
        let networks = Networks::new();
        let down = net_down_kb_per_s(&networks, Duration::from_secs(1)).unwrap();
        let up = net_up_kb_per_s(&networks, Duration::from_secs(1)).unwrap();
        assert_eq!(down, 0.0);
        assert_eq!(up, 0.0);
    }
}
