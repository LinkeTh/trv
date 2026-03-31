/// ADB (Android Debug Bridge) helpers for the TRV LCD device.
///
/// The app runs on an Android device. To communicate over TCP from the host,
/// ADB must forward a local port to the device's TCP port.
///
/// Command:  `adb forward tcp:<port> tcp:<port>`
use std::fmt;
use std::process::Command;
use std::time::Duration;

/// Default timeout for ADB subcommands (forward, push, settings).
const ADB_TIMEOUT: Duration = Duration::from_secs(15);

/// Timeout for short ADB query commands used by the TUI.
const ADB_QUERY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub enum AdbError {
    UnsafeArg(String),
    Spawn {
        args: String,
        source: String,
    },
    Timeout {
        args: String,
        timeout: Duration,
    },
    Wait {
        args: String,
        source: String,
    },
    NonZeroExit {
        args: String,
        code: Option<i32>,
        stderr: String,
    },
}

impl fmt::Display for AdbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdbError::UnsafeArg(arg) => write!(f, "unsafe adb argument: '{}'", arg),
            AdbError::Spawn { args, source } => {
                write!(f, "failed to spawn adb {}: {}", args, source)
            }
            AdbError::Timeout { args, timeout } => {
                write!(f, "adb {} timed out after {}s", args, timeout.as_secs_f64())
            }
            AdbError::Wait { args, source } => {
                write!(f, "failed waiting for adb {}: {}", args, source)
            }
            AdbError::NonZeroExit { args, code, stderr } => {
                if stderr.trim().is_empty() {
                    write!(f, "adb {} exited with status {:?}", args, code)
                } else {
                    write!(
                        f,
                        "adb {} exited with status {:?}: {}",
                        args,
                        code,
                        stderr.trim()
                    )
                }
            }
        }
    }
}

fn args_display(args: &[&str]) -> String {
    args.join(" ")
}

/// Run an `adb` command with timeout and captured output.
///
/// Spawns the child, polls with `try_wait` until exit or timeout, then reaps
/// and returns captured stdout/stderr on success.
fn run_adb_output_with_timeout(
    args: &[&str],
    timeout: Duration,
) -> Result<std::process::Output, AdbError> {
    let args_text = args_display(args);
    let mut child = Command::new("adb")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| AdbError::Spawn {
            args: args_text.clone(),
            source: e.to_string(),
        })?;

    let poll_interval = Duration::from_millis(50);
    let mut elapsed = Duration::ZERO;

    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                let output = child.wait_with_output().map_err(|e| AdbError::Wait {
                    args: args_text.clone(),
                    source: e.to_string(),
                })?;

                if output.status.success() {
                    return Ok(output);
                }

                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                return Err(AdbError::NonZeroExit {
                    args: args_text,
                    code: output.status.code(),
                    stderr,
                });
            }
            Ok(None) => {}
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AdbError::Wait {
                    args: args_text,
                    source: "try_wait failed".to_string(),
                });
            }
        }

        if elapsed >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AdbError::Timeout {
                args: args_text,
                timeout,
            });
        }

        std::thread::sleep(poll_interval);
        elapsed += poll_interval;
    }
}

/// Run an `adb` command with timeout and require success.
fn run_adb_success_with_timeout(args: &[&str], timeout: Duration) -> Result<(), AdbError> {
    let _ = run_adb_output_with_timeout(args, timeout)?;
    Ok(())
}

