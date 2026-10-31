//! Port forwarding through a server running inside the test, to a small echo server.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

use super::auth::{Login, LoginMethod, login};
use super::connection::connect;
use super::forward::{ForwardInfo, ForwardKind, ForwardSpec, Forwards};
use super::hostkeys::learn;
use super::pool::SharedConnection;
use super::testing::{Accepts, KEY_A, MemoryStore, ServerHandle, key, public_of, spawn_server};
use crate::error::ErrorCode;
use crate::keychain::{Keychain, SecretStore};
use crate::prompt::{PromptBroker, Prompter};
use crate::terminal::testing::RecordingSink;

/// A server that echoes whatever it is sent. Returns its port.
async fn echo_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buffer = [0u8; 1024];
                while let Ok(n) = socket.read(&mut buffer).await {
                    if n == 0 || socket.write_all(&buffer[..n]).await.is_err() {
                        return;
                    }
                }
            });
        }
    });
    port
}

struct Setup {
    forwards: Forwards,
    connection: Arc<SharedConnection>,
    server: ServerHandle,
    _dir: tempfile::TempDir,
}

async fn setup(allow_forwarding: bool) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let host_key = key(KEY_A);
    let server = spawn_server(
        host_key.clone(),
        Accepts {
            user: "tester".into(),
            password: Some("pw".into()),
            allow_forwarding,
            ..Default::default()
        },
    )
    .await;
    let known_hosts = dir.path().join("known_hosts");
    learn(
        "127.0.0.1",
        server.port,
        &public_of(&host_key),
        &known_hosts,
    )
    .unwrap();

    let secrets = Arc::new(MemoryStore::default());
    secrets
        .items
        .lock()
        .unwrap()
        .insert("session:s1:password".into(), "pw".into());
    let dyn_secrets: Arc<dyn SecretStore> = secrets;
    let keychain = Keychain::with_store(Some(dyn_secrets), Duration::from_millis(500));
    let prompter = Prompter::new(
        PromptBroker::new(),
        Arc::new(RecordingSink::default()),
        CancellationToken::new(),
    );
    let mut connection = connect("127.0.0.1", server.port, known_hosts, prompter.clone())
        .await
        .unwrap();
    login(
        &mut connection,
        &Login {
            session_id: "s1".into(),
            host: "127.0.0.1".into(),
            user: "tester".into(),
            method: LoginMethod::Password,
        },
        &keychain,
        &prompter,
    )
    .await
    .unwrap();
    Setup {
        forwards: Forwards::new(),
        connection: SharedConnection::new(connection),
        server,
        _dir: dir,
    }
}

fn local_to(port: u16) -> ForwardSpec {
    ForwardSpec {
        kind: ForwardKind::Local {
            dest_host: "127.0.0.1".into(),
            dest_port: port,
        },
        bind_host: "127.0.0.1".into(),
        bind_port: 0,
    }
}

async fn start(s: &Setup, spec: ForwardSpec) -> ForwardInfo {
    s.forwards
        .start(s.connection.clone(), "s1", spec, false)
        .await
        .unwrap()
}

/// Sends `text` to `127.0.0.1:port` and returns what comes back (as much as was sent).
async fn through(port: u16, text: &str) -> String {
    let mut socket = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    socket.write_all(text.as_bytes()).await.unwrap();
    let mut back = vec![0u8; text.len()];
    tokio::time::timeout(Duration::from_secs(5), socket.read_exact(&mut back))
        .await
        .expect("timed out")
        .unwrap();
    String::from_utf8(back).unwrap()
}

