use async_trait::async_trait;

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermSize {
    pub cols: u16,
    pub rows: u16,
}

/// What a backend reports through its queue. A backend sends exactly one `Exited` or
/// `Failed` as its last message.
#[derive(Debug)]
pub enum BackendEvent {
    Data(Vec<u8>),
    /// The program ended normally.
    Exited {
        code: Option<i32>,
    },
    /// Connection lost, device unplugged and so on.
    Failed(AppError),
}

/// Number of messages a backend's queue holds. A backend must `await` its sends, so that a
/// full queue stops it reading. That is how slowing down reaches the running program.
pub const QUEUE_SIZE: usize = 32;

/// Largest piece of output a backend sends as one `Data` event.
pub const MAX_BACKEND_READ: usize = 16 * 1024;

#[async_trait]
pub trait TerminalBackend: Send + Sync {
    async fn write(&self, data: &[u8]) -> Result<(), AppError>;
    /// Backends without a terminal size, such as serial, return `Ok(())` and do nothing.
    async fn resize(&self, size: TermSize) -> Result<(), AppError>;
    /// Ends the session. Safe to call twice.
    async fn close(&self) -> Result<(), AppError>;
}
