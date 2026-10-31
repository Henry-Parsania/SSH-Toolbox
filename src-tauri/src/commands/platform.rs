use toolbox_core::{AppError, ErrorCode, PlatformInfo};

#[tauri::command]
pub async fn platform_info() -> Result<PlatformInfo, AppError> {
    tokio::task::spawn_blocking(toolbox_core::platform::platform_info)
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't read system details."))
}
