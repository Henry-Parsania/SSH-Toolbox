use std::path::PathBuf;

use toolbox_core::config::config_dir;
use toolbox_core::snippets::{self, Snippets};
use toolbox_core::{AppError, ErrorCode};

fn snippets_path() -> Result<PathBuf, AppError> {
    config_dir()
        .map(|dir| dir.join("snippets.toml"))
        .ok_or_else(|| AppError::new(ErrorCode::Internal, "Couldn't find the settings folder."))
}

fn task_failed() -> AppError {
    AppError::new(ErrorCode::Internal, "Couldn't reach the saved snippets.")
}

#[tauri::command]
pub async fn snippets_get() -> Result<Snippets, AppError> {
    let path = snippets_path()?;
    tokio::task::spawn_blocking(move || snippets::load(&path))
        .await
        .map_err(|_| task_failed())
}

#[tauri::command]
pub async fn snippets_set(snippets: Snippets) -> Result<(), AppError> {
    let path = snippets_path()?;
    tokio::task::spawn_blocking(move || snippets::save(&path, &snippets))
        .await
        .map_err(|_| task_failed())?
}
