/// Async TCP connection to the TRV LCD device.
///
/// The device's TCP socket handler is a simple request/response protocol:
///   1. Connect (new connection per frame)
///   2. Send one AAF5 frame
///   3. Read the response (app replies with UTF-8 text, or nothing on timeout)
///   4. Close
///
/// Each new connection gets a fresh socket. This avoids any state issues with
/// the device's TCP handler and is simpler to reason about than a persistent
/// connection pool.
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Default timeout for waiting on a device reply (ms).
pub const DEFAULT_RECV_TIMEOUT_MS: u64 = 1000;

/// Default connection timeout (ms).
pub const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 5000;

/// Default inter-frame delay for split cmd3A sends (50 ms).
///
/// Sending one widget frame at a time with this gap prevents TCP fragmentation
/// issues observed on the device firmware.
pub const INTER_FRAME_DELAY: Duration = Duration::from_millis(50);

/// Send a single AAF5 frame to the device and return the reply bytes.
///
/// Opens a new TCP connection per call.
/// Returns an empty `Vec` if the device doesn't reply within `recv_timeout_ms`.
///
/// # Errors
/// Returns `Err` if the TCP connection or send fails.  A receive timeout is
/// NOT an error — it returns `Ok(vec![])`.
pub async fn send_frame(
    host: &str,
    port: u16,
    frame: &[u8],
    recv_timeout_ms: u64,
) -> Result<Vec<u8>> {
    let addr = format!("{}:{}", host, port);

    let stream = timeout(
        Duration::from_millis(DEFAULT_CONNECT_TIMEOUT_MS),
        TcpStream::connect(&addr),
    )
    .await
    .with_context(|| format!("connect timeout to {}", addr))?
    .with_context(|| format!("connect failed to {}", addr))?;

    // Disable Nagle — we send small frames and want immediate delivery
    stream
        .set_nodelay(true)
        .with_context(|| "set_nodelay failed")?;

    let (mut reader, mut writer) = stream.into_split();

    // Send
    writer
        .write_all(frame)
        .await
        .with_context(|| "frame write failed")?;

    // Flush to ensure the frame is sent
    writer.flush().await.with_context(|| "frame flush failed")?;

    // Receive with timeout — device may not always reply
    let mut buf = vec![0u8; 4096];
    match timeout(
        Duration::from_millis(recv_timeout_ms),
        reader.read(&mut buf),
    )
    .await
    {
        Ok(Ok(n)) => Ok(buf[..n].to_vec()),
        Ok(Err(e)) => Err(anyhow::anyhow!("recv error: {}", e)),
        Err(_) => Ok(vec![]), // timeout — no reply, not an error
    }
}

/// Send multiple frames in sequence with an optional inter-frame delay.
///
/// Suitable for sending split cmd3A frames (one widget per frame) to avoid
/// TCP fragmentation. Each frame gets its own TCP connection.
pub async fn send_frames(
    host: &str,
    port: u16,
    frames: &[Vec<u8>],
    recv_timeout_ms: u64,
    inter_frame_delay_ms: u64,
) -> Result<Vec<Vec<u8>>> {
    let mut replies = Vec::with_capacity(frames.len());
    for (i, frame) in frames.iter().enumerate() {
        let reply = send_frame(host, port, frame, recv_timeout_ms)
            .await
            .with_context(|| format!("sending frame {}", i))?;
        replies.push(reply);
        if inter_frame_delay_ms > 0 && i + 1 < frames.len() {
            tokio::time::sleep(Duration::from_millis(inter_frame_delay_ms)).await;
        }
    }
    Ok(replies)
}
