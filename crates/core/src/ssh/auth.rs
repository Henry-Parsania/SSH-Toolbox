//! Logging in over an open connection (INTERFACES.md section 7, "Login order").

use std::path::{Path, PathBuf};
use std::sync::Arc;

use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::{AgentClient, AgentStream};
use russh::keys::{PrivateKeyWithHashAlg, load_secret_key};
use tokio::io::{AsyncRead, AsyncWrite};

use super::connection::Connection;
use crate::error::{AppError, ErrorCode};
use crate::keychain::{Keychain, SecretKind};
use crate::prompt::{Prompt, Prompter};
use crate::secret::Secret;

/// How a saved session logs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginMethod {
    Password,
    /// A private key file. It is read each time and never copied (SEC5).
    Key {
        path: String,
    },
    Agent,
}

#[derive(Debug, Clone)]
pub struct Login {
    /// The saved session, which names its entries in the keychain.
    pub session_id: String,
    pub host: String,
    pub user: String,
    pub method: LoginMethod,
}

fn auth_failed(message: &str) -> AppError {
    AppError::new(ErrorCode::AuthFailed, message)
}

/// Logs in with the method saved on the session. A refused login is `AuthFailed`.
pub async fn login(
    connection: &mut Connection,
    login: &Login,
    keychain: &Keychain,
    prompter: &Prompter,
) -> Result<(), AppError> {
    match &login.method {
        LoginMethod::Password => password_login(connection, login, keychain, prompter).await,
        LoginMethod::Key { path } => key_login(connection, login, path, keychain, prompter).await,
        LoginMethod::Agent => agent_login(connection, &login.user).await,
    }
}

async fn try_password(
    connection: &mut Connection,
    user: &str,
    password: &Secret,
) -> Result<bool, AppError> {
    connection
        .authenticate_password(user, password.expose())
        .await
        .map(|r| r.success())
        .map_err(|_| {
            AppError::new(
                ErrorCode::ConnectionLost,
                "The connection closed while logging in.",
            )
        })
}

async fn password_login(
    connection: &mut Connection,
    login: &Login,
    keychain: &Keychain,
    prompter: &Prompter,
) -> Result<(), AppError> {
    // A saved password goes first. If the keychain can't be used, or the password no longer
    // works, the user is asked.
    if let Ok(Some(saved)) = keychain.get(&login.session_id, SecretKind::Password).await
        && try_password(connection, &login.user, &saved).await?
    {
        return Ok(());
    }

    let answer = prompter
        .ask(Prompt::Password {
            user: login.user.clone(),
            host: login.host.clone(),
            offer_save: keychain.is_available(),
        })
        .await?;
    let Some(typed) = answer.text.filter(|_| answer.accepted) else {
        return Err(AppError::new(
            ErrorCode::Cancelled,
            "The login was cancelled.",
        ));
    };
    if !try_password(connection, &login.user, &typed).await? {
        return Err(auth_failed(
            "The server refused that user name or password.",
        ));
    }
    if answer.save {
        // The keychain being unavailable doesn't undo a login that worked.
        let _ = keychain
            .set(&login.session_id, SecretKind::Password, typed)
            .await;
    }
    Ok(())
}

/// `~` at the start of a path means the user's home folder.
pub fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\"))
        && let Some(dirs) = directories::UserDirs::new()
    {
        return dirs.home_dir().join(rest);
    }
    PathBuf::from(path)
}

fn read_key(
    path: &Path,
    passphrase: Option<&Secret>,
) -> Result<russh::keys::PrivateKey, russh::keys::Error> {
    load_secret_key(path, passphrase.map(|p| p.expose()))
}