async fn until(what: &str, mut ok: impl FnMut() -> bool) {
    for _ in 0..400 {
        if ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for: {what}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_local_forward_carries_traffic_through_the_server() {
    let s = setup(true).await;
    let echo = echo_server().await;
    let info = start(&s, local_to(echo)).await;
    assert_ne!(
        info.port, 0,
        "the real port is reported when 0 was asked for"
    );
    assert_eq!(
        through(info.port, "hello through the tunnel").await,
        "hello through the tunnel"
    );
    assert_eq!(s.server.stats.forwarded.load(Ordering::SeqCst), 1);
    // Several connections at once each get their own channel.
    let (a, b) = tokio::join!(through(info.port, "first"), through(info.port, "second!"));
    assert_eq!((a.as_str(), b.as_str()), ("first", "second!"));
    assert_eq!(s.forwards.list().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_socks_proxy_connects_to_whatever_the_client_asks_for() {
    let s = setup(true).await;
    let echo = echo_server().await;
    let info = start(
        &s,
        ForwardSpec {
            kind: ForwardKind::Socks,
            bind_host: "127.0.0.1".into(),
            bind_port: 0,
        },
    )
    .await;

    let mut socket = TcpStream::connect(("127.0.0.1", info.port)).await.unwrap();
    socket.write_all(&[5, 1, 0]).await.unwrap();
    let mut greeting = [0u8; 2];
    socket.read_exact(&mut greeting).await.unwrap();
    assert_eq!(greeting, [5, 0]);

    let mut request = vec![5, 1, 0, 1, 127, 0, 0, 1];
    request.extend_from_slice(&echo.to_be_bytes());
    socket.write_all(&request).await.unwrap();
    let mut reply = [0u8; 10];
    socket.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply[..2], [5, 0], "succeeded");

    socket.write_all(b"over socks").await.unwrap();
    let mut back = [0u8; 10];
    socket.read_exact(&mut back).await.unwrap();
    assert_eq!(&back, b"over socks");
    assert_eq!(s.server.stats.forwarded.load(Ordering::SeqCst), 1);

    // A host name works too, and is resolved on the server's side.
    let mut named = TcpStream::connect(("127.0.0.1", info.port)).await.unwrap();
    named.write_all(&[5, 1, 0]).await.unwrap();
    named.read_exact(&mut greeting).await.unwrap();
    let mut request = vec![5, 1, 0, 3, 9];
    request.extend_from_slice(b"localhost");
    request.extend_from_slice(&echo.to_be_bytes());
    named.write_all(&request).await.unwrap();
    named.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply[..2], [5, 0]);
}

#[tokio::test(flavor = "multi_thread")]
async fn socks_says_connection_refused_when_the_far_side_is_not_there() {
    let s = setup(true).await;
    let closed = {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap().port()
    };
    let info = start(
        &s,
        ForwardSpec {
            kind: ForwardKind::Socks,
            bind_host: "127.0.0.1".into(),
            bind_port: 0,
        },
    )
    .await;
    let mut socket = TcpStream::connect(("127.0.0.1", info.port)).await.unwrap();
    socket.write_all(&[5, 1, 0]).await.unwrap();
    let mut greeting = [0u8; 2];
    socket.read_exact(&mut greeting).await.unwrap();
    let mut request = vec![5, 1, 0, 1, 127, 0, 0, 1];
    request.extend_from_slice(&closed.to_be_bytes());
    socket.write_all(&request).await.unwrap();
    let mut reply = [0u8; 10];
    socket.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply[..2], [5, 5], "connection refused");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_remote_forward_brings_the_servers_port_back_to_this_computer() {
    let s = setup(true).await;
    let echo = echo_server().await;
    let info = start(
        &s,
        ForwardSpec {
            kind: ForwardKind::Remote {
                dest_host: "127.0.0.1".into(),
                dest_port: echo,
            },
            bind_host: "127.0.0.1".into(),
            bind_port: 0,
        },
    )
    .await;
    assert_ne!(info.port, 0, "the port the server chose is reported");
    assert_eq!(
        through(info.port, "from the server side").await,
        "from the server side"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_refuses_remote_forwarding_gives_a_clear_error() {
    let s = setup(false).await;
    let err = s
        .forwards
        .start(
            s.connection.clone(),
            "s1",
            ForwardSpec {
                kind: ForwardKind::Remote {
                    dest_host: "127.0.0.1".into(),
                    dest_port: 80,
                },
                bind_host: "127.0.0.1".into(),
                bind_port: 0,
            },
            false,
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::PermissionDenied);
    assert!(s.forwards.list().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn listening_beyond_this_computer_needs_the_warning_to_have_been_confirmed() {
    let s = setup(true).await;
    let echo = echo_server().await;
    let mut open = local_to(echo);
    open.bind_host = "0.0.0.0".into();

    let err = s
        .forwards
        .start(s.connection.clone(), "s1", open.clone(), false)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(s.forwards.list().is_empty(), "nothing was started");

    let info = s
        .forwards
        .start(s.connection.clone(), "s1", open, true)
        .await
        .unwrap();
    assert_eq!(through(info.port, "wide open").await, "wide open");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_port_already_in_use_is_explained() {
    let s = setup(true).await;
    let taken = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut spec = local_to(80);
    spec.bind_port = taken.local_addr().unwrap().port();
    let err = s
        .forwards
        .start(s.connection.clone(), "s1", spec, false)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ConnectionFailed);
    assert!(err.message.contains("already in use"), "{}", err.message);
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_a_forward_closes_its_port() {
    let s = setup(true).await;
    let echo = echo_server().await;
    let info = start(&s, local_to(echo)).await;
    assert_eq!(through(info.port, "x").await, "x");

    s.forwards.stop(&info.id);
    assert!(s.forwards.list().is_empty());
    let mut refused = false;
    for _ in 0..200 {
        if TcpStream::connect(("127.0.0.1", info.port)).await.is_err() {
            refused = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(refused, "the port is still open after stopping");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_forward_ends_by_itself_when_the_connection_drops() {
    let s = setup(true).await;
    let echo = echo_server().await;
    let info = start(&s, local_to(echo)).await;
    assert_eq!(s.forwards.list().len(), 1);
    let _ = info;

    s.server.kill();
    until("the forward is removed", || s.forwards.list().is_empty()).await;
}
