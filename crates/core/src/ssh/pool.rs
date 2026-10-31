//! One connection per saved session, shared by every pane of that session (INTERFACES.md
//! section 7, "Connection"). The connection closes when its last pane lets go of it.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Weak};

use tokio::sync::Mutex;

use super::connection::Connection;
use crate::error::AppError;

/// A logged-in connection that panes open their channels on.
pub struct SharedConnection {
    pub handle: Connection,
    /// Jump host connections this one runs through. They close when this one does.
    _via: Vec<Connection>,
}

impl SharedConnection {
    /// For code that makes a connection itself instead of going through the pool.
    pub fn new(handle: Connection) -> Arc<Self> {
        Arc::new(Self {
            handle,
            _via: Vec::new(),
        })
    }
}

/// One slot per saved session. Its lock is held while that session connects, which makes
/// other panes of the same session wait for it, but never panes of other sessions.
type Slot = Arc<Mutex<Weak<SharedConnection>>>;

#[derive(Default)]
pub struct ConnectionPool {
    slots: std::sync::Mutex<HashMap<String, Slot>>,
}

impl ConnectionPool {
    pub fn new() -> Self {
        Self::default()
    }

    /// The live connection for `key`, or a new one made by `connect`. Panes that ask while a
    /// connection is being made wait for it instead of logging in again. A connection that has
    /// dropped is never handed out: asking again makes a fresh one.
    pub async fn get_or_connect<F>(
        &self,
        key: &str,
        connect: F,
    ) -> Result<Arc<SharedConnection>, AppError>
    where
        F: Future<Output = Result<(Connection, Vec<Connection>), AppError>>,
    {
        let slot = self
            .slots
            .lock()
            .unwrap()
            .entry(key.to_string())
            .or_default()
            .clone();
        let mut current = slot.lock().await;
        if let Some(shared) = current.upgrade()
            && !shared.handle.is_closed()
        {
            return Ok(shared);
        }
        let (handle, via) = connect.await?;
        let shared = Arc::new(SharedConnection { handle, _via: via });
        *current = Arc::downgrade(&shared);
        Ok(shared)
    }
}
