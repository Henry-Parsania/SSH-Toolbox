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

#[derive(Default)]
pub struct ConnectionPool {
    entries: Mutex<HashMap<String, Weak<SharedConnection>>>,
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
        let mut entries = self.entries.lock().await;
        if let Some(shared) = entries.get(key).and_then(Weak::upgrade)
            && !shared.handle.is_closed()
        {
            return Ok(shared);
        }
        let (handle, via) = connect.await?;
        let shared = Arc::new(SharedConnection { handle, _via: via });
        entries.insert(key.to_string(), Arc::downgrade(&shared));
        entries.retain(|_, weak| weak.strong_count() > 0);
        Ok(shared)
    }
}
