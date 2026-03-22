/// Daemon runner — loads a theme, initialises the device, runs the cmd15 metrics loop.
///
/// Flow:
///   1. Load theme from TOML
///   2. (optional) ADB forward
///   3. (optional) Send cmd24 wake-on
///   4. (optional) Push background and widget image files via ADB
///   5. Send cmd3A split frames (one widget per frame, 50 ms apart)
///   6. Prime CPU usage baseline (sysinfo needs two samples for a delta)
///   7. Loop: collect metrics → build cmd15 payload → send frame → sleep
use std::{collections::HashSet, path::Path, time::Duration};

use anyhow::{Context, Result};
use tracing::{debug, error, info, warn};

use crate::{
    device::{adb, connection},
    metrics::collector::MetricCollector,
    protocol::{
        cmd::{build_cmd15_payload, build_cmd24_payload},
        frame::build_frame_default,
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
        if adb::adb_available() {
            let ok = adb::adb_forward(cfg.port);
            if ok {
                info!("adb forward tcp:{p} tcp:{p} OK", p = cfg.port);
            } else {
                warn!("adb forward failed — continuing anyway");
            }
        } else {
            warn!("adb not found in PATH — skipping forward");
        }
    }

    // ── 3. Wake-on (cmd24) ─────────────────────────────────────────────────
    if cfg.send_wake {
        let payload = build_cmd24_payload(true);
        let frame = build_frame_default(0x24, &payload);
        if cfg.dry_run {
            info!("dry-run cmd24 wake frame={}", hex::encode_upper(&frame));
        } else {
            match connection::send_frame(&cfg.host, cfg.port, &frame, cfg.recv_timeout_ms).await {
                Ok(reply) => info!("cmd24 wake reply={}", hex::encode_upper(&reply)),
                Err(e) => warn!("cmd24 wake error: {e} — continuing"),
            }
        }
    }

    // ── 4. Push theme assets (background + image widgets) ─────────────────
    push_theme_assets(&theme, cfg.dry_run);

    // ── 5. Send cmd3A split frames ─────────────────────────────────────────
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
            adb::INTER_FRAME_DELAY.as_millis() as u64,
        )
        .await
        .context("sending cmd3A frames")?;
        info!("cmd3A theme frames sent OK");
    }

    // ── 6. Determine metric sources from theme ─────────────────────────────
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

    // ── 7. Prime CPU baseline ──────────────────────────────────────────────
    let mut collector = MetricCollector::new(cfg.temp_offset_c);
    collector.prime();
    // Give sysinfo time to accumulate a CPU usage delta
    tokio::time::sleep(Duration::from_millis(500)).await;

    // ── 8. Metrics loop ────────────────────────────────────────────────────
    let interval = Duration::from_secs_f64(cfg.interval_s.max(0.1));
    let mut sent: u32 = 0;
    let mut consecutive_errors: u32 = 0;
    let max_retries = cfg.max_retries;

    loop {
        if cfg.count > 0 && sent >= cfg.count {
            info!("completed {} cmd15 cycle(s), exiting", sent);
            break;
        }

        match send_metrics_frame(&cfg, &mut collector, &sources).await {
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
                let backoff = (1.0_f64 * 2_f64.powi(consecutive_errors as i32 - 1)).min(30.0);

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

        if cfg.count > 0 && sent >= cfg.count {
            break;
        }

        // Sleep until next cycle, but wake immediately on Ctrl-C.
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("received SIGINT, shutting down");
                break;
            }
            _ = tokio::time::sleep(interval) => {}
        }
    }

    info!("daemon exiting normally after {} cmd15 frame(s)", sent);
    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Push local theme assets referenced by the theme to device `/sdcard/`.
///
/// - Background: uses `background.local_path` -> `/sdcard/<background.image>`
/// - Image/video widgets: uses `widget.path` local file -> `/sdcard/<basename(widget.path)>`
///
/// Missing local files are not fatal; we assume the asset may already exist on
/// the device under the same remote name.
pub fn push_theme_assets(theme: &Theme, dry_run: bool) {
    if !dry_run && !adb::adb_available() {
        warn!("adb not found in PATH — skipping asset pushes");
        return;
    }

    if !theme.background.local_path.is_empty() {
        if theme.background.image.trim().is_empty() {
            warn!(
                "background.local_path is set but background.image is empty — skipping background push"
            );
        } else {
            let local = &theme.background.local_path;
            let remote = format!("/sdcard/{}", theme.background.image);
            if !Path::new(local).is_file() {
                info!(
                    "background local_path not found: '{local}' — assuming already present as {remote}"
                );
            } else {
                if dry_run {
                    info!("dry-run adb push {local} -> {remote}");
                } else {
                    info!("pushing background image: {local} -> {remote}");
                    let ok = adb::adb_push(local, &remote);
                    if ok {
                        info!("background image pushed OK");
                    } else {
                        warn!("adb push failed for background image — continuing");
                    }
                }
            }
        }
    }

    let mut pushed_local_paths: HashSet<String> = HashSet::new();
    let mut pushed_remote_names: HashSet<String> = HashSet::new();
    for (i, widget) in theme.widgets.iter().enumerate() {
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
        let ok = adb::adb_push(local, &remote);
        if ok {
            info!("widget[{i}] {kind_name} pushed OK");
        } else {
            warn!("adb push failed for widget[{i}] {kind_name} — continuing");
        }
    }
}

/// Collect one round of metrics and send a cmd15 frame.
async fn send_metrics_frame(
    cfg: &DaemonConfig,
    collector: &mut MetricCollector,
    sources: &[(String, crate::theme::model::MetricSource)],
) -> Result<()> {
    let readings = collector.collect(sources);

    if readings.is_empty() {
        return Err(anyhow::anyhow!("no metric values available"));
    }

    // Build show_values slice (borrow from readings map)
    let show_vals: Vec<(&str, f64)> = readings.iter().map(|(id, v)| (id.as_str(), *v)).collect();

    let payload =
        build_cmd15_payload(&show_vals).map_err(|e| anyhow::anyhow!("cmd15 build error: {}", e))?;

    let frame = build_frame_default(0x15, &payload);

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
    use crate::theme::hex::{WidgetHexParams, build_widget_bytes};

    if theme.widgets.is_empty() {
        return Ok(vec![]);
    }

    let mut widget_payloads: Vec<Vec<u8>> = Vec::with_capacity(theme.widgets.len());
    for (i, widget) in theme.widgets.iter().enumerate() {
        let params = WidgetHexParams::try_from(widget)
            .map_err(|e| anyhow::anyhow!("widget[{}] conversion error: {}", i, e))?;
        let bytes = build_widget_bytes(&params)
            .map_err(|e| anyhow::anyhow!("widget[{}] encode error: {}", i, e))?;
        widget_payloads.push(bytes);
    }

    Ok(split_cmd3a_frames(&widget_payloads))
}
