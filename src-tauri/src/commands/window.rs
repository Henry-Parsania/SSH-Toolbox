//! Extra windows: a tab moved to its own window, and a pop-up with a server's process monitor.

use tauri::{AppHandle, WebviewUrl, WebviewWindowBuilder};
use toolbox_core::{AppError, ErrorCode};

/// Largest description of a window's contents that is accepted.
const MAX_PAYLOAD: usize = 200_000;
const MAX_TITLE: usize = 100;

/// Opens a window that shows what `payload` describes. The payload is the interface's own
/// description of the tab (URL-safe base64), which the new window reads from its address.
#[tauri::command]
pub async fn window_open(app: AppHandle, title: String, payload: String) -> Result<(), AppError> {
    let valid = !payload.is_empty()
        && payload.len() <= MAX_PAYLOAD
        && payload
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !valid {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "That window's contents aren't valid.",
        ));
    }
    let title: String = title
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_TITLE)
        .collect();
    // A new label for every window. Only labels that start with `win-` are allowed to use the
    // app's commands (see the capabilities file).
    let label = format!("win-{}", uuid::Uuid::new_v4().simple());
    let url = WebviewUrl::App(format!("index.html?window={payload}").into());
    WebviewWindowBuilder::new(&app, label, url)
        .title(if title.is_empty() {
            "SSH Toolbox"
        } else {
            &title
        })
        .inner_size(1000.0, 650.0)
        .min_inner_size(400.0, 300.0)
        .build()
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't open a new window."))?;
    Ok(())
}
