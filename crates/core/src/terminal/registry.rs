use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::backend::{TermSize, TerminalBackend};
use super::pump::PumpHandle;
use crate::error::{AppError, ErrorCode};

/// One open terminal session: its backend and its output pump.
pub struct Session {
    backend: Arc<dyn TerminalBackend>,
    pump: PumpHandle,
}

impl Session {
    pub fn new(backend: Arc<dyn TerminalBackend>, pump: PumpHandle) -> Self {
        Self { backend, pump }
    }

    pub async fn write(&self, data: &[u8]) -> Result<(), AppError> {
        self.backend.write(data).await
    }

    pub async fn resize(&self, size: TermSize) -> Result<(), AppError> {
        self.backend.resize(size).await
    }

    /// The interface reports that it has processed `bytes` of output.
    pub fn ack(&self, bytes: usize) {
        self.pump.ack(bytes);
    }

    pub fn pump(&self) -> &PumpHandle {
        &self.pump
    }

    async fn close(&self) -> Result<(), AppError> {
        self.pump.stop();
        self.backend.close().await
    }
}

/// Open sessions, by ID.
#[derive(Default)]
pub struct SessionRegistry {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

fn not_found() -> AppError {
    AppError::new(
        ErrorCode::NotFound,
        "That terminal session isn't open any more.",
    )
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a session and returns its new ID.
    pub fn insert(&self, session: Session) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.sessions
            .lock()
            .expect("session registry lock")
            .insert(id.clone(), Arc::new(session));
        id
    }

    pub fn get(&self, id: &str) -> Result<Arc<Session>, AppError> {
        self.sessions
            .lock()
            .expect("session registry lock")
            .get(id)
            .cloned()
            .ok_or_else(not_found)
    }

    pub fn len(&self) -> usize {
        self.sessions.lock().expect("session registry lock").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes a session, stops its pump and closes its backend. Closing an ID that is already
    /// gone is not an error, so a double close from the interface is harmless.
    pub async fn close(&self, id: &str) -> Result<(), AppError> {
        let session = self
            .sessions
            .lock()
            .expect("session registry lock")
            .remove(id);
        match session {
            Some(session) => session.close().await,
            None => Ok(()),
        }
    }

    /// Closes every session, for app shutdown.
    pub async fn close_all(&self) {
        let sessions: Vec<_> = self
            .sessions
            .lock()
            .expect("session registry lock")
            .drain()
            .map(|(_, session)| session)
            .collect();
        for session in sessions {
            let _ = session.close().await;
        }
    }
}
