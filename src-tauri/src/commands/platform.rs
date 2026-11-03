use toolbox_core::{AppError, ErrorCode, PlatformInfo};

#[tauri::command]
pub async fn platform_info() -> Result<PlatformInfo, AppError> {
    tokio::task::spawn_blocking(toolbox_core::platform::platform_info)
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't read system details."))
}

/// The version and build, such as `0.1.0 (a1b2c3d built 2026-11-03 16:40 UTC)`.
pub fn version_line() -> String {
    format!("{} ({})", env!("CARGO_PKG_VERSION"), env!("TOOLBOX_BUILD"))
}

#[tauri::command]
pub fn app_version() -> String {
    version_line()
}
