//! Container logs and container shells, as terminal sessions: they sit in ordinary panes and use
//! the same output flow control as every other terminal.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use bollard::Docker;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::query_parameters::{LogsOptionsBuilder, ResizeExecOptionsBuilder};
use futures_util::StreamExt;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

use super::engine::{check_id, inspect};
use crate::error::{AppError, ErrorCode};
use crate::terminal::{
    BackendEvent, OutputSink, PumpConfig, QUEUE_SIZE, Session, SessionRegistry, TermSize,
    TerminalBackend, TerminalStatus, spawn_pump,
};

/// How many lines of old output come before the live ones.
pub const LOG_TAIL: &str = "500";

/// The shell a container is asked for: bash if it has one, otherwise sh.
const SHELL_COMMAND: [&str; 3] = [
    "/bin/sh",
    "-c",
    "if command -v bash >/dev/null 2>&1; then exec bash; else exec sh; fi",
];

fn lost() -> AppError {
    AppError::new(
        ErrorCode::ConnectionLost,
        "The connection to Docker was lost.",
    )
}

/// Output that has no terminal behind it uses bare line feeds, which a terminal needs as
/// carriage return and line feed.
fn lines_for_terminal(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 8);
    let mut last = 0u8;
    for &b in bytes {
        if b == b'\n' && last != b'\r' {
            out.push(b'\r');
        }
        out.push(b);
        last = b;
    }
    out
}

/// A read-only session showing a container's log.
struct LogsBackend {
    cancel: CancellationToken,
}

#[async_trait]
impl TerminalBackend for LogsBackend {
    // Logs can't be typed into.
    async fn write(&self, _data: &[u8]) -> Result<(), AppError> {
        Ok(())
    }
    async fn resize(&self, _size: TermSize) -> Result<(), AppError> {
        Ok(())
    }
    async fn close(&self) -> Result<(), AppError> {
        self.cancel.cancel();
        Ok(())
    }
}

/// Starts showing a container's log, ending with the live output when `follow` is set.
pub async fn open_logs(
    registry: &SessionRegistry,
    docker: Arc<Docker>,
    container_id: String,
    follow: bool,
    sink: Arc<dyn OutputSink>,
) -> Result<String, AppError> {
    check_id(&container_id)?;
    let tty = inspect(&docker, &container_id).await?.tty;
    let (events, queue) = mpsc::channel(QUEUE_SIZE);
    let cancel = CancellationToken::new();
    let backend = Arc::new(LogsBackend {
        cancel: cancel.clone(),
    });
    tokio::spawn(async move {
        let options = LogsOptionsBuilder::default()
            .follow(follow)
            .stdout(true)
            .stderr(true)
            .tail(LOG_TAIL)
            .build();
        let mut stream = Box::pin(docker.logs(&container_id, Some(options)));
        let last = loop {
            let next = tokio::select! {
                next = stream.next() => next,
                _ = cancel.cancelled() => return,
            };
            match next {
                None => break BackendEvent::Exited { code: None },
                Some(Err(_)) => break BackendEvent::Failed(lost()),
                Some(Ok(output)) => {
                    let bytes = output.into_bytes();
                    let chunk = if tty {
                        bytes.to_vec()
                    } else {
                        lines_for_terminal(&bytes)
                    };
                    if events.send(BackendEvent::Data(chunk)).await.is_err() {
                        return;
                    }
                }
            }
        };
        let _ = events.send(last).await;
    });
    let pump = spawn_pump(queue, Arc::clone(&sink), PumpConfig::default());
    let id = registry.insert(Session::new(backend, pump));
    let _ = sink.send_status(TerminalStatus::Connected);
    Ok(id)
}

type Input = Pin<Box<dyn AsyncWrite + Send>>;

/// A shell running in a container.
struct ExecBackend {
    docker: Arc<Docker>,
    exec_id: String,
    input: Mutex<Option<Input>>,
    cancel: CancellationToken,
}

