use std::path::PathBuf;

use toolbox_core::config::config_dir;
use toolbox_core::layout::{self, Layout};
use toolbox_core::{AppError, ErrorCode};

fn layout_path() -> Result<PathBuf, AppError> {
    config_dir()
        .map(|dir| dir.join("layout.toml"))
        .ok_or_else(|| AppError::new(ErrorCode::Internal, "Couldn't find the settings folder."))
}

fn task_failed() -> AppError {
    AppError::new(ErrorCode::Internal, "Couldn't reach the saved layout.")
}

/// The saved layout, or nothing if there isn't a usable one.
#[tauri::command]
pub async fn layout_get() -> Result<Option<Layout>, AppError> {
    let path = layout_path()?;
    tokio::task::spawn_blocking(move || layout::load(&path))
        .await
        .map_err(|_| task_failed())
}

#[tauri::command]
pub async fn layout_set(layout: Layout) -> Result<(), AppError> {
    let path = layout_path()?;
    tokio::task::spawn_blocking(move || layout::save(&path, &layout))
        .await
        .map_err(|_| task_failed())?
}
