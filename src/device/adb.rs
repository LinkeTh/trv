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

/// Timeout for short ADB query commands used by the TUI.
const ADB_QUERY_TIMEOUT: Duration = Duration::from_secs(5);

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

/// Run an `adb` command with timeout and capture textual output.
///
/// Returns `Some(output)` only when the command exits successfully within the
/// timeout. If stdout is empty, stderr is returned instead.
fn run_adb_capture_with_timeout(args: &[&str], timeout: Duration) -> Option<String> {
    let mut child = Command::new("adb")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;

    let poll_interval = Duration::from_millis(50);
    let mut elapsed = Duration::ZERO;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                use std::io::Read;

                let mut stdout = String::new();
                if let Some(mut out) = child.stdout.take() {
                    let _ = out.read_to_string(&mut stdout);
                }

                let mut stderr = String::new();
                if let Some(mut err) = child.stderr.take() {
                    let _ = err.read_to_string(&mut stderr);
                }

                if !status.success() {
                    return None;
                }

                if !stdout.trim().is_empty() {
                    return Some(stdout);
                }

                if !stderr.trim().is_empty() {
                    return Some(stderr);
                }

                return Some(String::new());
            }
            Ok(None) => {}
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }

        if elapsed >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }

        std::thread::sleep(poll_interval);
        elapsed += poll_interval;
    }
}

fn parse_resolution_token(token: &str) -> Option<(u16, u16)> {
    let cleaned = token
        .trim()
        .trim_matches(|c: char| !(c.is_ascii_digit() || c == 'x' || c == 'X'));

    let mut parts = cleaned.split(['x', 'X']);
    let width = parts.next()?.parse::<u16>().ok()?;
    let height = parts.next()?.parse::<u16>().ok()?;

    if parts.next().is_some() || width == 0 || height == 0 {
        return None;
    }

    Some((width, height))
}

fn parse_wm_size_output(output: &str) -> Option<(u16, u16)> {
    let mut fallback = None;
    let mut physical = None;
    let mut override_size = None;

    for line in output.lines() {
        let parsed = line.split_whitespace().find_map(parse_resolution_token);

        if let Some(size) = parsed {
            if fallback.is_none() {
                fallback = Some(size);
            }

            let lower = line.to_ascii_lowercase();
            if lower.contains("override size") {
                override_size = Some(size);
            } else if lower.contains("physical size") {
                physical = Some(size);
            }
        }
    }

    override_size.or(physical).or(fallback)
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

/// Query the connected Android device display resolution.
///
/// Uses `adb shell wm size` and returns `(width, height)` in pixels.
/// If no device is connected or the output is unparseable, returns `None`.
pub fn adb_display_size() -> Option<(u16, u16)> {
    let output = run_adb_capture_with_timeout(&["shell", "wm", "size"], ADB_QUERY_TIMEOUT)?;
    parse_wm_size_output(&output)
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
    use super::{is_safe_adb_arg, parse_wm_size_output};

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

    #[test]
    fn parse_wm_size_prefers_override_size() {
        let out = "Physical size: 1440x3120\nOverride size: 1080x2340\n";
        assert_eq!(parse_wm_size_output(out), Some((1080, 2340)));
    }

    #[test]
    fn parse_wm_size_uses_physical_when_no_override() {
        let out = "Physical size: 484x480\n";
        assert_eq!(parse_wm_size_output(out), Some((484, 480)));
    }

    #[test]
    fn parse_wm_size_falls_back_to_any_resolution_token() {
        let out = "mCurrentDisplayRect=Rect(0,0 - 800x600)\n";
        assert_eq!(parse_wm_size_output(out), Some((800, 600)));
    }

    #[test]
    fn parse_wm_size_returns_none_for_invalid_output() {
        let out = "wm size: unknown\n";
        assert_eq!(parse_wm_size_output(out), None);
    }
}
