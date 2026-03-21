/// Daemon configuration, parsed from CLI arguments.
use std::path::PathBuf;

/// All settings that control a daemon run.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// Path to the theme TOML file.
    pub theme_path: PathBuf,

    /// Device host (default: 127.0.0.1 via ADB forward).
    pub host: String,

    /// Device TCP port.
    pub port: u16,

    /// If true, log frames without sending to device.
    pub dry_run: bool,

    /// Number of cmd15 cycles to send (0 = infinite).
    pub count: u32,

    /// Metric collection interval in seconds.
    pub interval_s: f64,

    /// Temperature offset (°C) added to all sensor reads.
    pub temp_offset_c: f64,

    /// If true, run `adb forward tcp:<port> tcp:<port>` before connecting.
    pub adb_forward: bool,

    /// If true, send cmd24 wake-on before setup sequence.
    pub send_wake: bool,

    /// Receive timeout per frame in milliseconds.
    pub recv_timeout_ms: u64,

    /// Maximum consecutive errors before aborting (0 = infinite retries).
    pub max_retries: u32,
}

impl DaemonConfig {
    /// Resolve the fallback theme path: `~/.config/trv/themes/dashboard.toml`.
    pub fn default_theme_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("trv")
            .join("themes")
            .join("dashboard.toml")
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            theme_path: Self::default_theme_path(),
            host: "127.0.0.1".to_string(),
            port: 22222,
            dry_run: false,
            count: 0,
            interval_s: 1.0,
            temp_offset_c: 0.0,
            adb_forward: false,
            send_wake: false,
            recv_timeout_ms: 1000,
            max_retries: 0,
        }
    }
}
