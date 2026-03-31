use std::io::Write;
use std::time::Duration;

use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use trv::daemon::{DaemonConfig, run as run_daemon};

fn frame_cmd(frame: &[u8]) -> Option<u8> {
    if frame.len() >= 6 && frame[0] == 0xAA && frame[1] == 0xF5 {
        Some(frame[5])
    } else {
        None
    }
}

async fn spawn_mock_device(expected_frames: usize) -> (u16, tokio::task::JoinHandle<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock device");
    let port = listener.local_addr().expect("local addr").port();

    let handle = tokio::spawn(async move {
        let mut frames = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);

        while frames.len() < expected_frames {
            if tokio::time::Instant::now() >= deadline {
                break;
            }

            let accept_res =
                tokio::time::timeout(Duration::from_millis(500), listener.accept()).await;
            let Ok(Ok((mut stream, _addr))) = accept_res else {
                continue;
            };

            let mut buf = vec![0u8; 4096];
            let read_res =
                tokio::time::timeout(Duration::from_millis(500), stream.read(&mut buf)).await;
            if let Ok(Ok(n)) = read_res
                && n > 0
            {
                frames.push(buf[..n].to_vec());
                let _ = stream.write_all(b"0").await;
            }
        }

        frames
    });

    (port, handle)
}

#[tokio::test]
async fn test_daemon_sends_theme_then_metrics() {
    let mut theme_file = NamedTempFile::new().expect("create temp theme");
    let theme_toml = r#"
[meta]
name = "daemon-lifecycle"

[[widget]]
type = "metric"
x = 10
y = 10
text_size = 32
color = "FFFFFF"
source = "cpu_usage"
unit = "%"
label = "CPU "
show_label = true
"#;
    theme_file
        .write_all(theme_toml.as_bytes())
        .expect("write temp theme");
    theme_file.flush().expect("flush temp theme");

    let (port, server_handle) = spawn_mock_device(2).await;

    let cfg = DaemonConfig {
        theme_path: theme_file.path().to_path_buf(),
        host: "127.0.0.1".to_string(),
        port,
        dry_run: false,
        count: 1,
        interval_s: 0.1,
        temp_offset_c: 0.0,
        adb_forward: false,
        send_wake: false,
        sync_time: false,
        recv_timeout_ms: 500,
        max_retries: 2,
    };

    run_daemon(cfg).await.expect("daemon run should succeed");

    let frames = server_handle.await.expect("server task join");
    let cmds: Vec<u8> = frames.iter().filter_map(|f| frame_cmd(f)).collect();

    assert!(
        cmds.contains(&0x3A),
        "expected at least one cmd3A frame, got cmds={:?}",
        cmds
    );
    assert!(
        cmds.contains(&0x15),
        "expected at least one cmd15 frame, got cmds={:?}",
        cmds
    );
}
