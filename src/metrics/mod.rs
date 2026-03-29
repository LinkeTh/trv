/// Metrics subsystem: CPU, GPU, memory polling and collection.
pub mod collector;
pub mod cpu;
pub mod disk;
pub mod fan;
pub mod gpu;
pub mod memory;
pub mod network;

pub use collector::MetricCollector;
