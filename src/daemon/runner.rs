/// Daemon runner — loads a theme, initialises the device, runs the cmd15 metrics loop.
///
/// Flow:
///   1. Load theme from TOML
///   2. (optional) ADB forward
///   3. (optional) Send cmd24 wake-on
///   4. (optional) Send cmd36 time-sync to set device clock to local time
///   5. (optional) Push widget image/video files via ADB
///   6. Send cmd3A split frames (one widget per frame, 50 ms apart)
///   7. Determine metric sources from theme
///   8. Prime CPU usage baseline (sysinfo needs two samples for a delta)
///   9. Loop: collect metrics → build cmd15 payload → send frame → sleep
use std::{
    collections::HashSet,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result};
use chrono::{Offset, TimeZone};
use chrono_tz::Asia::Shanghai;
use tracing::{debug, error, info, warn};

use crate::{
    device::{adb, connection},
    metrics::collector::MetricCollector,
    protocol::cmd::{
        Cmd15Field, PowerState, build_cmd15_frame, build_cmd24_frame,
        build_cmd36_frame_inverse_long,
    },
    theme::{
        hex::split_cmd3a_frames,
        model::{Theme, WidgetKind, image_remote_name, theme_metric_sources},
        toml::load_theme_file,
    },
};

use super::config::DaemonConfig;