/// Run an `adb` command with timeout and capture textual output.
///
/// Returns `Some(output)` only when the command exits successfully within the
/// timeout. If stdout is empty, stderr is returned instead.
fn run_adb_capture_with_timeout(args: &[&str], timeout: Duration) -> Option<String> {
    let output = run_adb_output_with_timeout(args, timeout).ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !stdout.trim().is_empty() {
        return Some(stdout);
    }

    if !stderr.trim().is_empty() {
        return Some(stderr);
    }

    Some(String::new())
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

fn parse_utc_offset_hhmm(output: &str) -> Option<i32> {
    let raw = output.trim();

    let (sign, hhmm) = if let Some(rest) = raw.strip_prefix('+') {
        (1_i32, rest)
    } else if let Some(rest) = raw.strip_prefix('-') {
        (-1_i32, rest)
    } else {
        return None;
    };

    let digits = if hhmm.len() == 4 {
        hhmm.to_string()
    } else if hhmm.len() == 5 && hhmm.as_bytes().get(2) == Some(&b':') {
        format!("{}{}", &hhmm[0..2], &hhmm[3..5])
    } else {
        return None;
    };

    if !digits.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    let hour = digits[0..2].parse::<i32>().ok()?;
    let minute = digits[2..4].parse::<i32>().ok()?;

    if hour > 23 || minute > 59 {
        return None;
    }

    Some(sign * ((hour * 3600) + (minute * 60)))
}

/// Run `adb forward tcp:<port> tcp:<port>` to set up port forwarding.
pub fn adb_forward(port: u16) -> Result<(), AdbError> {
    let tcp_arg = format!("tcp:{}", port);
    run_adb_success_with_timeout(&["forward", &tcp_arg, &tcp_arg], ADB_TIMEOUT)
}

/// Push a local file to the device via ADB.
///
/// `local_path` — absolute path on the host.
/// `remote_path` — path on the device (e.g. `/sdcard/background.jpg`).
pub fn adb_push(local_path: &str, remote_path: &str) -> Result<(), AdbError> {
    if !is_safe_adb_arg(local_path) || !is_safe_adb_arg(remote_path) {
        return Err(AdbError::UnsafeArg(format!(
            "local='{}' remote='{}'",
            local_path, remote_path
        )));
    }

    run_adb_success_with_timeout(&["push", local_path, remote_path], ADB_TIMEOUT)
}

/// Check if `adb` is available in PATH.
pub fn adb_available() -> bool {
    run_adb_success_with_timeout(&["version"], Duration::from_secs(5)).is_ok()
}

/// Query the connected Android device display resolution.
///
/// Uses `adb shell wm size` and returns `(width, height)` in pixels.
/// If no device is connected or the output is unparseable, returns `None`.
pub fn adb_display_size() -> Option<(u16, u16)> {
    let output = run_adb_capture_with_timeout(&["shell", "wm", "size"], ADB_QUERY_TIMEOUT)?;
    parse_wm_size_output(&output)
}

/// Query device timezone offset in seconds from UTC.
///
/// Uses `adb shell date +%z` and parses `±HHMM` (or `±HH:MM`) output.
/// Returns `None` when no device is connected, adb is unavailable, or parsing
/// fails.
pub fn adb_timezone_offset_seconds() -> Option<i32> {
    let output = run_adb_capture_with_timeout(&["shell", "date", "+%z"], ADB_QUERY_TIMEOUT)?;
    parse_utc_offset_hhmm(&output)
}

/// Run `adb shell settings put system <key> <value>`.
pub fn adb_settings_put_system(key: &str, value: &str) -> Result<(), AdbError> {
    if !is_safe_adb_arg(key) || !is_safe_adb_arg(value) {
        return Err(AdbError::UnsafeArg(format!(
            "key='{}' value='{}'",
            key, value
        )));
    }

    run_adb_success_with_timeout(
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
    use std::time::Duration;

    use super::{is_safe_adb_arg, parse_utc_offset_hhmm, parse_wm_size_output, AdbError};

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

    #[test]
    fn parse_utc_offset_hhmm_accepts_basic_formats() {
        assert_eq!(parse_utc_offset_hhmm("+0800\n"), Some(8 * 3600));
        assert_eq!(
            parse_utc_offset_hhmm("-0530\r\n"),
            Some(-(5 * 3600 + 30 * 60))
        );
        assert_eq!(parse_utc_offset_hhmm("+08:00"), Some(8 * 3600));
    }

    #[test]
    fn parse_utc_offset_hhmm_rejects_invalid_values() {
        assert_eq!(parse_utc_offset_hhmm("0800"), None);
        assert_eq!(parse_utc_offset_hhmm("+2500"), None);
        assert_eq!(parse_utc_offset_hhmm("+0860"), None);
        assert_eq!(parse_utc_offset_hhmm("UTC+8"), None);
    }

    #[test]
    fn adb_error_display_includes_context() {
        let e = AdbError::Timeout {
            args: "push a b".to_string(),
            timeout: Duration::from_secs(1),
        };
        let s = e.to_string();
        assert!(s.contains("push a b"));
        assert!(s.contains("timed out"));
    }
}
