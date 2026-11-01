//! The server status the interface shows: which saved servers answer, and the footer's readings.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::State;
use toolbox_core::reach::{self, Reach, Target};
use toolbox_core::sessions::{SavedSession, SessionStore};
use toolbox_core::ssh::backend::SshServices;
use toolbox_core::ssh::install::run_exec;
use toolbox_core::stats::{self, CpuTimes, LocalStats, Stats};
use toolbox_core::{AppError, ErrorCode};

/// The last processor reading for each connected session, and this computer's own readings.
pub struct StatsState {
    local: Mutex<LocalStats>,
    remote: Mutex<HashMap<String, CpuTimes>>,
}

impl Default for StatsState {
    fn default() -> Self {
        Self {
            local: Mutex::new(LocalStats::new()),
            remote: Mutex::default(),
        }
    }
}

/// Tries to connect to the port of each saved server, a few at a time, and says which answered.
/// The servers come from the saved sessions, not from the caller, so this can't be used to try
/// other addresses. Sessions that go through a jump host aren't tried: their address may not
/// be reachable directly.
#[tauri::command]
pub async fn reach_check(services: State<'_, Arc<SshServices>>) -> Result<Vec<Reach>, AppError> {
    let path = services.sessions_file.clone();
    let loaded = tokio::task::spawn_blocking(move || SessionStore::new(path).load())
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't read the saved sessions."))?;
    let targets: Vec<Target> = loaded
        .file
        .sessions
        .iter()
        .filter_map(|s| match s {
            SavedSession::Ssh {
                id,
                host,
                port,
                jump,
                ..
            } if jump.is_empty() => Some(Target {
                id: id.clone(),
                host: host.clone(),
                port: *port,
            }),
            SavedSession::Ftp { id, host, port, .. } => Some(Target {
                id: id.clone(),
                host: host.clone(),
                port: *port,
            }),
            _ => None,
        })
        .collect();
    Ok(reach::check_all(targets, reach::TIMEOUT).await)
}

/// The footer's readings of this computer.
#[tauri::command]
pub async fn stats_local(state: State<'_, StatsState>) -> Result<Stats, AppError> {
    let home = toolbox_core::config::home_dir();
    Ok(state.local.lock().unwrap().read(home.as_deref()))
}

#[derive(Serialize)]
pub struct RemoteStats {
    /// `None` when the server isn't Linux-like, or the session isn't connected.
    pub stats: Option<Stats>,
}

/// The footer's readings of a connected SSH session's server. Never connects by itself.
#[tauri::command]
pub async fn stats_remote(
    state: State<'_, StatsState>,
    services: State<'_, Arc<SshServices>>,
    saved_session_id: String,
) -> Result<RemoteStats, AppError> {
    let Some(shared) = services.pool.existing(&saved_session_id).await else {
        return Ok(RemoteStats { stats: None });
    };
    let output = run_exec(&shared.handle, stats::REMOTE_COMMAND, None)
        .await
        .map_err(|_| AppError::new(ErrorCode::ConnectionLost, "Couldn't read the server."))?;
    let previous = state.remote.lock().unwrap().get(&saved_session_id).copied();
    let parsed = stats::parse_linux(&output.stdout, previous);
    let mut remote = state.remote.lock().unwrap();
    Ok(match parsed {
        Some((stats, times)) => {
            remote.insert(saved_session_id, times);
            RemoteStats { stats: Some(stats) }
        }
        None => RemoteStats { stats: None },
    })
}
