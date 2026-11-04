use std::sync::Arc;

use tauri::State;
use tauri::ipc::{Channel, InvokeResponseBody};
use toolbox_core::docker;
use toolbox_core::serial::{self, PortInfo, SerialConfig};
use toolbox_core::sessions::SessionStore;
use toolbox_core::ssh::{self, backend::SshServices};
use toolbox_core::terminal::log::{LogSink, log_path, utc_stamp};
use toolbox_core::terminal::{
    self, OutputSink, PortControl, SessionRegistry, SessionSpec, Shell, SwitchSink, TerminalStatus,
    checked_size,
};
use toolbox_core::{AppError, ErrorCode};

/// Open terminal sessions, shared by all commands.
#[derive(Default)]
pub struct Terminals {
    pub sessions: SessionRegistry,
    /// Where each session's output goes, so a session can be shown by another window.
    switches: std::sync::Mutex<std::collections::HashMap<String, Arc<SwitchSink>>>,
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

/// If the user chose a log folder, wraps `sink` so a plain-text copy of the session is saved
/// there. Any problem with the folder or file just means no log: the terminal still opens.
async fn logged(
    sink: Arc<SwitchSink>,
    spec: &SessionSpec,
    sessions_file: &std::path::Path,
) -> Arc<dyn OutputSink> {
    let plain: Arc<dyn OutputSink> = sink;
    if matches!(spec, SessionSpec::Ssh { monitor: true, .. }) {
        return plain;
    }
    let Some(dir) = toolbox_core::config::config_dir() else {
        return plain;
    };
    let settings = toolbox_core::settings::load(&dir.join("settings.toml"));
    if settings.log_folder.trim().is_empty() {
        return plain;
    }
    let folder = std::path::PathBuf::from(settings.log_folder.trim());
    let saved_id = match spec {
        SessionSpec::Ssh {
            saved_session_id, ..
        }
        | SessionSpec::Serial { saved_session_id } => Some(saved_session_id.clone()),
        _ => None,
    };
    let fallback = match spec {
        SessionSpec::Local { shell_id } => shell_id.clone(),
        SessionSpec::Wsl { distro } => distro.clone(),
        SessionSpec::Ssh { .. } => "ssh".into(),
        SessionSpec::Serial { .. } => "serial".into(),
        _ => "docker".into(),
    };
    let sessions_file = sessions_file.to_path_buf();
    let wrapped = plain.clone();
    let made = tokio::task::spawn_blocking(move || {
        let name = saved_id
            .and_then(|id| {
                SessionStore::new(sessions_file)
                    .load()
                    .file
                    .sessions
                    .iter()
                    .find(|s| s.id() == id)
                    .map(|s| s.name().to_string())
            })
            .unwrap_or(fallback);
        let path = log_path(&folder, &name, &utc_stamp()).ok()?;
        LogSink::new(wrapped, &path)
            .ok()
            .map(|l| Arc::new(l) as Arc<dyn OutputSink>)
    })
    .await;
    match made {
        Ok(Some(logging)) => logging,
        _ => plain,
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

// Every piece of state and both channels are separate arguments, as Tauri commands require.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn terminal_open(
    terminals: State<'_, Terminals>,
    services: State<'_, Arc<SshServices>>,
    engines: State<'_, super::docker::Engines>,
    spec: SessionSpec,
    cols: u16,
    rows: u16,
    on_output: Channel<InvokeResponseBody>,
    on_status: Channel<TerminalStatus>,
) -> Result<String, AppError> {
    let size = checked_size(cols, rows)?;
    let switch = Arc::new(SwitchSink::new(Arc::new(ChannelSink {
        output: on_output,
        status: on_status,
    })));
    let sink = logged(switch.clone(), &spec, &services.sessions_file).await;
    let id = match spec {
        SessionSpec::Ssh {
            saved_session_id,
            monitor,
        } => {
            ssh::backend::open_ssh_terminal(
                &terminals.sessions,
                services.inner().clone(),
                saved_session_id,
                monitor,
                size,
                sink,
            )
            .await
        }
        SessionSpec::Serial { saved_session_id } => {
            let path = services.sessions_file.clone();
            let loaded = tokio::task::spawn_blocking(move || SessionStore::new(path).load())
                .await
                .map_err(|_| {
                    AppError::new(ErrorCode::Internal, "Couldn't read the saved sessions.")
                })?;
            if let Some(problem) = loaded.problem {
                return Err(AppError::new(ErrorCode::InvalidInput, problem));
            }
            let session = loaded
                .file
                .sessions
                .iter()
                .find(|s| s.id() == saved_session_id)
                .ok_or_else(|| {
                    AppError::new(ErrorCode::NotFound, "That saved session no longer exists.")
                })?;
            let config = SerialConfig::from_saved(session)?;
            serial::open_serial_terminal(&terminals.sessions, config, sink).await
        }
        SessionSpec::DockerLogs {
            docker_id,
            container_id,
        } => {
            docker::pane::open_logs(
                &terminals.sessions,
                engines.get(&docker_id)?,
                container_id,
                true,
                sink,
            )
            .await
        }
        SessionSpec::DockerShell {
            docker_id,
            container_id,
        } => {
            docker::pane::open_shell(
                &terminals.sessions,
                engines.get(&docker_id)?,
                container_id,
                size,
                sink,
            )
            .await
        }
        other => terminal::open_terminal(&terminals.sessions, other, size, sink).await,
    }?;
    terminals
        .switches
        .lock()
        .unwrap()
        .insert(id.clone(), switch);
    Ok(id)
}

/// Shows an open session in the calling window instead of the one that was showing it. Used
/// when a tab is moved to its own window: the programs keep running, but the new window starts
/// with an empty screen.
#[tauri::command]
pub async fn terminal_attach(
    terminals: State<'_, Terminals>,
    session_id: String,
    on_output: Channel<InvokeResponseBody>,
    on_status: Channel<TerminalStatus>,
) -> Result<String, AppError> {
    let session = terminals.sessions.get(&session_id)?;
    let switch = terminals
        .switches
        .lock()
        .unwrap()
        .get(&session_id)
        .cloned()
        .ok_or_else(|| AppError::new(ErrorCode::NotFound, "That session has ended."))?;
    switch.switch_to(Arc::new(ChannelSink {
        output: on_output,
        status: on_status,
    }));
    session.pump().reset_flow();
    let _ = switch.send_status(TerminalStatus::Connected);
    Ok(session_id)
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
    terminals.switches.lock().unwrap().remove(&session_id);
    terminals.sessions.close(&session_id).await
}

/// The serial ports this computer has right now (R1).
#[tauri::command]
pub async fn serial_list_ports() -> Result<Vec<PortInfo>, AppError> {
    serial::list_ports().await
}

/// Sets the DTR and RTS lines of a serial session. Lines left out stay as they are (R7).
#[tauri::command]
pub async fn serial_set_control(
    terminals: State<'_, Terminals>,
    session_id: String,
    dtr: Option<bool>,
    rts: Option<bool>,
) -> Result<(), AppError> {
    let session = terminals.sessions.get(&session_id)?;
    if let Some(on) = dtr {
        session.control(PortControl::Dtr(on)).await?;
    }
    if let Some(on) = rts {
        session.control(PortControl::Rts(on)).await?;
    }
    Ok(())
}

/// Sends a break on a serial session (R7).
#[tauri::command]
pub async fn serial_send_break(
    terminals: State<'_, Terminals>,
    session_id: String,
) -> Result<(), AppError> {
    terminals
        .sessions
        .get(&session_id)?
        .control(PortControl::Break)
        .await
}

/// Largest output that can be saved, in bytes.
const MAX_SAVED_OUTPUT: usize = 50 * 1024 * 1024;

/// Saves a terminal's text to a new file in the Downloads folder (or the home folder). The name
/// is made here and never reuses an existing file, so nothing can be overwritten. Returns the path.
#[tauri::command]
pub async fn terminal_save_output(text: String) -> Result<String, AppError> {
    if text.len() > MAX_SAVED_OUTPUT {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "That's too much output to save.",
        ));
    }
    tokio::task::spawn_blocking(move || {
        use std::io::Write;
        let dirs = directories::UserDirs::new().ok_or_else(|| {
            AppError::new(ErrorCode::NotFound, "Couldn't find your Downloads folder.")
        })?;
        let folder = dirs
            .download_dir()
            .map(|d| d.to_path_buf())
            .unwrap_or_else(|| dirs.home_dir().to_path_buf());
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        for attempt in 0..100u32 {
            let suffix = if attempt == 0 {
                String::new()
            } else {
                format!("-{attempt}")
            };
            let path = folder.join(format!("ssh-toolbox-output-{stamp}{suffix}.txt"));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    file.write_all(text.as_bytes()).map_err(|_| {
                        AppError::new(ErrorCode::Internal, "Couldn't write the file.")
                    })?;
                    return Ok(path.display().to_string());
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => {
                    return Err(AppError::new(
                        ErrorCode::PermissionDenied,
                        "Couldn't save into the Downloads folder.",
                    ));
                }
            }
        }
        Err(AppError::new(
            ErrorCode::Internal,
            "Couldn't pick a file name.",
        ))
    })
    .await
    .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't save the output."))?
}
