/// trv — TRV LCD display daemon and TUI theme editor.
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use trv::config as app_config;
use trv::daemon::{DaemonConfig, run as run_daemon};
use trv::theme::presets::{ALL_PRESETS, find_preset};
use trv::theme::toml::{parse_theme_toml, serialize_theme};

#[derive(Parser)]
#[command(
    name = "trv",
    about = "TRV LCD display daemon and TUI theme editor",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the display daemon (load theme, connect, stream metrics)
    Daemon {
        /// Path to theme TOML file (default: ~/.config/trv/config.toml -> theme, else dashboard)
        #[arg(short, long, conflicts_with = "preset")]
        theme: Option<PathBuf>,

        /// Built-in preset to use instead of a file (see `trv list`)
        #[arg(long, conflicts_with = "theme")]
        preset: Option<String>,

        /// Device host/IP (default: localhost via ADB forward)
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Device TCP port
        #[arg(long, default_value_t = 22222)]
        port: u16,

        /// Dry-run: log frames without sending to device
        #[arg(long)]
        dry_run: bool,

        /// Number of metric update cycles (0 = infinite)
        #[arg(long, default_value_t = 0)]
        count: u32,

        /// Metric collection interval in seconds
        #[arg(long, default_value_t = 1.0)]
        interval: f64,

        /// Log level (error|warn|info|debug|trace)
        #[arg(long, default_value = "info")]
        log_level: String,

        /// Temperature offset in °C (added to all temp readings)
        #[arg(long, default_value_t = 0.0)]
        temp_offset: f64,

        /// Run `adb forward tcp:PORT tcp:PORT` before connecting
        #[arg(long)]
        adb_forward: bool,

        /// Send cmd24 wake-on before setup sequence
        #[arg(long)]
        wake: bool,

        /// Receive timeout per frame in milliseconds
        #[arg(long, default_value_t = 1000)]
        recv_timeout_ms: u64,

        /// Max consecutive errors before aborting (0 = infinite retries)
        #[arg(long, default_value_t = 0)]
        max_retries: u32,
    },

    /// Launch the interactive TUI theme editor
    Tui {
        /// Path to theme TOML file to edit (default: ~/.config/trv/config.toml -> theme, else dashboard)
        #[arg(short, long, conflicts_with = "preset")]
        theme: Option<PathBuf>,

        /// Built-in preset to load instead of a file (see `trv list`)
        #[arg(long, conflicts_with = "theme")]
        preset: Option<String>,

        /// Device host for push-to-device
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Device TCP port
        #[arg(long, default_value_t = 22222)]
        port: u16,

        /// Run `adb forward tcp:PORT tcp:PORT` before connecting
        #[arg(long)]
        adb_forward: bool,

        /// Receive timeout per frame in milliseconds (used during push)
        #[arg(long, default_value_t = 2000)]
        recv_timeout_ms: u64,
    },

    /// List available built-in theme presets
    List,

    /// Export a built-in preset as TOML (prints to stdout)
    ///
    /// Example: trv export dashboard > ~/.config/trv/themes/dashboard.toml
    Export {
        /// Preset slug (see `trv list` for available slugs)
        slug: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Daemon {
            theme,
            preset,
            host,
            port,
            dry_run,
            count,
            interval,
            log_level,
            temp_offset,
            adb_forward,
            wake,
            recv_timeout_ms,
            max_retries,
        } => {
            let filter =
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_level));
            tracing_subscriber::fmt().with_env_filter(filter).init();

            let using_preset = preset.is_some();

            // Resolve theme path: --preset writes to a temp file so the daemon
            // can use its path-based config.  For the daemon we need a file path,
            // so we write the preset to a temp location.
            let theme_path = if let Some(slug) = preset {
                match resolve_preset_to_tempfile(&slug) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("error: {}", e);
                        std::process::exit(1);
                    }
                }
            } else if let Some(path) = theme {
                path
            } else {
                match resolve_theme_path_from_config_or_default() {
                    Ok(path) => path,
                    Err(e) => {
                        eprintln!("error: {}", e);
                        std::process::exit(1);
                    }
                }
            };

            let theme_path_for_config = theme_path.clone();

            let cfg = DaemonConfig {
                theme_path,
                host,
                port,
                dry_run,
                count,
                interval_s: interval,
                temp_offset_c: temp_offset,
                adb_forward,
                send_wake: wake,
                recv_timeout_ms,
                max_retries,
            };

            if let Err(e) = run_daemon(cfg).await {
                eprintln!("daemon error: {:#}", e);
                std::process::exit(1);
            }

            if !using_preset
                && let Err(e) = app_config::set_default_theme_path(&theme_path_for_config)
            {
                eprintln!(
                    "warning: daemon completed but could not update default theme in config: {}",
                    e
                );
            }
        }

        Commands::Tui {
            theme,
            preset,
            host,
            port,
            adb_forward,
            recv_timeout_ms,
        } => {
            // Optionally set up ADB forward before entering raw mode.
            if adb_forward {
                use trv::device::adb;
                if adb::adb_available() {
                    let ok = adb::adb_forward(port);
                    if !ok {
                        eprintln!("warning: adb forward failed — continuing");
                    }
                } else {
                    eprintln!("warning: adb not found in PATH — skipping forward");
                }
            }

            // Suppress tracing output so it doesn't corrupt the terminal display.
            let filter = EnvFilter::new("warn");
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(std::io::sink)
                .init();

            // Resolve theme / preset into (Option<Theme>, Option<PathBuf>).
            let (loaded_theme, loaded_path) = if let Some(slug) = preset {
                match resolve_preset(&slug) {
                    Ok(t) => (Some(t), None),
                    Err(e) => {
                        eprintln!("error: {}", e);
                        std::process::exit(1);
                    }
                }
            } else if let Some(path) = theme {
                (None, Some(path))
            } else {
                match resolve_theme_path_from_config_or_default() {
                    Ok(path) => (None, Some(path)),
                    Err(e) => {
                        eprintln!("error: {}", e);
                        std::process::exit(1);
                    }
                }
            };

            if let Err(e) = trv::tui::run(loaded_theme, loaded_path, host, port, recv_timeout_ms) {
                eprintln!("tui error: {:#}", e);
                std::process::exit(1);
            }
        }

        Commands::List => {
            println!("Built-in theme presets:");
            println!();
            for (slug, toml) in ALL_PRESETS {
                let desc = parse_theme_toml(toml)
                    .ok()
                    .map(|t| {
                        if t.meta.description.is_empty() {
                            t.meta.name
                        } else {
                            format!("{} — {}", t.meta.name, t.meta.description)
                        }
                    })
                    .unwrap_or_else(|| "(parse error)".into());
                println!("  {:<16} {}", slug, desc);
            }
            println!();
            println!("Load in TUI:     trv tui --preset <slug>");
            println!("Run as daemon:   trv daemon --preset <slug> --adb-forward");
            println!("Save to file:    trv export <slug> > ~/my-theme.toml");
        }

        Commands::Export { slug } => {
            match find_preset(&slug) {
                None => {
                    eprintln!(
                        "error: unknown preset '{}' — run `trv list` to see available presets",
                        slug
                    );
                    std::process::exit(1);
                }
                Some(toml_str) => {
                    // Re-parse and re-serialize for a clean, normalised output.
                    match parse_theme_toml(toml_str) {
                        Ok(theme) => match serialize_theme(&theme) {
                            Ok(out) => print!("{}", out),
                            Err(e) => {
                                eprintln!("error serializing preset: {}", e);
                                std::process::exit(1);
                            }
                        },
                        Err(e) => {
                            eprintln!("error parsing preset: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Parse a preset by slug.  Returns an error string if not found.
fn resolve_preset(slug: &str) -> Result<trv::theme::model::Theme, String> {
    let toml_str = find_preset(slug)
        .ok_or_else(|| format!("unknown preset '{}' — run `trv list` to see options", slug))?;
    parse_theme_toml(toml_str).map_err(|e| format!("preset '{}' parse error: {}", slug, e))
}

/// Resolve default theme path for daemon/TUI when `--theme` is not provided.
///
/// Order:
/// 1) `~/.config/trv/config.toml` (`theme` key), if present and file exists
/// 2) Ensure `~/.config/trv/themes/dashboard.toml` exists (create from preset)
///
/// The resolved path is written back to config as the default (best effort).
fn resolve_theme_path_from_config_or_default() -> Result<PathBuf, String> {
    if let Some(path) =
        app_config::get_default_theme_path().map_err(|e| format!("failed reading config: {}", e))?
    {
        if path.is_file() {
            return Ok(path);
        }
        eprintln!(
            "warning: configured default theme not found: {} — falling back to dashboard",
            path.display()
        );
    }

    let fallback = ensure_default_tui_theme_path()?;
    if let Err(e) = app_config::set_default_theme_path(&fallback) {
        eprintln!("warning: could not persist default theme in config: {}", e);
    }
    Ok(fallback)
}

/// Ensure the default TUI theme file exists at
/// `~/.config/trv/themes/dashboard.toml`.
///
/// If the file already exists, it is left untouched.
fn ensure_default_tui_theme_path() -> Result<PathBuf, String> {
    let path = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("trv")
        .join("themes")
        .join("dashboard.toml");

    if path.exists() {
        return Ok(path);
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create theme directory {:?}: {}", parent, e))?;
    }

    let dashboard = find_preset("dashboard")
        .ok_or_else(|| "internal error: built-in preset 'dashboard' not found".to_string())?;

    use std::io::Write;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => {
            file.write_all(dashboard.as_bytes())
                .map_err(|e| format!("could not write default theme {:?}: {}", path, e))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Created by another process concurrently; treat as success.
        }
        Err(e) => {
            return Err(format!(
                "could not create default theme file {:?}: {}",
                path, e
            ));
        }
    }

    Ok(path)
}

/// Write a preset TOML to a named temporary file and return its path.
///
/// Used by `trv daemon --preset` so the daemon config can hold a file path.
fn resolve_preset_to_tempfile(slug: &str) -> Result<PathBuf, String> {
    let toml_str = find_preset(slug)
        .ok_or_else(|| format!("unknown preset '{}' — run `trv list` to see options", slug))?;

    let dir = std::env::temp_dir();
    let path = dir.join(format!("trv_preset_{}.toml", slug));
    std::fs::write(&path, toml_str)
        .map_err(|e| format!("could not write preset temp file {:?}: {}", path, e))?;
    Ok(path)
}
