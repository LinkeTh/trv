/// TUI module — interactive theme editor.
///
/// Entry point: [`run`] sets up the terminal, launches the event loop,
/// and restores the terminal on exit.
pub mod app;
pub mod canvas;
pub mod event;
pub mod fields;
pub mod input;
pub mod palette;
pub mod ui;

use std::path::PathBuf;

use anyhow::Result;
use crossterm::{
    event::{DisableBracketedPaste, EnableBracketedPaste},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::theme::model::Theme;
use crate::theme::toml::load_theme_file;

/// Launch the TUI.
///
/// - `theme`: optional pre-loaded Theme (takes precedence over `theme_path`)
/// - `theme_path`: optional path to theme TOML to load on startup (used when
///   `theme` is `None`; also stored as the default save path)
/// - `host` / `port`: device connection for push-to-device
/// - `recv_timeout_ms`: per-frame receive timeout for device push
pub fn run(
    theme: Option<Theme>,
    theme_path: Option<PathBuf>,
    host: String,
    port: u16,
    recv_timeout_ms: u64,
) -> Result<()> {
    // Resolve theme: prefer pre-loaded, fall back to loading from path.
    let (resolved_theme, resolved_path) = match theme {
        Some(t) => (Some(t), theme_path),
        None => {
            if let Some(ref path) = theme_path {
                match load_theme_file(path) {
                    Ok(t) => (Some(t), Some(path.clone())),
                    Err(e) => {
                        return Err(e.context(format!("loading theme {}", path.display())));
                    }
                }
            } else {
                (None, None)
            }
        }
    };

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;

    // Install a panic hook that restores the terminal before printing the
    // panic message, so the terminal is never left in a broken state.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            std::io::stdout(),
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        original_hook(info);
    }));

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = app::App::new(resolved_theme, resolved_path, host, port, recv_timeout_ms);

    // Spawn background threads and run the event loop
    let (tx, rx, quit) = event::spawn_event_threads();
    let result = event::run_loop(&mut terminal, &mut app, &rx);

    // Stop any active push worker.
    app.shutdown();

    // Signal background threads to stop
    use std::sync::atomic::Ordering;
    quit.store(true, Ordering::Relaxed);
    drop(tx);

    // Always restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableBracketedPaste,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    result
}
