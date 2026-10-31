use toolbox_core::link::checked_link;
use toolbox_core::{AppError, ErrorCode};

/// Opens a web link in the default browser. The address is checked here, in the backend: the
/// interface has no permission to use the opener plugin itself.
#[tauri::command]
pub async fn open_link(url: String) -> Result<(), AppError> {
    let url = checked_link(&url)?;
    tokio::task::spawn_blocking(move || tauri_plugin_opener::open_url(url, None::<&str>))
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't open the link."))?
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't open the link."))
}
