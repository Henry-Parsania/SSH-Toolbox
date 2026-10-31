//! Local shell backend, using a pseudo-terminal from `portable-pty` (INTERFACES.md section 3).
//!
//! The pty's reads and writes block, so reading runs on a dedicated thread and writing on
//! `spawn_blocking`. A second thread waits for the program to end, because on Windows the
//! reader only sees the end of the output once the pseudo-terminal has been closed.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc as std_mpsc};

use async_trait::async_trait;
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::sync::mpsc;

use super::backend::{BackendEvent, MAX_BACKEND_READ, TermSize, TerminalBackend};
use crate::error::{AppError, ErrorCode};

/// A program to run in a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellSpec {
    pub program: String,
    pub args: Vec<String>,
}

impl ShellSpec {
    pub fn new(program: impl Into<String>, args: &[&str]) -> Self {
        Self {
            program: program.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
        }
    }
}

fn pty_size(size: TermSize) -> PtySize {
    PtySize {
        rows: size.rows,
        cols: size.cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn home_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf())
}

/// Turns a failure to start the program into a plain message. On Windows `portable-pty` reports
/// a missing program as text rather than a typed error, and both systems use code 2 for it.
fn spawn_error(program: &str, err: Box<dyn std::error::Error + Send + Sync>) -> AppError {
    let detail = err.to_string().replace('\0', "");
    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(err.as_ref());
    let mut typed_not_found = false;
    while let Some(current) = cause {
        if let Some(io) = current.downcast_ref::<std::io::Error>() {
            typed_not_found |= io.kind() == std::io::ErrorKind::NotFound;
        }
        cause = current.source();
    }
    // On Linux, portable-pty searches the path itself and says "No viable candidates found".
    let missing = typed_not_found
        || detail.contains("os error 2")
        || detail.contains("No viable candidates found");
    if missing {
        AppError::new(
            ErrorCode::NotFound,
            format!("Couldn't find {program}. Check that it is installed."),
        )
    } else {
        AppError::new(
            ErrorCode::ConnectionFailed,
            format!("Couldn't start {program}: {detail}"),
        )
    }
}

struct Inner {
    /// Dropped once the program has ended, which is what ends the output on Windows.
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    writer: Mutex<Option<Box<dyn Write + Send>>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
}

pub struct LocalShell {
    inner: Arc<Inner>,
}

impl LocalShell {
    /// Starts `spec` in a new terminal of `size`, in the user's home folder. Output and the
    /// final `Exited` event arrive on `events`.
    pub fn spawn(
        spec: &ShellSpec,
        size: TermSize,
        events: mpsc::Sender<BackendEvent>,
    ) -> Result<Self, AppError> {
        let start_failed = |err: &dyn std::fmt::Display, code: ErrorCode| {
            AppError::new(code, format!("Couldn't start {}: {err}", spec.program))
        };

        let pair = native_pty_system()
            .openpty(pty_size(size))
            .map_err(|e| start_failed(&e, ErrorCode::Internal))?;

        let mut command = CommandBuilder::new(&spec.program);
        command.args(&spec.args);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        if let Some(home) = home_dir() {
            command.cwd(home);
        }

        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| spawn_error(&spec.program, e.into()))?;
        // The child holds its own end. Keeping ours open would stop the output from ending.
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| start_failed(&e, ErrorCode::Internal))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| start_failed(&e, ErrorCode::Internal))?;

        let inner = Arc::new(Inner {
            master: Mutex::new(Some(pair.master)),
            writer: Mutex::new(Some(writer)),
            killer: Mutex::new(child.clone_killer()),
        });

        // Waiter: reports the exit code, then closes the terminal so the reader sees the end.
        let (code_tx, code_rx) = std_mpsc::channel::<Option<i32>>();
        let waiter_inner = Arc::clone(&inner);
        std::thread::Builder::new()
            .name("local-shell-wait".into())
            .spawn(move || {
                let code = child.wait().ok().map(|status| status.exit_code() as i32);
                let _ = code_tx.send(code);
                waiter_inner.writer.lock().expect("writer lock").take();
                waiter_inner.master.lock().expect("master lock").take();
            })
            .map_err(|e| start_failed(&e, ErrorCode::Internal))?;

        // Reader: pass output on, then send exactly one `Exited` as the last message.
        std::thread::Builder::new()
            .name("local-shell-read".into())
            .spawn(move || read_output(reader, events, code_rx))
            .map_err(|e| start_failed(&e, ErrorCode::Internal))?;

        Ok(Self { inner })
    }
}

fn read_output(
    mut reader: Box<dyn Read + Send>,
    events: mpsc::Sender<BackendEvent>,
    exit_code: std_mpsc::Receiver<Option<i32>>,
) {
    let mut buf = vec![0u8; MAX_BACKEND_READ];
    loop {
        match reader.read(&mut buf) {
            // End of output, or the usual error when the other side has closed.
            Ok(0) | Err(_) => break,
            Ok(n) => {
                // Waiting here when the queue is full is what slows the program down. An error
                // means the session was closed, so there is no one left to tell.
                if events
                    .blocking_send(BackendEvent::Data(buf[..n].to_vec()))
                    .is_err()
                {
                    return;
                }
            }
        }
    }
    let code = exit_code.recv().ok().flatten();
    let _ = events.blocking_send(BackendEvent::Exited { code });
}

#[async_trait]
impl TerminalBackend for LocalShell {
    async fn write(&self, data: &[u8]) -> Result<(), AppError> {
        let inner = Arc::clone(&self.inner);
        let data = data.to_vec();
        // A write can block if the program isn't reading, so it can't run on the async threads.
        tokio::task::spawn_blocking(move || {
            let mut writer = inner.writer.lock().expect("writer lock");
            match writer.as_mut() {
                Some(writer) => writer.write_all(&data).and_then(|_| writer.flush()),
                // The program has ended. Keys typed after that are dropped quietly.
                None => Ok(()),
            }
        })
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't send that to the shell."))?
        .map_err(|e| {
            AppError::new(
                ErrorCode::ConnectionLost,
                format!("Couldn't send that to the shell: {e}"),
            )
        })
    }

    async fn resize(&self, size: TermSize) -> Result<(), AppError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let master = inner.master.lock().expect("master lock");
            match master.as_ref() {
                Some(master) => master.resize(pty_size(size)).map_err(|e| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!("Couldn't resize the terminal: {e}"),
                    )
                }),
                None => Ok(()),
            }
        })
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't resize the terminal."))?
    }

    async fn close(&self) -> Result<(), AppError> {
        // Killing an already ended program fails, which is fine: this is safe to call twice.
        let _ = self.inner.killer.lock().expect("killer lock").kill();
        Ok(())
    }
}
