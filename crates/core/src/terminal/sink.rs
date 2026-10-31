use serde::Serialize;

use crate::error::AppError;
use crate::prompt::Prompt;

/// Status messages for the interface.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalStatus {
    Connecting,
    Connected,
    Exited {
        code: Option<i32>,
    },
    Failed {
        error: AppError,
    },
    /// A question for the user. The answer goes back with `prompt_respond`.
    Prompt {
        prompt_id: String,
        prompt: Prompt,
    },
}

/// Where the pump delivers output. `src-tauri` implements this with Tauri channels.
pub trait OutputSink: Send + Sync + 'static {
    fn send_data(&self, chunk: Vec<u8>) -> Result<(), AppError>;
    fn send_status(&self, status: TerminalStatus) -> Result<(), AppError>;
}
