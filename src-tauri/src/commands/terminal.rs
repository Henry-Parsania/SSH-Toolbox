use std::sync::Arc;

use tauri::State;
use tauri::ipc::{Channel, InvokeResponseBody};
use toolbox_core::ssh::{self, backend::SshServices};
use toolbox_core::terminal::{
    self, OutputSink, SessionRegistry, SessionSpec, Shell, TerminalStatus, checked_size,
};
use toolbox_core::{AppError, ErrorCode};

/// Open terminal sessions, shared by all commands.
#[derive(Default)]
pub struct Terminals {
    pub sessions: SessionRegistry,
}

/// Delivers a session's output through Tauri channels. Output goes as raw bytes, never as
/// events, which aren't built for high throughput.
struct ChannelSink {
    output: Channel<InvokeResponseBody>,
    status: Channel<TerminalStatus>,
}

fn send_failed() -> AppError {
    AppError::new(
        ErrorCode::Internal,
        "The window stopped listening to the terminal.",
    )
}

impl OutputSink for ChannelSink {
    fn send_data(&self, chunk: Vec<u8>) -> Result<(), AppError> {
        self.output
            .send(InvokeResponseBody::Raw(chunk))
            .map_err(|_| send_failed())
    }

    fn send_status(&self, status: TerminalStatus) -> Result<(), AppError> {
        self.status.send(status).map_err(|_| send_failed())
    }
}

#[tauri::command]
pub async fn list_shells() -> Result<Vec<Shell>, AppError> {
    tokio::task::spawn_blocking(terminal::list_shells)
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't look for shells."))
}

#[tauri::command]
pub async fn list_wsl_distros() -> Result<Vec<String>, AppError> {
    tokio::task::spawn_blocking(terminal::list_wsl_distros)
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't look for WSL."))
}

#[tauri::command]
pub async fn terminal_open(
    terminals: State<'_, Terminals>,
    services: State<'_, Arc<SshServices>>,
    spec: SessionSpec,
    cols: u16,
    rows: u16,
    on_output: Channel<InvokeResponseBody>,
    on_status: Channel<TerminalStatus>,
) -> Result<String, AppError> {
    let size = checked_size(cols, rows)?;
    let sink = Arc::new(ChannelSink {
        output: on_output,
        status: on_status,
    });
    match spec {
        SessionSpec::Ssh { saved_session_id } => {
            ssh::backend::open_ssh_terminal(
                &terminals.sessions,
                services.inner().clone(),
                saved_session_id,
                size,
                sink,
            )
            .await
        }
        other => terminal::open_terminal(&terminals.sessions, other, size, sink).await,
    }
}

#[tauri::command]
pub async fn terminal_write(
    terminals: State<'_, Terminals>,
    session_id: String,
    data: Vec<u8>,
) -> Result<(), AppError> {
    terminals.sessions.get(&session_id)?.write(&data).await
}

#[tauri::command]
pub async fn terminal_resize(
    terminals: State<'_, Terminals>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), AppError> {
    let size = checked_size(cols, rows)?;
    terminals.sessions.get(&session_id)?.resize(size).await
}

#[tauri::command]
pub async fn terminal_ack(
    terminals: State<'_, Terminals>,
    session_id: String,
    bytes: usize,
) -> Result<(), AppError> {
    terminals.sessions.get(&session_id)?.ack(bytes);
    Ok(())
}

#[tauri::command]
pub async fn terminal_close(
    terminals: State<'_, Terminals>,
    session_id: String,
) -> Result<(), AppError> {
    terminals.sessions.close(&session_id).await
}
