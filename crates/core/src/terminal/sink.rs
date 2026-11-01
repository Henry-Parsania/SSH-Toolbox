use std::sync::{Arc, Mutex};

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

/// A sink whose destination can be changed while the session runs. A tab moved to its own window
/// keeps its sessions: only where their output goes changes.
pub struct SwitchSink {
    inner: Mutex<Arc<dyn OutputSink>>,
}

impl SwitchSink {
    pub fn new(first: Arc<dyn OutputSink>) -> Self {
        Self {
            inner: Mutex::new(first),
        }
    }

    /// From now on, output and statuses go to `next`.
    pub fn switch_to(&self, next: Arc<dyn OutputSink>) {
        *self.inner.lock().unwrap() = next;
    }

    fn current(&self) -> Arc<dyn OutputSink> {
        self.inner.lock().unwrap().clone()
    }
}

impl OutputSink for SwitchSink {
    fn send_data(&self, chunk: Vec<u8>) -> Result<(), AppError> {
        self.current().send_data(chunk)
    }

    fn send_status(&self, status: TerminalStatus) -> Result<(), AppError> {
        self.current().send_status(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::testing::{RecordingSink, Sent};

    #[test]
    fn output_follows_the_latest_destination() {
        let (a, b) = (
            Arc::new(RecordingSink::default()),
            Arc::new(RecordingSink::default()),
        );
        let switch = SwitchSink::new(a.clone());
        switch.send_data(b"one".to_vec()).unwrap();
        switch.switch_to(b.clone());
        switch.send_data(b"two".to_vec()).unwrap();
        switch.send_status(TerminalStatus::Connected).unwrap();
        assert_eq!(a.all().len(), 1);
        let got = b.all();
        assert!(matches!(&got[0], Sent::Data(d) if d == b"two"));
        assert!(matches!(&got[1], Sent::Status(TerminalStatus::Connected)));
    }
}
