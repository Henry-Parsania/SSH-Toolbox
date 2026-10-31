//! Opening an SSH connection, with the host key rules applied (INTERFACES.md section 7).

use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
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
/// How long a connection may take, not counting the time the user spends on a question.
const CONNECT_TIMEOUT: Duration = if cfg!(test) {
    Duration::from_millis(600)
} else {
    Duration::from_secs(20)
};

struct TimedOut;

/// Waits for `work`, giving up after `CONNECT_TIMEOUT`. The clock stops while a question is
/// open, so a careful look at a fingerprint can't make the connection fail.
async fn with_deadline<T>(
    work: impl Future<Output = T>,
    asking: &AtomicBool,
) -> Result<T, TimedOut> {
    tokio::pin!(work);
    let tick = Duration::from_millis(100);
    let mut waited = Duration::ZERO;
    loop {
        tokio::select! {
            done = &mut work => return Ok(done),
            _ = tokio::time::sleep(tick) => {
                if !asking.load(Ordering::SeqCst) {
                    waited += tick;
                    if waited >= CONNECT_TIMEOUT {
                        return Err(TimedOut);
                    }
                }
            }
        }
    }
}

/// Why the host key check stopped a connection, kept for the caller because the library only
/// reports "unknown key".
type Failure = Arc<Mutex<Option<AppError>>>;

/// Where connections that come in through a remote forward should go, by the port the server
/// listens on: the host and port on this computer.
pub type RemoteForwards = Arc<Mutex<std::collections::HashMap<u32, (String, u16)>>>;

pub struct ClientHandler {
    asking: Arc<AtomicBool>,
    remote: RemoteForwards,
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

    // A server can open channels towards this computer. Only remote forwards the user asked
    // for are accepted (below). Everything else is refused, so a hostile server can't reach
    // an agent, a display or any other service through the connection.
    async fn server_channel_open_session(
        &mut self,
        _channel: russh::Channel<client::Msg>,
        _reply: russh::client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn server_channel_open_agent_forward(
        &mut self,
        _channel: russh::Channel<client::Msg>,
        _reply: russh::client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn server_channel_open_direct_tcpip(
        &mut self,
        _channel: russh::Channel<client::Msg>,
        _host: &str,
        _port: u32,
        _originator: &str,
        _originator_port: u32,
        _reply: russh::client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn server_channel_open_direct_streamlocal(
        &mut self,
        _channel: russh::Channel<client::Msg>,
        _path: &str,
        _reply: russh::client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn server_channel_open_forwarded_streamlocal(
        &mut self,
        _channel: russh::Channel<client::Msg>,
        _path: &str,
        _reply: russh::client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn server_channel_open_x11(
        &mut self,
        _channel: russh::Channel<client::Msg>,
        _originator: &str,
        _originator_port: u32,
        _reply: russh::client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// A connection arrives through a remote forward: pass it on to the place chosen for that
    /// port. A port nobody asked for is refused.
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<client::Msg>,
        _connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: russh::client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let target = self.remote.lock().unwrap().get(&connected_port).cloned();
        let Some((host, port)) = target else {
            return Ok(()); // Dropping the handle refuses the channel.
        };
        let Ok(mut local) = tokio::net::TcpStream::connect((host.as_str(), port)).await else {
            return Ok(());
        };
        reply.accept().await;
        tokio::spawn(async move {
            let mut through = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut through, &mut local).await;
        });
        Ok(())
    }

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

        self.asking.store(true, Ordering::SeqCst);
        let answer = self.prompter.ask(prompt).await;
        self.asking.store(false, Ordering::SeqCst);
        match answer {
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

/// An open connection. Terminals and file panels each open their own channel on it. It can be
/// used as the library's handle directly.
pub struct Connection {
    handle: Handle<ClientHandler>,
    /// Where remote forwards on this connection lead.
    pub remote: RemoteForwards,
}

impl std::ops::Deref for Connection {
    type Target = Handle<ClientHandler>;
    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

impl std::ops::DerefMut for Connection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.handle
    }
}

fn make_config() -> Arc<client::Config> {
    Arc::new(client::Config {
        keepalive_interval: Some(KEEPALIVE_INTERVAL),
        keepalive_max: KEEPALIVE_MAX,
        ..Default::default()
    })
}

fn make_handler(
    host: &str,
    port: u16,
    known_hosts: PathBuf,
    prompter: Prompter,
) -> (ClientHandler, Failure, RemoteForwards, Arc<AtomicBool>) {
    let failure: Failure = Arc::default();
    let remote = RemoteForwards::default();
    let asking = Arc::new(AtomicBool::new(false));
    let handler = ClientHandler {
        asking: asking.clone(),
        remote: remote.clone(),
        host: host.to_string(),
        port,
        known_hosts,
        prompter,
        failure: failure.clone(),
    };
    (handler, failure, remote, asking)
}

/// Turns what the library said into a plain message, with the host key check's own reason first
/// because the library only says "unknown key".
fn explain<T>(
    attempt: Result<Result<T, russh::Error>, TimedOut>,
    failure: &Failure,
    host: &str,
    port: u16,
) -> Result<T, AppError> {
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

/// Connects and checks the server's key, asking the user when the rules say so.
pub async fn connect(
    host: &str,
    port: u16,
    known_hosts: PathBuf,
    prompter: Prompter,
) -> Result<Connection, AppError> {
    let (handler, failure, remote, asking) = make_handler(host, port, known_hosts, prompter);
    let attempt = with_deadline(
        client::connect(make_config(), (host, port), handler),
        &asking,
    )
    .await;
    explain(attempt, &failure, host, port).map(|handle| Connection { handle, remote })
}

/// Connects to `host` by way of an already open connection (a jump host). The server's key is
/// checked exactly as for a direct connection.
pub async fn connect_through(
    jump: &Connection,
    host: &str,
    port: u16,
    known_hosts: PathBuf,
    prompter: Prompter,
) -> Result<Connection, AppError> {
    let (handler, failure, remote, asking) = make_handler(host, port, known_hosts, prompter);
    let attempt = with_deadline(
        async {
            let channel = jump
                .channel_open_direct_tcpip(host, port.into(), "127.0.0.1", 0)
                .await?;
            client::connect_stream(make_config(), channel.into_stream(), handler).await
        },
        &asking,
    )
    .await;
    explain(attempt, &failure, host, port).map(|handle| Connection { handle, remote })
}
