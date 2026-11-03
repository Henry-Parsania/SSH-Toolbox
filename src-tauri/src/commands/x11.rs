use std::sync::Arc;

use tauri::State;
use toolbox_core::AppError;
use toolbox_core::x11::{X11Local, X11Status};

/// Whether windows from programs on a server can be shown on this computer.
#[tauri::command]
pub async fn x11_status(x11: State<'_, Arc<X11Local>>) -> Result<X11Status, AppError> {
    Ok(x11.status().await)
}

/// Starts the X server now instead of when the first session needs it.
#[tauri::command]
pub async fn x11_start(x11: State<'_, Arc<X11Local>>) -> Result<X11Status, AppError> {
    x11.start().await
}

/// Stops the X server the app started, closing the windows on it.
#[tauri::command]
pub async fn x11_stop(x11: State<'_, Arc<X11Local>>) -> Result<X11Status, AppError> {
    Ok(x11.stop().await)
}
