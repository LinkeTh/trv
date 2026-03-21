/// Daemon subcommand — initialises device and runs the cmd15 metrics loop.
pub mod config;
pub mod runner;

pub use config::DaemonConfig;
pub use runner::run;