#[async_trait]
impl TerminalBackend for ExecBackend {
    async fn write(&self, data: &[u8]) -> Result<(), AppError> {
        let mut input = self.input.lock().await;
        let Some(input) = input.as_mut() else {
            return Err(lost());
        };
        input.write_all(data).await.map_err(|_| lost())?;
        input.flush().await.map_err(|_| lost())
    }

    async fn resize(&self, size: TermSize) -> Result<(), AppError> {
        self.docker
            .resize_exec(
                &self.exec_id,
                ResizeExecOptionsBuilder::default()
                    .h(size.rows.into())
                    .w(size.cols.into())
                    .build(),
            )
            .await
            .map_err(|_| lost())
    }

    async fn close(&self) -> Result<(), AppError> {
        self.cancel.cancel();
        // Dropping the input closes the connection, which ends the shell.
        self.input.lock().await.take();
        Ok(())
    }
}

/// Opens a shell in a container: bash if it has one, otherwise sh. Never as another user, and
/// never with extra privileges.
pub async fn open_shell(
    registry: &SessionRegistry,
    docker: Arc<Docker>,
    container_id: String,
    size: TermSize,
    sink: Arc<dyn OutputSink>,
) -> Result<String, AppError> {
    check_id(&container_id)?;
    let created = docker
        .create_exec(
            &container_id,
            CreateExecOptions::<String> {
                attach_stdin: Some(true),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                tty: Some(true),
                env: Some(vec!["TERM=xterm-256color".into()]),
                cmd: Some(SHELL_COMMAND.iter().map(|s| s.to_string()).collect()),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| match e {
            bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            } => AppError::new(ErrorCode::NotFound, "That container no longer exists."),
            bollard::errors::Error::DockerResponseServerError {
                status_code: 409, ..
            } => AppError::new(
                ErrorCode::InvalidInput,
                "The container isn't running, so there's no shell to open.",
            ),
            _ => lost(),
        })?;
    let started = docker
        .start_exec(&created.id, None)
        .await
        .map_err(|_| lost())?;
    let StartExecResults::Attached { mut output, input } = started else {
        return Err(lost());
    };

    let (events, queue) = mpsc::channel(QUEUE_SIZE);
    let cancel = CancellationToken::new();
    let backend = Arc::new(ExecBackend {
        docker: docker.clone(),
        exec_id: created.id.clone(),
        input: Mutex::new(Some(input)),
        cancel: cancel.clone(),
    });
    // The first size is set as soon as the shell exists, since the program may draw at once.
    let _ = backend.resize(size).await;

    let exec_id = created.id;
    tokio::spawn(async move {
        let last = loop {
            let next = tokio::select! {
                next = output.next() => next,
                _ = cancel.cancelled() => return,
            };
            match next {
                Some(Ok(chunk)) => {
                    if events
                        .send(BackendEvent::Data(chunk.into_bytes().to_vec()))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                Some(Err(_)) => break BackendEvent::Failed(lost()),
                None => {
                    let code = docker
                        .inspect_exec(&exec_id)
                        .await
                        .ok()
                        .and_then(|e| e.exit_code)
                        .and_then(|c| i32::try_from(c).ok());
                    break BackendEvent::Exited { code };
                }
            }
        };
        let _ = events.send(last).await;
    });
    let pump = spawn_pump(queue, Arc::clone(&sink), PumpConfig::default());
    let id = registry.insert(Session::new(backend, pump));
    let _ = sink.send_status(TerminalStatus::Connected);
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::lines_for_terminal;

    #[test]
    fn bare_line_feeds_become_carriage_return_and_line_feed() {
        assert_eq!(lines_for_terminal(b"a\nb\r\nc\n"), b"a\r\nb\r\nc\r\n");
        assert_eq!(lines_for_terminal(b""), b"");
    }
}
