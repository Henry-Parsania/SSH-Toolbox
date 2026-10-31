use std::path::PathBuf;

use toolbox_core::config::config_dir;
use toolbox_core::settings::{self, Settings};
use toolbox_core::{AppError, ErrorCode};

fn settings_path() -> Result<PathBuf, AppError> {
    config_dir()
        .map(|dir| dir.join("settings.toml"))
        .ok_or_else(|| AppError::new(ErrorCode::Internal, "Couldn't find the settings folder."))
}

fn task_failed() -> AppError {
    AppError::new(ErrorCode::Internal, "Couldn't reach the settings.")
}

#[tauri::command]
pub async fn settings_get() -> Result<Settings, AppError> {
    let path = settings_path()?;
    tokio::task::spawn_blocking(move || settings::load(&path))
        .await
        .map_err(|_| task_failed())
}

#[tauri::command]
pub async fn settings_set(settings: Settings) -> Result<(), AppError> {
    let path = settings_path()?;
    tokio::task::spawn_blocking(move || settings::save(&path, &settings))
        .await
        .map_err(|_| task_failed())?
}
