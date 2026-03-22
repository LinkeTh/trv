/// Event loop for the TUI.
///
/// `spawn_event_threads` launches an input reader, a tick timer, and a metrics
/// polling thread on background threads, returning the event receiver and a
/// shutdown flag.  `run_loop` takes the receiver and drives the main draw/handle
/// cycle.
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use crossbeam_channel::{Receiver, Sender, bounded, select};
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, MouseEvent};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::metrics::collector::MetricCollector;
use crate::theme::model::MetricSource;

use super::app::App;
use super::ui;

/// Log the first `event::poll` failure once; subsequent failures are silent
/// to avoid log spam on unusual terminal environments.
static POLL_ERROR_LOGGED: OnceLock<()> = OnceLock::new();

/// Tick interval: controls how often the screen redraws with no input.
const TICK_RATE: Duration = Duration::from_millis(200);

/// Metrics polling interval.
const METRICS_RATE: Duration = Duration::from_millis(1_000);

/// A snapshot of live metric readings keyed by human-readable source name.
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    /// Map from `MetricSource` debug label → formatted string (e.g. "42.3°C").
    pub values: HashMap<String, String>,
}

/// Events sent on the channel from the input/tick threads to the main loop.
#[derive(Debug)]
pub enum Event {
    /// A keyboard event from crossterm.
    Key(KeyEvent),
    /// Bracketed paste payload.
    Paste(String),
    /// A mouse event from crossterm.
    Mouse(MouseEvent),
    /// Periodic tick — redraw even without input.
    Tick,
    /// Terminal resize (new cols, new rows).
    Resize(u16, u16),
    /// Fresh metrics readings from the background poller.
    MetricsUpdate(MetricsSnapshot),
}

/// Spawn the input-reader, tick-timer, and metrics-polling threads.
///
/// Returns `(tx, rx, quit_flag)`.  The caller holds the `Sender` (unused
/// directly, but keeping it alive prevents channel closure) and passes the
/// `Receiver` to `run_loop`.  Set `quit_flag` to `true` to stop all threads.
pub fn spawn_event_threads() -> (Sender<Event>, Receiver<Event>, Arc<AtomicBool>) {
    let (tx, rx): (Sender<Event>, Receiver<Event>) = bounded(256);
    let quit = Arc::new(AtomicBool::new(false));

    // --- Input thread ---
    let tx_input = tx.clone();
    let quit_input = Arc::clone(&quit);
    thread::spawn(move || {
        while !quit_input.load(Ordering::Relaxed) {
            match event::poll(Duration::from_millis(50)) {
                Ok(true) => match event::read() {
                    Ok(CrosstermEvent::Key(k)) => {
                        let _ = tx_input.send(Event::Key(k));
                    }
                    Ok(CrosstermEvent::Paste(s)) => {
                        let _ = tx_input.send(Event::Paste(s));
                    }
                    Ok(CrosstermEvent::Mouse(m)) => {
                        let _ = tx_input.send(Event::Mouse(m));
                    }
                    Ok(CrosstermEvent::Resize(cols, rows)) => {
                        let _ = tx_input.send(Event::Resize(cols, rows));
                    }
                    _ => {}
                },
                Ok(false) => {}
                Err(_e) => {
                    POLL_ERROR_LOGGED.get_or_init(|| {
                        // Use eprintln since the TUI owns stdout/stderr
                        eprintln!("[trv] event::poll error — input may be degraded");
                    });
                    // Back off to avoid CPU spin on repeated terminal errors.
                    thread::sleep(Duration::from_millis(500));
                }
            }
        }
    });

    // --- Tick thread ---
    let tx_tick = tx.clone();
    let quit_tick = Arc::clone(&quit);
    thread::spawn(move || {
        while !quit_tick.load(Ordering::Relaxed) {
            thread::sleep(TICK_RATE);
            let _ = tx_tick.send(Event::Tick);
        }
    });

    // --- Metrics polling thread ---
    let tx_metrics = tx.clone();
    let quit_metrics = Arc::clone(&quit);
    thread::spawn(move || {
        let mut collector = MetricCollector::new(0.0);
        collector.prime();
        // Brief initial sleep so the first CPU usage sample is meaningful.
        thread::sleep(Duration::from_millis(500));

        // Collect all known metric sources once at startup.
        let all_sources: Vec<(String, MetricSource)> = [
            ("cpu_temp", MetricSource::CpuTemp),
            ("cpu_usage", MetricSource::CpuUsage),
            ("mem_usage", MetricSource::MemUsage),
            ("gpu_temp", MetricSource::GpuTemp),
            ("gpu_usage", MetricSource::GpuUsage),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();

        while !quit_metrics.load(Ordering::Relaxed) {
            let raw = collector.collect(&all_sources);

            // Format values into human-readable strings.
            let values: HashMap<String, String> = raw
                .into_iter()
                .map(|(k, v)| {
                    let s = match k.as_str() {
                        "cpu_temp" => format!("{:.1}°C", v),
                        "gpu_temp" => format!("{:.0}°C", v),
                        "cpu_usage" | "gpu_usage" | "mem_usage" => format!("{:.1}%", v),
                        _ => format!("{:.1}", v),
                    };
                    (k, s)
                })
                .collect();

            let snapshot = MetricsSnapshot { values };
            if tx_metrics.send(Event::MetricsUpdate(snapshot)).is_err() {
                break;
            }

            thread::sleep(METRICS_RATE);
        }
    });

    (tx, rx, quit)
}

/// Run the main event loop until the app signals it should quit.
///
/// Accepts a pre-created event `Receiver` (from `spawn_event_threads`) so the
/// caller can optionally inject additional event sources in the future.
pub fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    rx: &Receiver<Event>,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        let evt = select! {
            recv(rx) -> msg => match msg {
                Ok(e) => e,
                Err(_) => break,
            }
        };

        match evt {
            Event::Key(k) => {
                app.handle_key(k);
            }
            Event::Paste(s) => app.handle_paste(&s),
            Event::Mouse(m) => app.handle_mouse(m),
            Event::Resize(_, _) => {}
            Event::Tick => {}
            Event::MetricsUpdate(snapshot) => {
                app.metrics = snapshot;
            }
        }

        // Drain any extra pending events before redrawing.
        while let Ok(next_evt) = rx.try_recv() {
            match next_evt {
                Event::Key(k) => app.handle_key(k),
                Event::Paste(s) => app.handle_paste(&s),
                Event::Mouse(m) => app.handle_mouse(m),
                Event::Resize(_, _) => {}
                Event::Tick => {}
                Event::MetricsUpdate(snapshot) => app.metrics = snapshot,
            }

            if app.should_quit {
                break;
            }
        }

        app.poll_push_result();

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Decode a `KeyCode` to a human-readable label used in the help overlay.
pub fn key_label(code: KeyCode) -> &'static str {
    match code {
        KeyCode::Tab => "Tab",
        KeyCode::BackTab => "Shift+Tab",
        KeyCode::Enter => "Enter",
        KeyCode::Esc => "Esc",
        KeyCode::F(1) => "F1",
        KeyCode::Char('q') => "q",
        KeyCode::Char('?') => "?",
        _ => "?",
    }
}