async fn key_login(
    connection: &mut Connection,
    login: &Login,
    key_path: &str,
    keychain: &Keychain,
    prompter: &Prompter,
) -> Result<(), AppError> {
    let path = expand_home(key_path);
    if !path.is_file() {
        return Err(AppError::new(
            ErrorCode::NotFound,
            format!("The key file {} wasn't found.", path.display()),
        ));
    }

    let key = match read_key(&path, None) {
        Ok(key) => key,
        Err(russh::keys::Error::KeyIsEncrypted) => {
            let mut offer_save = keychain.is_available();
            let mut key = None;
            // The saved passphrase first, then one question.
            if let Ok(Some(saved)) = keychain
                .get(&login.session_id, SecretKind::Passphrase)
                .await
            {
                key = read_key(&path, Some(&saved)).ok();
                offer_save = offer_save && key.is_none();
            }
            match key {
                Some(key) => key,
                None => {
                    let answer = prompter
                        .ask(Prompt::Passphrase {
                            key_path: path.display().to_string(),
                            offer_save,
                        })
                        .await?;
                    let Some(typed) = answer.text.filter(|_| answer.accepted) else {
                        return Err(AppError::new(
                            ErrorCode::Cancelled,
                            "The login was cancelled.",
                        ));
                    };
                    let key = read_key(&path, Some(&typed))
                        .map_err(|_| auth_failed("That passphrase doesn't open the key."))?;
                    if answer.save {
                        let _ = keychain
                            .set(&login.session_id, SecretKind::Passphrase, typed)
                            .await;
                    }
                    key
                }
            }
        }
        Err(_) => {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("{} isn't a private key the app can read.", path.display()),
            ));
        }
    };

    let hash = connection
        .best_supported_rsa_hash()
        .await
        .ok()
        .flatten()
        .flatten();
    let result = connection
        .authenticate_publickey(&login.user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
        .await
        .map_err(|_| {
            AppError::new(
                ErrorCode::ConnectionLost,
                "The connection closed while logging in.",
            )
        })?;
    if result.success() {
        Ok(())
    } else {
        Err(auth_failed("The server didn't accept that key."))
    }
}

/// Tries each key the agent holds. Returns whether one was accepted.
async fn try_agent<S>(
    connection: &mut Connection,
    user: &str,
    mut agent: AgentClient<S>,
) -> Result<bool, AppError>
where
    S: AgentStream + AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let lost = || {
        AppError::new(
            ErrorCode::ConnectionLost,
            "The connection closed while logging in.",
        )
    };
    // The agent's own wording is a plain library error: no key or secret is in it.
    let identities = agent.request_identities().await.map_err(|e| {
        AppError::new(
            ErrorCode::ConnectionFailed,
            format!("The SSH agent didn't answer ({e})."),
        )
    })?;
    if identities.is_empty() {
        return Err(auth_failed(
            "The SSH agent has no keys. Add one with ssh-add.",
        ));
    }
    let hash = connection
        .best_supported_rsa_hash()
        .await
        .ok()
        .flatten()
        .flatten();
    for identity in identities {
        let AgentIdentity::PublicKey { key, .. } = identity else {
            continue; // Certificates in the agent aren't used.
        };
        let result = connection
            .authenticate_publickey_with(user, key, hash, &mut agent)
            .await
            .map_err(|_| lost())?;
        if result.success() {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(unix)]
async fn agent_login(connection: &mut Connection, user: &str) -> Result<(), AppError> {
    if std::env::var_os("SSH_AUTH_SOCK").is_none() {
        return Err(AppError::new(
            ErrorCode::ConnectionFailed,
            "No SSH agent is running: SSH_AUTH_SOCK isn't set. Start one, or choose another login method.",
        ));
    }
    let agent = AgentClient::connect_env().await.map_err(|_| {
        AppError::new(
            ErrorCode::ConnectionFailed,
            "Couldn't reach the SSH agent named by SSH_AUTH_SOCK.",
        )
    })?;
    if try_agent(connection, user, agent).await? {
        Ok(())
    } else {
        Err(auth_failed(
            "The server didn't accept any key from the SSH agent.",
        ))
    }
}

#[cfg(windows)]
async fn agent_login(connection: &mut Connection, user: &str) -> Result<(), AppError> {
    let mut accepted = false;
    let mut reached = false;
    // The Windows agent sometimes drops the first request on a fresh pipe, so one more try
    // is made on a new connection before giving up.
    for attempt in 0..2 {
        let Ok(agent) = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await else {
            break;
        };
        reached = true;
        match try_agent(connection, user, agent).await {
            Ok(done) => {
                accepted = done;
                break;
            }
            Err(e) if attempt == 0 && e.message.starts_with("The SSH agent didn't answer") => {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            Err(e) => return Err(e),
        }
    }
    if !accepted && let Ok(agent) = AgentClient::connect_pageant().await {
        reached = true;
        accepted = try_agent(connection, user, agent).await?;
    }
    if accepted {
        Ok(())
    } else if reached {
        Err(auth_failed(
            "The server didn't accept any key from the SSH agent.",
        ))
    } else {
        Err(AppError::new(
            ErrorCode::ConnectionFailed,
            "No SSH agent was found. Neither the Windows OpenSSH agent (start the ssh-agent service) nor Pageant is running.",
        ))
    }
}

#[cfg(not(any(unix, windows)))]
async fn agent_login(_connection: &mut Connection, _user: &str) -> Result<(), AppError> {
    Err(AppError::new(
        ErrorCode::Unsupported,
        "SSH agents aren't supported on this system.",
    ))
}
