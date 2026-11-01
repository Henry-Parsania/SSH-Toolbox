use std::sync::Arc;

use tauri::State;
use tauri::ipc::Channel;
use toolbox_core::AppError;
use toolbox_core::ssh::backend::SshServices;
use toolbox_core::ssh::forward::{ForwardInfo, ForwardSpec, Forwards};
use toolbox_core::terminal::TerminalStatus;

use super::connect::shared_connection;
use super::keys::KeySetups;

/// Starts a forward on a saved session, connecting first if needed. Questions (host key,
/// password) arrive on `on_status`; `operation_id` lets `key_setup_cancel` stop the connecting.
// Tauri passes each argument of a command separately, so this takes many.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn forward_start(
    setups: State<'_, KeySetups>,
    services: State<'_, Arc<SshServices>>,
    forwards: State<'_, Forwards>,
    operation_id: String,
    saved_session_id: String,
    spec: ForwardSpec,
    confirm_external: bool,
    on_status: Channel<TerminalStatus>,
) -> Result<ForwardInfo, AppError> {
    let shared = shared_connection(
        &setups,
        &services,
        &operation_id,
        &saved_session_id,
        on_status,
    )
    .await?;
    forwards
        .start(shared, &saved_session_id, spec, confirm_external)
        .await
}

#[tauri::command]
pub async fn forward_stop(forwards: State<'_, Forwards>, id: String) -> Result<(), AppError> {
    forwards.stop(&id);
    Ok(())
}

/// Stops every forward of a saved session, for when the session is deleted.
#[tauri::command]
pub async fn forward_stop_session(
    forwards: State<'_, Forwards>,
    saved_session_id: String,
) -> Result<(), AppError> {
    forwards.stop_session(&saved_session_id);
    Ok(())
}

#[tauri::command]
pub async fn forward_list(forwards: State<'_, Forwards>) -> Result<Vec<ForwardInfo>, AppError> {
    Ok(forwards.list())
}
