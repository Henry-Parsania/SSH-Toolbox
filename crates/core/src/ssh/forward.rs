//! Port forwarding over an SSH connection (INTERFACES.md section 7, S7): local, remote and SOCKS.
//!
//! Listeners bind to the loopback address unless the caller says the user was warned that other
//! machines could connect.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

use super::pool::SharedConnection;
use crate::error::{AppError, ErrorCode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ForwardKind {
    /// Connections to a port on this computer go to `dest_host:dest_port` as seen from the server.
    Local { dest_host: String, dest_port: u16 },
    /// Connections to a port on the server come here, to `dest_host:dest_port` on this computer.
    Remote { dest_host: String, dest_port: u16 },
    /// A SOCKS5 proxy on this computer: each connection says where it wants to go.
    Socks,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForwardSpec {
    #[serde(flatten)]
    pub kind: ForwardKind,
    /// The address to listen on: this computer for local and SOCKS, the server for remote.
    pub bind_host: String,
    /// 0 lets the system choose a free port.
    pub bind_port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ForwardInfo {
    pub id: String,
    pub session_id: String,
    pub spec: ForwardSpec,
    /// The port really listening, which differs from `bind_port` when that was 0.
    pub port: u16,
}

/// True if only this computer can reach an address you listen on.
pub fn is_loopback(host: &str) -> bool {
    let host = host.trim();
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.trim_matches(['[', ']'])
        .parse::<IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

fn invalid(message: &str) -> AppError {
    AppError::new(ErrorCode::InvalidInput, message)
}

fn check_spec(spec: &ForwardSpec, confirmed_external: bool) -> Result<(), AppError> {
    let host = spec.bind_host.trim();
    if host.is_empty() || host.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(invalid("The address to listen on isn't valid."));
    }
    if let ForwardKind::Local {
        dest_host,
        dest_port,
    }
    | ForwardKind::Remote {
        dest_host,
        dest_port,
    } = &spec.kind
        && (dest_host.trim().is_empty()
            || dest_host
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
            || *dest_port == 0)
    {
        return Err(invalid("The destination isn't valid."));
    }
    // Anything but loopback lets other computers use the connection (for a remote forward,
    // that depends on the server's settings), so the user must have been warned.
    if !is_loopback(host) && !confirmed_external {
        return Err(invalid(
            "Listening on that address lets other computers on your network use this connection. \
             Confirm that you want this.",
        ));
    }
    Ok(())
}

struct Running {
    info: ForwardInfo,
    cancel: CancellationToken,
}

/// The forwards that are running, across all connections.
#[derive(Default)]
pub struct Forwards {
    running: Arc<Mutex<HashMap<String, Running>>>,
}

impl Forwards {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn list(&self) -> Vec<ForwardInfo> {
        let mut all: Vec<ForwardInfo> = self
            .running
            .lock()
            .unwrap()
            .values()
            .map(|r| r.info.clone())
            .collect();
        all.sort_by(|a, b| a.id.cmp(&b.id));
        all
    }

    pub fn stop(&self, id: &str) {
        if let Some(running) = self.running.lock().unwrap().remove(id) {
            running.cancel.cancel();
        }
    }

    /// Stops every forward of one saved session.
    pub fn stop_session(&self, session_id: &str) {
        let ids: Vec<String> = self
            .running
            .lock()
            .unwrap()
            .values()
            .filter(|r| r.info.session_id == session_id)
            .map(|r| r.info.id.clone())
            .collect();
        for id in ids {
            self.stop(&id);
        }
    }

    /// Starts a forward on `connection`, which stays open for as long as the forward runs. The
    /// forward ends by itself if the connection drops.
    pub async fn start(
        &self,
        connection: Arc<SharedConnection>,
        session_id: &str,
        spec: ForwardSpec,
        confirmed_external: bool,
    ) -> Result<ForwardInfo, AppError> {
        check_spec(&spec, confirmed_external)?;
        let cancel = CancellationToken::new();
        let id = uuid::Uuid::new_v4().to_string();

        let port = match &spec.kind {
            ForwardKind::Local { .. } | ForwardKind::Socks => {
                let listener = TcpListener::bind((spec.bind_host.trim(), spec.bind_port))
                    .await
                    .map_err(|e| bind_failed(&spec, &e))?;
                let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
                tokio::spawn(accept_loop(
                    listener,
                    connection.clone(),
                    spec.kind.clone(),
                    cancel.clone(),
                ));
                port
            }
            ForwardKind::Remote {
                dest_host,
                dest_port,
            } => {
                let assigned = connection
                    .handle
                    .tcpip_forward(spec.bind_host.trim(), spec.bind_port.into())
                    .await
                    .map_err(|_| {
                        AppError::new(
                            ErrorCode::PermissionDenied,
                            "The server refused to listen on that port. It may not allow remote \
                             forwarding, or the port may be in use.",
                        )
                    })?;
                // With port 0 the server says which port it chose.
                let port = if spec.bind_port == 0 {
                    assigned
                } else {
                    spec.bind_port.into()
                };
                connection
                    .handle
                    .remote
                    .lock()
                    .unwrap()
                    .insert(port, (dest_host.clone(), *dest_port));
                port as u16
            }
        };

        let info = ForwardInfo {
            id: id.clone(),
            session_id: session_id.to_string(),
            spec,
            port,
        };
        self.running.lock().unwrap().insert(
            id.clone(),
            Running {
                info: info.clone(),
                cancel: cancel.clone(),
            },
        );

        // Watches the connection: when it drops or the forward is stopped, tidy up.
        let running = self.running.clone();
        let connection_for_cleanup = connection;
        let watched = info.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        if connection_for_cleanup.handle.is_closed() {
                            break;
                        }
                    }
                }
            }
            cancel.cancel();
            if let ForwardKind::Remote { .. } = watched.spec.kind {
                connection_for_cleanup
                    .handle
                    .remote
                    .lock()
                    .unwrap()
                    .remove(&u32::from(watched.port));
                let _ = connection_for_cleanup
                    .handle
                    .cancel_tcpip_forward(watched.spec.bind_host.trim(), watched.port.into())
                    .await;
            }
            running.lock().unwrap().remove(&watched.id);
        });
        Ok(info)
    }
}

