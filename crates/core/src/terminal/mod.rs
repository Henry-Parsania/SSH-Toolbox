//! Terminal sessions: the backend interface, the output pump and the session registry.

mod backend;
mod local;
mod open;
mod pump;
mod registry;
mod shells;
mod sink;
#[cfg(test)]
pub(crate) mod testing;
mod wsl;

pub use backend::{
    BackendEvent, MAX_BACKEND_READ, PortControl, QUEUE_SIZE, TermSize, TerminalBackend,
};
pub use local::{LocalShell, ShellSpec};
pub use open::{SessionSpec, checked_size, open_terminal};
pub use pump::{
    ACK_DELAY, ACK_THRESHOLD, FLUSH_AFTER, HIGH, LOW, MAX_CHUNK, PumpConfig, PumpHandle, spawn_pump,
};
pub use registry::{Session, SessionRegistry};
pub use shells::{Shell, list_shells, shell_spec};
pub use sink::{OutputSink, TerminalStatus};
pub use wsl::{list_wsl_distros, wsl_spec};

#[cfg(test)]
mod local_tests;
#[cfg(test)]
mod pump_tests;
#[cfg(test)]
mod registry_tests;
