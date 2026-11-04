use toolbox_core::ssh::hostkeys::default_known_hosts;
use toolbox_core::ssh::keygen::{self, ExistingKey, NewKey};
use toolbox_core::{AppError, ErrorCode, Secret};

fn ssh_dir() -> Result<std::path::PathBuf, AppError> {
    default_known_hosts()
        .and_then(|file| file.parent().map(|p| p.to_path_buf()))
        .ok_or_else(|| AppError::new(ErrorCode::Internal, "Couldn't find your .ssh folder."))
}

fn task_failed() -> AppError {
    AppError::new(ErrorCode::Internal, "Couldn't reach the key tools.")
}

/// Makes a new ed25519 key in `~/.ssh`, optionally protected by a passphrase. An existing key
/// is never overwritten.
#[tauri::command]
pub async fn key_generate(passphrase: Option<Secret>) -> Result<NewKey, AppError> {
    let dir = ssh_dir()?;
    tokio::task::spawn_blocking(move || keygen::generate_key(&dir, passphrase.as_ref()))
        .await
        .map_err(|_| task_failed())?
}

/// The keys already in `~/.ssh` that can be picked.
#[tauri::command]
pub async fn key_list() -> Result<Vec<ExistingKey>, AppError> {
    let dir = ssh_dir()?;
    tokio::task::spawn_blocking(move || keygen::list_keys(&dir))
        .await
        .map_err(|_| task_failed())
}

/// Setups that are running, so the dialog's Cancel can stop one.
#[derive(Default)]
pub struct KeySetups {
    pub(crate) running:
        std::sync::Mutex<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
}

/// Delivers only the questions (host key, password) to the interface; there is no output.
pub(crate) struct QuestionSink {
    pub(crate) status: tauri::ipc::Channel<toolbox_core::terminal::TerminalStatus>,
}

impl toolbox_core::terminal::OutputSink for QuestionSink {
    fn send_data(&self, _chunk: Vec<u8>) -> Result<(), AppError> {
        Ok(())
    }
    fn send_status(&self, status: toolbox_core::terminal::TerminalStatus) -> Result<(), AppError> {
        self.status
            .send(status)
            .map_err(|_| AppError::new(ErrorCode::Internal, "The window stopped listening."))
    }
}

/// Installs a public key on the server of a saved session and checks that the key login works.
/// Questions arrive on `on_status`; `operation_id` lets `key_setup_cancel` stop it.
#[tauri::command]
pub async fn key_setup(
    setups: tauri::State<'_, KeySetups>,
    services: tauri::State<'_, std::sync::Arc<toolbox_core::ssh::backend::SshServices>>,
    operation_id: String,
    saved_session_id: String,
    private_key_path: String,
    public_line: String,
    on_status: tauri::ipc::Channel<toolbox_core::terminal::TerminalStatus>,
) -> Result<toolbox_core::ssh::install::KeySetupResult, AppError> {
    let cancel = tokio_util::sync::CancellationToken::new();
    setups
        .running
        .lock()
        .unwrap()
        .insert(operation_id.clone(), cancel.clone());
    let result = toolbox_core::ssh::install::setup_key_login(
        &services,
        &saved_session_id,
        std::path::PathBuf::from(private_key_path),
        &public_line,
        std::sync::Arc::new(QuestionSink { status: on_status }),
        cancel,
    )
    .await;
    setups.running.lock().unwrap().remove(&operation_id);
    result
}

#[tauri::command]
pub async fn key_setup_cancel(
    setups: tauri::State<'_, KeySetups>,
    operation_id: String,
) -> Result<(), AppError> {
    if let Some(token) = setups.running.lock().unwrap().get(&operation_id) {
        token.cancel();
    }
    Ok(())
}

fn known_hosts_file() -> Result<std::path::PathBuf, AppError> {
    default_known_hosts()
        .ok_or_else(|| AppError::new(ErrorCode::Internal, "Couldn't find your .ssh folder."))
}

/// The servers whose keys are trusted, from `~/.ssh/known_hosts`.
#[tauri::command]
pub async fn known_hosts_list()
-> Result<Vec<toolbox_core::ssh::known_hosts_list::KnownHostEntry>, AppError> {
    let file = known_hosts_file()?;
    tokio::task::spawn_blocking(move || toolbox_core::ssh::known_hosts_list::list(&file))
        .await
        .map_err(|_| task_failed())?
}

/// Stops trusting one entry. The next connection to that server asks again.
#[tauri::command]
pub async fn known_hosts_remove(line: usize, fingerprint: String) -> Result<(), AppError> {
    let file = known_hosts_file()?;
    tokio::task::spawn_blocking(move || {
        toolbox_core::ssh::known_hosts_list::remove(&file, line, &fingerprint)
    })
    .await
    .map_err(|_| task_failed())?
}