fn bind_failed(spec: &ForwardSpec, error: &std::io::Error) -> AppError {
    let why = if error.kind() == std::io::ErrorKind::AddrInUse {
        "that port is already in use"
    } else {
        "it couldn't be opened"
    };
    AppError::new(
        ErrorCode::ConnectionFailed,
        format!(
            "Couldn't listen on {}:{}: {why}.",
            spec.bind_host, spec.bind_port
        ),
    )
}

async fn accept_loop(
    listener: TcpListener,
    connection: Arc<SharedConnection>,
    kind: ForwardKind,
    cancel: CancellationToken,
) {
    loop {
        let accepted = tokio::select! {
            accepted = listener.accept() => accepted,
            _ = cancel.cancelled() => return,
        };
        let Ok((socket, peer)) = accepted else {
            return;
        };
        let connection = connection.clone();
        let kind = kind.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = serve(socket, peer, connection, kind) => {}
                _ = cancel.cancelled() => {}
            }
        });
    }
}

async fn serve(
    mut socket: TcpStream,
    peer: std::net::SocketAddr,
    connection: Arc<SharedConnection>,
    kind: ForwardKind,
) {
    let is_socks = matches!(kind, ForwardKind::Socks);
    let (host, port) = match kind {
        ForwardKind::Local {
            dest_host,
            dest_port,
        } => (dest_host, dest_port),
        ForwardKind::Socks => match socks5_negotiate(&mut socket).await {
            Ok(target) => target,
            Err(_) => return,
        },
        ForwardKind::Remote { .. } => return,
    };
    let channel = connection
        .handle
        .channel_open_direct_tcpip(
            host.clone(),
            port.into(),
            peer.ip().to_string(),
            peer.port().into(),
        )
        .await;
    match channel {
        Ok(channel) => {
            if is_socks {
                let _ = socks5_reply(&mut socket, SocksReply::Succeeded).await;
            }
            let mut through = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut socket, &mut through).await;
        }
        Err(_) => {
            if is_socks {
                let _ = socks5_reply(&mut socket, SocksReply::ConnectionRefused).await;
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum SocksReply {
    Succeeded = 0x00,
    ConnectionRefused = 0x05,
    CommandNotSupported = 0x07,
    AddressTypeNotSupported = 0x08,
}

pub(crate) async fn socks5_reply<W: AsyncWrite + Unpin>(
    out: &mut W,
    reply: SocksReply,
) -> std::io::Result<()> {
    // Version 5, the reply, reserved, an IPv4 address of 0.0.0.0 and port 0.
    out.write_all(&[5, reply as u8, 0, 1, 0, 0, 0, 0, 0, 0])
        .await
}

/// The SOCKS5 greeting and request: no authentication, and only `CONNECT`. Returns where the
/// client wants to go. Anything else is answered with the right refusal and ends the connection.
pub(crate) async fn socks5_negotiate<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
) -> std::io::Result<(String, u16)> {
    let bad = |m: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, m.to_string());

    let mut head = [0u8; 2];
    stream.read_exact(&mut head).await?;
    if head[0] != 5 {
        return Err(bad("not SOCKS5"));
    }
    let mut methods = vec![0u8; head[1] as usize];
    stream.read_exact(&mut methods).await?;
    if !methods.contains(&0) {
        stream.write_all(&[5, 0xFF]).await?; // No acceptable method.
        return Err(bad("no 'no authentication' method offered"));
    }
    stream.write_all(&[5, 0]).await?;

    let mut request = [0u8; 4];
    stream.read_exact(&mut request).await?;
    if request[0] != 5 {
        return Err(bad("not SOCKS5"));
    }
    if request[1] != 1 {
        socks5_reply(stream, SocksReply::CommandNotSupported).await?;
        return Err(bad("only CONNECT is supported"));
    }
    let host = match request[3] {
        1 => {
            let mut ip = [0u8; 4];
            stream.read_exact(&mut ip).await?;
            std::net::Ipv4Addr::from(ip).to_string()
        }
        3 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            let mut name = vec![0u8; len[0] as usize];
            stream.read_exact(&mut name).await?;
            let name = String::from_utf8(name).map_err(|_| bad("name isn't text"))?;
            if name.is_empty() || name.chars().any(|c| c.is_control()) {
                return Err(bad("bad name"));
            }
            name
        }
        4 => {
            let mut ip = [0u8; 16];
            stream.read_exact(&mut ip).await?;
            std::net::Ipv6Addr::from(ip).to_string()
        }
        _ => {
            socks5_reply(stream, SocksReply::AddressTypeNotSupported).await?;
            return Err(bad("unknown address type"));
        }
    };
    let mut port = [0u8; 2];
    stream.read_exact(&mut port).await?;
    Ok((host, u16::from_be_bytes(port)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[test]
    fn only_this_computer_counts_as_loopback() {
        for yes in [
            "127.0.0.1",
            "localhost",
            "LOCALHOST",
            "::1",
            "[::1]",
            "127.1.2.3",
        ] {
            assert!(is_loopback(yes), "{yes}");
        }
        for no in [
            "0.0.0.0",
            "192.168.1.5",
            "::",
            "example.com",
            "",
            "10.0.0.1",
        ] {
            assert!(!is_loopback(no), "{no}");
        }
    }

    fn local(bind: &str) -> ForwardSpec {
        ForwardSpec {
            kind: ForwardKind::Local {
                dest_host: "db.internal".into(),
                dest_port: 5432,
            },
            bind_host: bind.into(),
            bind_port: 0,
        }
    }

    #[test]
    fn a_listener_open_to_the_network_needs_confirmation() {
        assert!(check_spec(&local("127.0.0.1"), false).is_ok());
        let err = check_spec(&local("0.0.0.0"), false).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput);
        assert!(err.message.contains("other computers"), "{}", err.message);
        assert!(check_spec(&local("0.0.0.0"), true).is_ok());
        let socks = ForwardSpec {
            kind: ForwardKind::Socks,
            bind_host: "192.168.0.2".into(),
            bind_port: 1080,
        };
        assert!(check_spec(&socks, false).is_err());
        assert!(check_spec(&socks, true).is_ok());
    }

    #[test]
    fn bad_addresses_and_destinations_are_refused() {
        assert!(check_spec(&local(""), true).is_err());
        assert!(check_spec(&local("a b"), true).is_err());
        let mut spec = local("127.0.0.1");
        spec.kind = ForwardKind::Local {
            dest_host: "".into(),
            dest_port: 80,
        };
        assert!(check_spec(&spec, false).is_err());
        spec.kind = ForwardKind::Local {
            dest_host: "h".into(),
            dest_port: 0,
        };
        assert!(check_spec(&spec, false).is_err());
    }

    #[test]
    fn the_interface_shape_of_a_forward() {
        let spec: ForwardSpec = serde_json::from_str(
            r#"{"type":"local","dest_host":"db","dest_port":5432,"bind_host":"127.0.0.1","bind_port":0}"#,
        )
        .unwrap();
        assert_eq!(spec, local_with("127.0.0.1", "db"));
        let socks: ForwardSpec =
            serde_json::from_str(r#"{"type":"socks","bind_host":"127.0.0.1","bind_port":1080}"#)
                .unwrap();
        assert_eq!(socks.kind, ForwardKind::Socks);
    }

    fn local_with(bind: &str, dest: &str) -> ForwardSpec {
        ForwardSpec {
            kind: ForwardKind::Local {
                dest_host: dest.into(),
                dest_port: 5432,
            },
            bind_host: bind.into(),
            bind_port: 0,
        }
    }

    /// Plays a client against `socks5_negotiate` and returns what it said and what it got back.
    async fn socks(client_bytes: &[u8]) -> (std::io::Result<(String, u16)>, Vec<u8>) {
        let (mut client, mut server) = duplex(1024);
        client.write_all(client_bytes).await.unwrap();
        // The client says nothing more, so a cut-off request ends instead of waiting.
        client.shutdown().await.unwrap();
        let result = socks5_negotiate(&mut server).await;
        drop(server);
        let mut reply = Vec::new();
        client.read_to_end(&mut reply).await.unwrap();
        (result, reply)
    }

    #[tokio::test]
    async fn socks_connect_to_an_ipv4_address() {
        let (result, reply) = socks(&[5, 1, 0, 5, 1, 0, 1, 10, 0, 0, 7, 0x1F, 0x90]).await;
        assert_eq!(result.unwrap(), ("10.0.0.7".to_string(), 8080));
        assert_eq!(reply, [5, 0], "greeting accepted, no authentication");
    }

    #[tokio::test]
    async fn socks_connect_to_a_name_and_to_ipv6() {
        let mut name = vec![5, 1, 0, 5, 1, 0, 3, 11];
        name.extend_from_slice(b"example.com");
        name.extend_from_slice(&[0, 80]);
        assert_eq!(
            socks(&name).await.0.unwrap(),
            ("example.com".to_string(), 80)
        );

        let mut v6 = vec![5, 1, 0, 5, 1, 0, 4];
        v6.extend_from_slice(&[0; 15]);
        v6.push(1);
        v6.extend_from_slice(&[1, 0xBB]);
        assert_eq!(socks(&v6).await.0.unwrap(), ("::1".to_string(), 443));
    }

    #[tokio::test]
    async fn socks_needs_the_no_authentication_method() {
        let (result, reply) = socks(&[5, 1, 2]).await; // Only username/password offered.
        assert!(result.is_err());
        assert_eq!(reply, [5, 0xFF]);
    }

    #[tokio::test]
    async fn socks_only_does_connect() {
        // BIND (2) and UDP ASSOCIATE (3) are refused with "command not supported".
        for command in [2u8, 3] {
            let (result, reply) = socks(&[5, 1, 0, 5, command, 0, 1, 1, 2, 3, 4, 0, 80]).await;
            assert!(result.is_err());
            assert_eq!(reply[..2], [5, 0]);
            assert_eq!(reply[2..4], [5, 7], "command {command}");
        }
    }

    #[tokio::test]
    async fn socks_refuses_other_versions_and_unknown_address_types() {
        assert!(socks(&[4, 1, 0]).await.0.is_err());
        let (result, reply) = socks(&[5, 1, 0, 5, 1, 0, 9]).await;
        assert!(result.is_err());
        assert_eq!(reply[2..4], [5, 8]);
        assert!(
            socks(&[5, 1, 0, 5, 1, 0, 3, 0, 0, 80]).await.0.is_err(),
            "empty name"
        );
    }

    #[tokio::test]
    async fn a_cut_off_request_is_an_error_not_a_hang() {
        assert!(socks(&[5, 1, 0, 5, 1]).await.0.is_err());
    }
}
