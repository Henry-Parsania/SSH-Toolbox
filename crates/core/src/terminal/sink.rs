use serde::Serialize;

use crate::error::AppError;

/// Status messages for the interface. Prompts are added in phase 2.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalStatus {
    Connecting,
    Connected,
    Exited { code: Option<i32> },
    Failed { error: AppError },
}

/// Where the pump delivers output. `src-tauri` implements this with Tauri channels.
pub trait OutputSink: Send + Sync + 'static {
    fn send_data(&self, chunk: Vec<u8>) -> Result<(), AppError>;
    fn send_status(&self, status: TerminalStatus) -> Result<(), AppError>;
}
