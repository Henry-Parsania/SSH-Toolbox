use std::sync::Arc;

use tauri::State;
use toolbox_core::keychain::{Keychain, SecretKind};
use toolbox_core::{AppError, ErrorCode, Secret};

/// Whether saved passwords can be used. False hides "save password" in the session editor.
#[tauri::command]
pub async fn keychain_available(keychain: State<'_, Arc<Keychain>>) -> Result<bool, AppError> {
    Ok(keychain.is_available())
}

fn kind_of(kind: &str) -> Result<SecretKind, AppError> {
    match kind {
        "password" => Ok(SecretKind::Password),
        "passphrase" => Ok(SecretKind::Passphrase),
        "ftp_password" => Ok(SecretKind::FtpPassword),
        _ => Err(AppError::new(
            ErrorCode::InvalidInput,
            "That isn't a kind of saved secret.",
        )),
    }
}

/// Saves a secret typed by the user. There is deliberately no command that reads one back to the
/// interface: only the backend reads secrets, when it connects.
#[tauri::command]
pub async fn keychain_set(
    keychain: State<'_, Arc<Keychain>>,
    session_id: String,
    kind: String,
    secret: Secret,
) -> Result<(), AppError> {
    keychain.set(&session_id, kind_of(&kind)?, secret).await
}

#[tauri::command]
pub async fn keychain_delete_session(
    keychain: State<'_, Arc<Keychain>>,
    session_id: String,
) -> Result<(), AppError> {
    keychain.delete_all(&session_id).await
}

/// Removes one saved secret, for when the user chooses to delete a password that a key now
/// replaces. Never done without asking.
#[tauri::command]
pub async fn keychain_delete_secret(
    keychain: State<'_, Arc<Keychain>>,
    session_id: String,
    kind: String,
) -> Result<(), AppError> {
    keychain.delete(&session_id, kind_of(&kind)?).await
}
