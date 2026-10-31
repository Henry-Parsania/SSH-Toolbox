use std::sync::Arc;

use tauri::State;
use tauri::ipc::Channel;
use toolbox_core::AppError;
use toolbox_core::ErrorCode;
use toolbox_core::prompt::Prompter;
use toolbox_core::sessions::SessionStore;
use toolbox_core::ssh::backend::SshServices;
use toolbox_core::ssh::chain::{connect_chain, resolve_chain};
use toolbox_core::ssh::forward::{ForwardInfo, ForwardSpec, Forwards};
use toolbox_core::terminal::TerminalStatus;

use super::keys::{KeySetups, QuestionSink};

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
    let cancel = tokio_util::sync::CancellationToken::new();
    setups
        .running
        .lock()
        .unwrap()
        .insert(operation_id.clone(), cancel.clone());
    let result = async {
        let path = services.sessions_file.clone();
        let loaded = tokio::task::spawn_blocking(move || SessionStore::new(path).load())
            .await
            .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't read the saved sessions."))?;
        if let Some(problem) = loaded.problem {
            return Err(AppError::new(ErrorCode::InvalidInput, problem));
        }
        let chain = resolve_chain(&loaded.file, &saved_session_id)?;
        let prompter = Prompter::new(
            services.broker.clone(),
            Arc::new(QuestionSink { status: on_status }),
            cancel.clone(),
        );
        let connecting = services.pool.get_or_connect(
            &saved_session_id,
            connect_chain(&services, &chain, None, &prompter),
        );
        let shared = tokio::select! {
            shared = connecting => shared?,
            _ = cancel.cancelled() => {
                return Err(AppError::new(ErrorCode::Cancelled, "Cancelled."));
            }
        };
        forwards
            .start(shared, &saved_session_id, spec, confirm_external)
            .await
    }
    .await;
    setups.running.lock().unwrap().remove(&operation_id);
    result
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
