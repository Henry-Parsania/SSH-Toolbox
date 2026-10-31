use toolbox_core::config::config_dir;
use toolbox_core::sessions::{Loaded, SessionStore, SessionsFile};
use toolbox_core::{AppError, ErrorCode};

fn store() -> Result<SessionStore, AppError> {
    config_dir()
        .map(|dir| SessionStore::new(dir.join("sessions.toml")))
        .ok_or_else(|| AppError::new(ErrorCode::Internal, "Couldn't find the settings folder."))
}

fn task_failed() -> AppError {
    AppError::new(ErrorCode::Internal, "Couldn't reach the saved sessions.")
}

/// The saved sessions, never with secrets. If the file can't be used, `problem` says so.
#[tauri::command]
pub async fn sessions_get() -> Result<Loaded, AppError> {
    let store = store()?;
    tokio::task::spawn_blocking(move || store.load())
        .await
        .map_err(|_| task_failed())
}

#[tauri::command]
pub async fn sessions_save(file: SessionsFile) -> Result<(), AppError> {
    let store = store()?;
    tokio::task::spawn_blocking(move || store.save(&file))
        .await
        .map_err(|_| task_failed())?
}

/// The user chose to start again: the unreadable file is kept as `sessions.toml.bad`.
#[tauri::command]
pub async fn sessions_set_aside_unreadable() -> Result<(), AppError> {
    let store = store()?;
    tokio::task::spawn_blocking(move || store.set_aside_unreadable())
        .await
        .map_err(|_| task_failed())
}
