//! A terminal on a remote shell (INTERFACES.md section 7, "Connection" and "Terminal").

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use russh::ChannelMsg;
use russh::ChannelWriteHalf;
use russh::client::Msg;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::chain::{connect_chain, resolve_chain};
use super::pool::{ConnectionPool, SharedConnection};
use crate::error::{AppError, ErrorCode};
use crate::keychain::Keychain;
use crate::prompt::{PromptBroker, Prompter};
use crate::sessions::SessionStore;
use crate::terminal::{
    BackendEvent, MAX_BACKEND_READ, OutputSink, PumpConfig, QUEUE_SIZE, Session, SessionRegistry,
    TermSize, TerminalBackend, TerminalStatus, spawn_pump,
};

/// What an SSH pane needs from the rest of the app.
pub struct SshServices {
    pub keychain: Arc<Keychain>,
    pub broker: Arc<PromptBroker>,
    pub pool: ConnectionPool,
    /// Where `sessions.toml` is.
    pub sessions_file: std::path::PathBuf,
    /// The `known_hosts` file the host key rules use.
    pub known_hosts: std::path::PathBuf,
}

type Writer = ChannelWriteHalf<Msg>;

pub struct SshBackend {
    cancel: CancellationToken,
    writer: tokio::sync::Mutex<Option<Writer>>,
    size: Mutex<TermSize>,
}

fn lost() -> AppError {
    AppError::new(
        ErrorCode::ConnectionLost,
        "The connection to the server was lost.",
    )
}

impl SshBackend {
    /// Opens the shell. Everything that can take a while (connecting, questions to the user,
    /// logging in) happens here, after the session is already registered, so closing the pane
    /// at any point cancels it cleanly.
    async fn establish(
        &self,
        services: &SshServices,
        saved_session_id: &str,
        sink: &Arc<dyn OutputSink>,
    ) -> Result<(russh::ChannelReadHalf, Arc<SharedConnection>), AppError> {
        let path = services.sessions_file.clone();
        let loaded = tokio::task::spawn_blocking(move || SessionStore::new(path).load())
            .await
            .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't read the saved sessions."))?;
        if let Some(problem) = loaded.problem {
            return Err(AppError::new(ErrorCode::InvalidInput, problem));
        }
        let chain = resolve_chain(&loaded.file, saved_session_id)?;
        let prompter = Prompter::new(services.broker.clone(), sink.clone(), self.cancel.clone());
        let shared = services
            .pool
            .get_or_connect(
                saved_session_id,
                connect_chain(services, &chain, None, &prompter),
            )
            .await?;

        let size = *self.size.lock().unwrap();
        let channel = shared
            .handle
            .channel_open_session()
            .await
            .map_err(|_| lost())?;
        channel
            .request_pty(
                false,
                "xterm-256color",
                size.cols.into(),
                size.rows.into(),
                0,
                0,
                &[],
            )
            .await
            .map_err(|_| lost())?;
        channel.request_shell(true).await.map_err(|_| lost())?;
        let (reader, writer) = channel.split();
        *self.writer.lock().await = Some(writer);
        Ok((reader, shared))
    }

    async fn run(
        self: Arc<Self>,
        services: Arc<SshServices>,
        saved_session_id: String,
        sink: Arc<dyn OutputSink>,
        events: mpsc::Sender<BackendEvent>,
    ) {
        let _ = sink.send_status(TerminalStatus::Connecting);
        let opened = tokio::select! {
            result = self.establish(&services, &saved_session_id, &sink) => result,
            _ = self.cancel.cancelled() => return,
        };
        let (mut reader, shared) = match opened {
            Ok(ok) => ok,
            Err(error) => {
                if error.code != ErrorCode::Cancelled {
                    let _ = events.send(BackendEvent::Failed(error)).await;
                }
                return;
            }
        };
        let _ = sink.send_status(TerminalStatus::Connected);

        let mut exit_code = None;
        // The channel's queue ending without a close message means the connection itself went.
        let mut connection_ended = false;
        loop {
            let message = tokio::select! {
                message = reader.wait() => message,
                _ = self.cancel.cancelled() => return,
            };
            match message {
                Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                    for chunk in data.chunks(MAX_BACKEND_READ) {
                        // A full queue makes this wait, which is how slowing down reaches the
                        // server. A closed queue means the pane is gone.
                        if events
                            .send(BackendEvent::Data(chunk.to_vec()))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    exit_code = Some(exit_status as i32);
                }
                Some(ChannelMsg::Close) => break,
                None => {
                    connection_ended = true;
                    break;
                }
                Some(_) => {}
            }
        }

        // The shell ended. If the whole connection went with it, that's a lost connection and
        // only the panes on it say so; otherwise it was a normal end.
        let last = if (connection_ended || shared.handle.is_closed()) && exit_code.is_none() {
            BackendEvent::Failed(lost())
        } else {
            BackendEvent::Exited { code: exit_code }
        };
        let _ = events.send(last).await;
    }
}

#[async_trait]
impl TerminalBackend for SshBackend {
    async fn write(&self, data: &[u8]) -> Result<(), AppError> {
        let guard = self.writer.lock().await;
        // Typing before the shell is open is dropped, as other SSH clients do.
        let Some(writer) = guard.as_ref() else {
            return Ok(());
        };
        writer
            .data_bytes(bytes::Bytes::copy_from_slice(data))
            .await
            .map_err(|_| lost())
    }

    async fn resize(&self, size: TermSize) -> Result<(), AppError> {
        *self.size.lock().unwrap() = size;
        let guard = self.writer.lock().await;
        if let Some(writer) = guard.as_ref() {
            writer
                .window_change(size.cols.into(), size.rows.into(), 0, 0)
                .await
                .map_err(|_| lost())?;
        }
        Ok(())
    }

    async fn close(&self) -> Result<(), AppError> {
        self.cancel.cancel();
        if let Some(writer) = self.writer.lock().await.take() {
            let _ = writer.close().await;
        }
        Ok(())
    }
}

/// Opens a terminal on a saved SSH session. The session exists straight away; connecting
/// carries on in the background and reports through the status messages.
pub async fn open_ssh_terminal(
    registry: &SessionRegistry,
    services: Arc<SshServices>,
    saved_session_id: String,
    size: TermSize,
    sink: Arc<dyn OutputSink>,
) -> Result<String, AppError> {
    let (events, queue) = mpsc::channel(QUEUE_SIZE);
    let backend = Arc::new(SshBackend {
        cancel: CancellationToken::new(),
        writer: tokio::sync::Mutex::new(None),
        size: Mutex::new(size),
    });
    let pump = spawn_pump(queue, Arc::clone(&sink), PumpConfig::default());
    let id = registry.insert(Session::new(backend.clone(), pump));
    tokio::spawn(backend.run(services, saved_session_id, sink, events));
    Ok(id)
}
