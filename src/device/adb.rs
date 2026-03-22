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
    if !is_safe_adb_arg(local_path) || !is_safe_adb_arg(remote_path) {
        return false;
    }

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

/// Run `adb shell settings put system <key> <value>`.
///
/// Returns `true` on successful command exit status.
pub fn adb_settings_put_system(key: &str, value: &str) -> bool {
    if !is_safe_adb_arg(key) || !is_safe_adb_arg(value) {
        return false;
    }

    match Command::new("adb")
        .args(["shell", "settings", "put", "system", key, value])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut c| c.wait())
    {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}

/// Default inter-frame delay for split cmd3A sends (50 ms).
pub const INTER_FRAME_DELAY: Duration = Duration::from_millis(50);

fn is_safe_adb_arg(arg: &str) -> bool {
    let trimmed = arg.trim();
    !trimmed.is_empty() && !trimmed.starts_with('-')
}

#[cfg(test)]
mod tests {
    use super::is_safe_adb_arg;

    #[test]
    fn adb_arg_rejects_empty_and_flag_like_values() {
        assert!(!is_safe_adb_arg(""));
        assert!(!is_safe_adb_arg("   "));
        assert!(!is_safe_adb_arg("-bad"));
        assert!(!is_safe_adb_arg(" --also-bad"));
    }

    #[test]
    fn adb_arg_accepts_normal_paths() {
        assert!(is_safe_adb_arg("/home/user/a.png"));
        assert!(is_safe_adb_arg("/sdcard/background.jpg"));
        assert!(is_safe_adb_arg("relative/path.png"));
    }
}