/// Run the daemon with the given config.  Blocks until done (or Ctrl-C).
pub async fn run(cfg: DaemonConfig) -> Result<()> {
    // ── 1. Load theme ──────────────────────────────────────────────────────
    info!("loading theme from {:?}", cfg.theme_path);
    let theme: Theme = load_theme_file(&cfg.theme_path)
        .with_context(|| format!("failed to load theme {:?}", cfg.theme_path))?;
    info!("theme loaded: {:?}", theme.meta.name);

    // ── 2. ADB forward ─────────────────────────────────────────────────────
    if cfg.adb_forward {
        let port = cfg.port;
        let (available, forward_result) = tokio::task::spawn_blocking(move || {
            if adb::adb_available() {
                (true, Some(adb::adb_forward(port)))
            } else {
                (false, None)
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("adb forward worker join error: {}", e))?;
        if available {
            match forward_result {
                Some(Ok(())) => info!("adb forward tcp:{p} tcp:{p} OK", p = cfg.port),
                Some(Err(e)) => warn!("adb forward failed: {e} — continuing anyway"),
                None => warn!("adb forward skipped unexpectedly — continuing anyway"),
            }
        } else {
            warn!("adb not found in PATH — skipping forward");
        }
    }

    // ── 3. Wake-on (cmd24) ─────────────────────────────────────────────────
    if cfg.send_wake {
        let frame = build_cmd24_frame(PowerState::Wake)
            .map_err(|e| anyhow::anyhow!("build cmd24 frame: {}", e))?;
        if cfg.dry_run {
            info!("dry-run cmd24 wake frame={}", hex::encode_upper(&frame));
        } else {
            match connection::send_frame(&cfg.host, cfg.port, &frame, cfg.recv_timeout_ms).await {
                Ok(reply) => info!("cmd24 wake reply={}", hex::encode_upper(&reply)),
                Err(e) => warn!("cmd24 wake error: {e} — continuing"),
            }
        }
    }

    // ── 4. Time sync (cmd36) ─────────────────────────────────────────────
    if cfg.sync_time {
        let host_now = chrono::Local::now();
        let host_offset_s = host_now.offset().local_minus_utc() as i64;
        let shanghai_offset_s = Shanghai
            .offset_from_utc_datetime(&host_now.naive_utc())
            .fix()
            .local_minus_utc() as i64;
        let adb_device_offset_s =
            tokio::task::spawn_blocking(|| adb::adb_timezone_offset_seconds().map(i64::from))
                .await
                .map_err(|e| anyhow::anyhow!("adb timezone worker join error: {}", e))?;
        let (device_base_offset_s, device_offset_source) = if let Some(offset) = adb_device_offset_s
        {
            (offset, "adb")
        } else {
            (shanghai_offset_s, "asia-shanghai-fallback")
        };

        // We shift epoch by host_offset - device_base_offset so on-screen
        // wall-clock matches host local time even when timezone itself is immutable.
        let shift_s = host_offset_s - device_base_offset_s;
        let shift_ms = shift_s.checked_mul(1000).ok_or_else(|| {
            anyhow::anyhow!(
                "cmd36 shift overflow: shift_s={} cannot be represented in ms",
                shift_s
            )
        })?;
        let target_epoch_ms = host_now
            .timestamp_millis()
            .checked_add(shift_ms)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "cmd36 target epoch overflow: host_ms={} shift_ms={}",
                    host_now.timestamp_millis(),
                    shift_ms
                )
            })?;
        let target_epoch_ms_u64 = u64::try_from(target_epoch_ms).map_err(|_| {
            anyhow::anyhow!(
                "computed cmd36 target epoch ms is negative: {}",
                target_epoch_ms
            )
        })?;
        let frame = build_cmd36_frame_inverse_long(target_epoch_ms_u64)
            .map_err(|e| anyhow::anyhow!("build cmd36 inverse-long frame: {}", e))?;
        if cfg.dry_run {
            info!(
                "dry-run cmd36 time-sync frame={} target_epoch_ms={} host_offset_s={} device_base_offset_s={} device_offset_source={} shift_s={}",
                hex::encode_upper(&frame),
                target_epoch_ms_u64,
                host_offset_s,
                device_base_offset_s,
                device_offset_source,
                shift_s
            );
        } else {
            match connection::send_frame(&cfg.host, cfg.port, &frame, cfg.recv_timeout_ms).await {
                Ok(reply) => info!(
                    "cmd36 time-sync reply={} strategy=inverse-long target_epoch_ms={} host_offset_s={} device_base_offset_s={} device_offset_source={} shift_s={}",
                    hex::encode_upper(&reply),
                    target_epoch_ms_u64,
                    host_offset_s,
                    device_base_offset_s,
                    device_offset_source,
                    shift_s
                ),
                Err(e) => warn!("cmd36 time-sync error: {e} — continuing"),
            }
        }
    }

    // ── 5. Push theme assets (image/video widgets) ─────────────────────────
    {
        let dry_run = cfg.dry_run;
        let theme_for_assets = theme.clone();
        tokio::task::spawn_blocking(move || push_theme_assets(&theme_for_assets, dry_run, None))
            .await
            .map_err(|e| anyhow::anyhow!("asset push worker join error: {}", e))?;
    }

    // ── 6. Send cmd3A split frames ─────────────────────────────────────────
    let split_frames = build_theme_frames(&theme)?;
    info!(
        "sending {} cmd3A widget frame(s) to {}:{}",
        split_frames.len(),
        cfg.host,
        cfg.port
    );

    if cfg.dry_run {
        for (i, frame) in split_frames.iter().enumerate() {
            let ttype = if i == 0 { "clear+add" } else { "append" };
            info!(
                "dry-run cmd3A frame[{i}] type={ttype} len={} hex_prefix={}...",
                frame.len(),
                hex::encode_upper(&frame[..frame.len().min(16)])
            );
        }
    } else {
        connection::send_frames(
            &cfg.host,
            cfg.port,
            &split_frames,
            cfg.recv_timeout_ms,
            connection::INTER_FRAME_DELAY.as_millis() as u64,
        )
        .await
        .context("sending cmd3A frames")?;
        info!("cmd3A theme frames sent OK");
    }

    // ── 7. Determine metric sources from theme ─────────────────────────────
    let sources = theme_metric_sources(&theme);
    if sources.is_empty() {
        warn!("theme has no metric widgets — no cmd15 updates will be sent");
        return Ok(());
    }
    info!(
        "metric sources: {:?}",
        sources
            .iter()
            .map(|(id, src)| format!("{}={:?}", id, src))
            .collect::<Vec<_>>()
    );

    // ── 8. Prime CPU baseline ──────────────────────────────────────────────
    let collector = Arc::new(Mutex::new(MetricCollector::new(cfg.temp_offset_c)));
    {
        let collector_for_prime = Arc::clone(&collector);
        tokio::task::spawn_blocking(move || {
            if let Ok(mut c) = collector_for_prime.lock() {
                c.prime();
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("metric prime worker join error: {}", e))?;
    }
    // Give sysinfo time to accumulate a CPU usage delta
    tokio::time::sleep(Duration::from_millis(500)).await;

    // ── 9. Metrics loop ────────────────────────────────────────────────────
    let interval = Duration::from_secs_f64(cfg.interval_s.max(0.1));
    let mut sent: u32 = 0;
    let mut consecutive_errors: u32 = 0;
    let max_retries = cfg.max_retries;

    // Pin the ctrl_c future outside the loop so the OS signal handler is
    // registered exactly once. Re-creating it every iteration would leak
    // a new registration on each cycle.
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        if cfg.count > 0 && sent >= cfg.count {
            info!("completed {} cmd15 cycle(s), exiting", sent);
            break;
        }

        match send_metrics_frame(&cfg, Arc::clone(&collector), &sources).await {
            Ok(()) => {
                sent += 1;
                consecutive_errors = 0;
                debug!(
                    "cmd15 sent ({}/{})",
                    sent,
                    if cfg.count == 0 { u32::MAX } else { cfg.count }
                );
            }
            Err(e) => {
                consecutive_errors += 1;
                let backoff = (1.0_f64
                    * 2_f64.powi(consecutive_errors.saturating_sub(1).min(5) as i32))
                .min(30.0);

                if max_retries > 0 && consecutive_errors > max_retries {
                    error!("cmd15 error ({consecutive_errors} consecutive, giving up): {e}");
                    return Err(e);
                }

                warn!(
                    "cmd15 error ({consecutive_errors}/{retries}), retrying in {backoff:.1}s: {e}",
                    retries = if max_retries == 0 {
                        "inf".to_string()
                    } else {
                        max_retries.to_string()
                    }
                );
                tokio::time::sleep(Duration::from_secs_f64(backoff)).await;
                continue;
            }
        }

        // Sleep until next cycle, but wake immediately on Ctrl-C.
        tokio::select! {
            result = &mut ctrl_c => {
                if result.is_ok() {
                    info!("received SIGINT, shutting down");
                }
                break;
            }
            _ = tokio::time::sleep(interval) => {}
        }
    }

    info!("daemon exiting normally after {} cmd15 frame(s)", sent);
    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Push local image/video widget assets referenced by the theme to `/sdcard/`.
///
/// Missing local files are not fatal; we assume the asset may already exist on
/// the device under the same remote name.
///
/// If `cancel` is provided, each push is preceded by a cancellation check;
/// the function returns early (without error) if the flag is set.
pub fn push_theme_assets(
    theme: &Theme,
    dry_run: bool,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) {
    if !dry_run && !adb::adb_available() {
        warn!("adb not found in PATH — skipping asset pushes");
        return;
    }

    let mut pushed_local_paths: HashSet<String> = HashSet::new();
    let mut pushed_remote_names: HashSet<String> = HashSet::new();
    for (i, widget) in theme.widgets.iter().enumerate() {
        // Check for cancellation before each potential blocking push.
        if let Some(flag) = cancel
            && flag.load(std::sync::atomic::Ordering::Acquire)
        {
            info!("push_theme_assets: cancelled at widget[{i}]");
            return;
        }

        let (local, kind_name) = match &widget.kind {
            WidgetKind::Image { path } => (path.trim(), "image"),
            WidgetKind::Video { path } => (path.trim(), "video"),
            _ => continue,
        };
        if local.is_empty() {
            continue;
        }

        if !pushed_local_paths.insert(local.to_string()) {
            continue;
        }

        let remote_name = image_remote_name(local);
        if remote_name.is_empty() {
            warn!("widget[{i}] image path is invalid: '{local}'");
            continue;
        }

        if !pushed_remote_names.insert(remote_name.clone()) {
            warn!(
                "widget[{i}] image basename collision for '{remote_name}' — later pushes overwrite earlier files"
            );
        }

        let remote = format!("/sdcard/{remote_name}");

        if !Path::new(local).is_file() {
            info!(
                "widget[{i}] local {kind_name} not found: '{local}' — assuming already present as {remote}"
            );
            continue;
        }

        if dry_run {
            info!("dry-run adb push widget[{i}] {kind_name} {local} -> {remote}");
            continue;
        }

        info!("pushing widget[{i}] {kind_name}: {local} -> {remote}");
        match adb::adb_push(local, &remote) {
            Ok(()) => info!("widget[{i}] {kind_name} pushed OK"),
            Err(e) => warn!("adb push failed for widget[{i}] {kind_name}: {e} — continuing"),
        }
    }
}

/// Collect one round of metrics and send a cmd15 frame.
async fn send_metrics_frame(
    cfg: &DaemonConfig,
    collector: Arc<Mutex<MetricCollector>>,
    sources: &[(
        crate::protocol::cmd::ShowId,
        crate::theme::model::MetricSource,
    )],
) -> Result<()> {
    let sources_owned = sources.to_vec();
    let readings = tokio::task::spawn_blocking(move || {
        let mut guard = collector
            .lock()
            .map_err(|_| anyhow::anyhow!("metric collector mutex poisoned"))?;
        Ok::<_, anyhow::Error>(guard.collect(&sources_owned))
    })
    .await
    .map_err(|e| anyhow::anyhow!("metric collection worker join error: {}", e))??;

    if readings.is_empty() {
        return Err(anyhow::anyhow!("no metric values available"));
    }

    let mut fields: Vec<Cmd15Field> = Vec::with_capacity(readings.len());
    for (show_id, value) in &readings {
        fields.push(Cmd15Field {
            show_id: *show_id,
            value: *value,
        });
    }

    let frame =
        build_cmd15_frame(&fields).map_err(|e| anyhow::anyhow!("cmd15 build error: {}", e))?;

    if cfg.dry_run {
        info!(
            "dry-run cmd15 values={:?} frame={}",
            readings,
            hex::encode_upper(&frame)
        );
        return Ok(());
    }

    let reply = connection::send_frame(&cfg.host, cfg.port, &frame, cfg.recv_timeout_ms).await?;

    debug!(
        "cmd15 values={:?} reply={}",
        readings,
        hex::encode_upper(&reply)
    );

    Ok(())
}

/// Build the split cmd3A frames for the theme's widget list.
pub fn build_theme_frames(theme: &Theme) -> Result<Vec<Vec<u8>>> {
    use crate::protocol::widget::WidgetPayloadRaw;
    use crate::theme::hex::WidgetHexParams;

    if theme.widgets.is_empty() {
        return Ok(vec![]);
    }

    let mut widget_payloads: Vec<WidgetPayloadRaw> = Vec::with_capacity(theme.widgets.len());
    for (i, widget) in theme.widgets.iter().enumerate() {
        let params = WidgetHexParams::try_from(widget)
            .map_err(|e| anyhow::anyhow!("widget[{}] conversion error: {}", i, e))?;
        let raw = WidgetPayloadRaw::try_from(&params)
            .map_err(|e| anyhow::anyhow!("widget[{}] encode error: {}", i, e))?;
        widget_payloads.push(raw);
    }

    split_cmd3a_frames(&widget_payloads).map_err(|e| anyhow::anyhow!("split cmd3a frames: {}", e))
}
