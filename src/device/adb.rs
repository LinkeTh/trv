/// ADB (Android Debug Bridge) helpers for the TRV LCD device.
///
/// The app runs on an Android device. To communicate over TCP from the host,
/// ADB must forward a local port to the device's TCP port.
///
/// Command:  `adb forward tcp:<port> tcp:<port>`
use std::process::Command;
use std::time::Duration;

/// Run `adb forward tcp:<port> tcp:<port>` to set up port forwarding.
///
/// Returns `true` if the command succeeded, `false` otherwise.
/// Failures are silently ignored by callers.
pub fn adb_forward(port: u16) -> bool {
    let port_str = port.to_string();
    let tcp_arg = format!("tcp:{}", port_str);
    match Command::new("adb")
        .args(["forward", &tcp_arg, &tcp_arg])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut c| c.wait())
    {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}

/// Push a local file to the device via ADB.
///
/// `local_path` — absolute path on the host.
/// `remote_path` — path on the device (e.g. `/sdcard/background.jpg`).
pub fn adb_push(local_path: &str, remote_path: &str) -> bool {
    match Command::new("adb")
        .args(["push", local_path, remote_path])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut c| c.wait())
    {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}

/// Check if `adb` is available in PATH.
pub fn adb_available() -> bool {
    Command::new("adb")
        .arg("version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Default inter-frame delay for split cmd3A sends (50 ms).
pub const INTER_FRAME_DELAY: Duration = Duration::from_millis(50);
