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
