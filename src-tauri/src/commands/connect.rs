use std::sync::Arc;

use tauri::State;
use tauri::ipc::Channel;
use toolbox_core::prompt::Prompter;
use toolbox_core::sessions::SessionStore;
use toolbox_core::ssh::backend::SshServices;
use toolbox_core::ssh::chain::{connect_chain, resolve_chain};
use toolbox_core::ssh::pool::SharedConnection;
use toolbox_core::terminal::TerminalStatus;
use toolbox_core::{AppError, ErrorCode};

use super::keys::{KeySetups, QuestionSink};

/// The shared connection for a saved session, connecting first (and asking the user what is
/// needed) if there isn't one. `operation_id` lets `key_setup_cancel` stop the connecting.
pub(crate) async fn shared_connection(
    setups: &State<'_, KeySetups>,
    services: &State<'_, Arc<SshServices>>,
    operation_id: &str,
    saved_session_id: &str,
    on_status: Channel<TerminalStatus>,
) -> Result<Arc<SharedConnection>, AppError> {
    let cancel = tokio_util::sync::CancellationToken::new();
    setups
        .running
        .lock()
        .unwrap()
        .insert(operation_id.to_string(), cancel.clone());
    let result = async {
        let path = services.sessions_file.clone();
        let loaded = tokio::task::spawn_blocking(move || SessionStore::new(path).load())
            .await
            .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't read the saved sessions."))?;
        if let Some(problem) = loaded.problem {
            return Err(AppError::new(ErrorCode::InvalidInput, problem));
        }
        let chain = resolve_chain(&loaded.file, saved_session_id)?;
        let prompter = Prompter::new(
            services.broker.clone(),
            Arc::new(QuestionSink { status: on_status }),
            cancel.clone(),
        );
        let connecting = services.pool.get_or_connect(
            saved_session_id,
            connect_chain(services, &chain, None, &prompter),
        );
        tokio::select! {
            shared = connecting => shared,
            _ = cancel.cancelled() => Err(AppError::new(ErrorCode::Cancelled, "Cancelled.")),
        }
    }
    .await;
    setups.running.lock().unwrap().remove(operation_id);
    result
}
