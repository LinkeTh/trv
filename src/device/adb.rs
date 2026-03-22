/// ADB (Android Debug Bridge) helpers for the TRV LCD device.
///
/// The app runs on an Android device. To communicate over TCP from the host,
/// ADB must forward a local port to the device's TCP port.
///
/// Command:  `adb forward tcp:<port> tcp:<port>`
use std::process::Command;
use std::time::Duration;

/// Default timeout for ADB subcommands (forward, push, settings).
const ADB_TIMEOUT: Duration = Duration::from_secs(15);

/// Run an `adb` command with a timeout.
///
/// Spawns the child, polls with `try_wait` until it exits or the timeout
/// elapses, then kills and reaps the process if still running.
///
/// Returns `true` if the process exited successfully within the timeout,
/// `false` on spawn failure, timeout, or non-zero exit status.
fn run_adb_with_timeout(args: &[&str], timeout: Duration) -> bool {
    let mut child = match Command::new("adb")
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    let poll_interval = Duration::from_millis(50);
    let mut elapsed = Duration::ZERO;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => {}
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }

        if elapsed >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }

        std::thread::sleep(poll_interval);
        elapsed += poll_interval;
    }
}

/// Run `adb forward tcp:<port> tcp:<port>` to set up port forwarding.
///
/// Returns `true` if the command succeeded, `false` otherwise.
/// Failures are silently ignored by callers.
pub fn adb_forward(port: u16) -> bool {
    let tcp_arg = format!("tcp:{}", port);
    run_adb_with_timeout(&["forward", &tcp_arg, &tcp_arg], ADB_TIMEOUT)
}

/// Push a local file to the device via ADB.
///
/// `local_path` — absolute path on the host.
/// `remote_path` — path on the device (e.g. `/sdcard/background.jpg`).
pub fn adb_push(local_path: &str, remote_path: &str) -> bool {
    if !is_safe_adb_arg(local_path) || !is_safe_adb_arg(remote_path) {
        return false;
    }

    run_adb_with_timeout(&["push", local_path, remote_path], ADB_TIMEOUT)
}

/// Check if `adb` is available in PATH.
pub fn adb_available() -> bool {
    run_adb_with_timeout(&["version"], Duration::from_secs(5))
}

/// Run `adb shell settings put system <key> <value>`.
///
/// Returns `true` on successful command exit status.
pub fn adb_settings_put_system(key: &str, value: &str) -> bool {
    if !is_safe_adb_arg(key) || !is_safe_adb_arg(value) {
        return false;
    }

    run_adb_with_timeout(
        &["shell", "settings", "put", "system", key, value],
        ADB_TIMEOUT,
    )
}

/// Shell metacharacters that must never appear in ADB arguments.
///
/// These characters have special meaning in POSIX shells and could allow
/// command injection if an argument is ever interpreted by a shell layer.
/// Rejecting them here is a defense-in-depth measure even though
/// `Command::new("adb").args(...)` does not invoke a shell.
const SHELL_METACHARACTERS: &[char] = &[';', '|', '`', '$', '&', '\n', '\r', '\\'];

fn is_safe_adb_arg(arg: &str) -> bool {
    let trimmed = arg.trim();
    !trimmed.is_empty()
        && !trimmed.starts_with('-')
        && !trimmed.chars().any(|c| SHELL_METACHARACTERS.contains(&c))
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
    fn adb_arg_rejects_shell_metacharacters() {
        assert!(!is_safe_adb_arg("/sdcard/file;rm -rf /"));
        assert!(!is_safe_adb_arg("foo|bar"));
        assert!(!is_safe_adb_arg("`id`"));
        assert!(!is_safe_adb_arg("$HOME"));
        assert!(!is_safe_adb_arg("a&b"));
        assert!(!is_safe_adb_arg("foo\nbar"));
        assert!(!is_safe_adb_arg("foo\rbar"));
        assert!(!is_safe_adb_arg("foo\\bar"));
    }

    #[test]
    fn adb_arg_accepts_normal_paths() {
        assert!(is_safe_adb_arg("/home/user/a.png"));
        assert!(is_safe_adb_arg("/sdcard/background.jpg"));
        assert!(is_safe_adb_arg("relative/path.png"));
    }
}
