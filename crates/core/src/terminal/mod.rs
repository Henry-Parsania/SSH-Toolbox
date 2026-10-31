//! Terminal sessions: the backend interface, the output pump and the session registry.

mod backend;
mod local;
mod pump;
mod registry;
mod sink;
#[cfg(test)]
mod testing;

pub use backend::{BackendEvent, MAX_BACKEND_READ, QUEUE_SIZE, TermSize, TerminalBackend};
pub use local::{LocalShell, ShellSpec};
pub use pump::{
    ACK_DELAY, ACK_THRESHOLD, FLUSH_AFTER, HIGH, LOW, MAX_CHUNK, PumpConfig, PumpHandle, spawn_pump,
};
pub use registry::{Session, SessionRegistry};
pub use sink::{OutputSink, TerminalStatus};

#[cfg(test)]
mod local_tests;
#[cfg(test)]
mod pump_tests;
#[cfg(test)]
mod registry_tests;
