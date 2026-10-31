//! Opening an SSH connection, with the host key rules applied (INTERFACES.md section 7).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::PublicKeyOrCertificate;

use super::hostkeys::{HostKeyDecision, decide, learn};
use crate::error::{AppError, ErrorCode};
use crate::prompt::{Prompt, Prompter};

/// How often to check the connection is alive, and how many misses mean it is gone (S5).
pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
pub const KEEPALIVE_MAX: usize = 3;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Why the host key check stopped a connection, kept for the caller because the library only
/// reports "unknown key".
type Failure = Arc<Mutex<Option<AppError>>>;

pub struct ClientHandler {
    host: String,
    port: u16,
    known_hosts: PathBuf,
    prompter: Prompter,
    failure: Failure,
}

fn refuse(failure: &Failure, error: AppError) -> Result<bool, russh::Error> {
    *failure.lock().unwrap() = Some(error);
    Ok(false)
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            return refuse(
                &self.failure,
                AppError::new(
                    ErrorCode::ConnectionFailed,
                    "This server uses a host certificate, which isn't supported yet.",
                ),
            );
        };

        let decision = match decide(&self.host, self.port, key, &self.known_hosts) {
            Ok(decision) => decision,
            Err(error) => return refuse(&self.failure, error),
        };

        let prompt = match decision {
            HostKeyDecision::Trusted => return Ok(true),
            HostKeyDecision::Unknown {
                algorithm,
                fingerprint,
            } => Prompt::HostKeyUnknown {
                host: self.host.clone(),
                port: self.port,
                algorithm,
                fingerprint,
            },
            HostKeyDecision::NewType {
                algorithm,
                fingerprint,
                known_algorithms,
            } => Prompt::HostKeyNewType {
                host: self.host.clone(),
                port: self.port,
                algorithm,
                fingerprint,
                known_algorithms,
            },
        };

        match self.prompter.ask(prompt).await {
            Ok(answer) if answer.accepted => {
                if let Err(error) = learn(&self.host, self.port, key, &self.known_hosts) {
                    return refuse(&self.failure, error);
                }
                Ok(true)
            }
            Ok(_) => refuse(
                &self.failure,
                AppError::new(
                    ErrorCode::HostKeyRejected,
                    "The server's key wasn't accepted, so the connection was closed.",
                ),
            ),
            Err(error) => refuse(&self.failure, error),
        }
    }
}

/// An open connection. Terminals and file panels each open their own channel on it.
pub type Connection = Handle<ClientHandler>;

/// Connects and checks the server's key, asking the user when the rules say so.
pub async fn connect(
    host: &str,
    port: u16,
    known_hosts: PathBuf,
    prompter: Prompter,
) -> Result<Connection, AppError> {
    let failure: Failure = Arc::default();
    let handler = ClientHandler {
        host: host.to_string(),
        port,
        known_hosts,
        prompter,
        failure: failure.clone(),
    };
    let config = Arc::new(client::Config {
        keepalive_interval: Some(KEEPALIVE_INTERVAL),
        keepalive_max: KEEPALIVE_MAX,
        ..Default::default()
    });

    let attempt = tokio::time::timeout(
        CONNECT_TIMEOUT,
        client::connect(config, (host, port), handler),
    )
    .await;

    // The host key check's own reason comes first: the library only says "unknown key".
    if let Some(error) = failure.lock().unwrap().take() {
        return Err(error);
    }
    match attempt {
        Ok(Ok(connection)) => Ok(connection),
        Ok(Err(_)) => Err(AppError::new(
            ErrorCode::ConnectionFailed,
            format!("Couldn't connect to {host} on port {port}."),
        )),
        Err(_) => Err(AppError::new(
            ErrorCode::ConnectionFailed,
            format!("{host} didn't answer in time."),
        )),
    }
}
