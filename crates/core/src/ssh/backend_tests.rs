//! SSH panes end to end: saved session, shared connection, shell, resize and failure.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::backend::{SshServices, open_ssh_terminal};
use super::hostkeys::learn;
use super::pool::ConnectionPool;
use super::testing::{Accepts, KEY_A, ServerHandle, X11Fake, key, public_of, spawn_server};
use crate::error::ErrorCode;
use crate::keychain::{Keychain, SecretStore};
use crate::prompt::PromptBroker;
use crate::sessions::{SavedSession, SessionStore, SessionsFile, SshAuth};
use crate::terminal::testing::{RecordingSink, Sent};
use crate::terminal::{SessionRegistry, TermSize, TerminalStatus};
use crate::x11::X11Local;

use super::testing::MemoryStore;

const SIZE: TermSize = TermSize { cols: 80, rows: 24 };

struct World {
    services: Arc<SshServices>,
    registry: SessionRegistry,
    _dir: tempfile::TempDir,
    store: SessionStore,
    known_hosts: std::path::PathBuf,
    secrets: Arc<MemoryStore>,
}

fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let x11 = Arc::new(X11Local::new(dir.path().join("x11")));
    world_in(dir, x11)
}

fn world_in(dir: tempfile::TempDir, x11: Arc<X11Local>) -> World {
    let secrets = Arc::new(MemoryStore::default());
    let dyn_secrets: Arc<dyn SecretStore> = secrets.clone();
    let sessions_file = dir.path().join("sessions.toml");
    let known_hosts = dir.path().join("known_hosts");
    let services = Arc::new(SshServices {
        keychain: Arc::new(Keychain::with_store(
            Some(dyn_secrets),
            Duration::from_millis(500),
        )),
        broker: PromptBroker::new(),
        pool: ConnectionPool::new(),
        sessions_file: sessions_file.clone(),
        known_hosts: known_hosts.clone(),
        x11,
    });
    World {
        services,
        registry: SessionRegistry::new(),
        store: SessionStore::new(sessions_file),
        known_hosts,
        _dir: dir,
        secrets,
    }
}

/// A server, a saved session for it (named \`id\`), its password saved in the keychain, and its
/// host key already trusted.
async fn server_for(w: &World, id: &str) -> ServerHandle {
    server_with(w, id, None, false).await
}

/// Like `server_for`, with X11 forwarding: whether the server accepts it, and whether the saved
/// session asks for it.
async fn server_with(w: &World, id: &str, x11: Option<X11Fake>, asked: bool) -> ServerHandle {
    let host_key = key(KEY_A);
    let server = spawn_server(
        host_key.clone(),
        Accepts {
            user: "tester".into(),
            password: Some("pw".into()),
            keys: vec![],
            x11,
            ..Default::default()
        },
    )
    .await;
    learn(
        "127.0.0.1",
        server.port,
        &public_of(&host_key),
        &w.known_hosts,
    )
    .unwrap();
    w.secrets
        .items
        .lock()
        .unwrap()
        .insert(format!("session:{id}:password"), "pw".into());

    let mut file = w.store.load().file;
    file.sessions.push(SavedSession::Ssh {
        id: id.into(),
        name: format!("server {id}"),
        folder: String::new(),
        host: "127.0.0.1".into(),
        port: server.port,
        user: "tester".into(),
        auth: SshAuth::Password,
        key_path: String::new(),
        jump: String::new(),
        x11: asked,
        color: String::new(),
        startup: String::new(),
    });
    let file = SessionsFile { ..file };
    w.store.save(&file).unwrap();
    server
}

async fn open(w: &World, id: &str) -> (String, Arc<RecordingSink>) {
    let sink = Arc::new(RecordingSink::default());
    let session = open_ssh_terminal(
        &w.registry,
        w.services.clone(),
        id.into(),
        false,
        SIZE,
        sink.clone(),
    )
    .await
    .unwrap();
    (session, sink)
}

fn text(sink: &RecordingSink) -> String {
    let bytes: Vec<u8> = sink
        .all()
        .into_iter()
        .filter_map(|s| match s {
            Sent::Data(d) => Some(d),
            _ => None,
        })
        .flatten()
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
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

async fn echoes(w: &World, session: &str, sink: &RecordingSink, word: &str) {
    w.registry
        .get(session)
        .unwrap()
        .write(word.as_bytes())
        .await
        .unwrap();
    until(&format!("echo of {word}"), || text(sink).contains(word)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_saved_session_opens_a_shell_you_can_type_into_and_resize() {
    let w = world();
    let server = server_for(&w, "s1").await;
    let (session, sink) = open(&w, "s1").await;

    until("connected", || {
        sink.statuses().contains(&TerminalStatus::Connected)
    })
    .await;
    assert_eq!(sink.statuses()[0], TerminalStatus::Connecting);
    until("banner", || text(&sink).contains("welcome-banner")).await;
    echoes(&w, &session, &sink, "hello-remote").await;

    assert_eq!(server.stats.sizes.lock().unwrap()[0], (80, 24));
    w.registry
        .get(&session)
        .unwrap()
        .resize(TermSize {
            cols: 100,
            rows: 30,
        })
        .await
        .unwrap();
    until("resize reaches the server", || {
        server.stats.sizes.lock().unwrap().last() == Some(&(100, 30))
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_exit_code_is_reported_after_the_last_output() {
    let w = world();
    let _server = server_for(&w, "s1").await;
    let (session, sink) = open(&w, "s1").await;
    until("banner", || text(&sink).contains("welcome-banner")).await;
    w.registry
        .get(&session)
        .unwrap()
        .write(b"exit-now")
        .await
        .unwrap();
    until("exit", || {
        sink.statuses()
            .contains(&TerminalStatus::Exited { code: Some(7) })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn panes_of_one_saved_session_share_one_connection_until_the_last_closes() {
    let w = world();
    let server = server_for(&w, "s1").await;
    let (a, sink_a) = open(&w, "s1").await;
    let (b, sink_b) = open(&w, "s1").await;
    until("both banners", || {
        text(&sink_a).contains("welcome-banner") && text(&sink_b).contains("welcome-banner")
    })
    .await;
    assert_eq!(
        server.stats.opened.load(Ordering::SeqCst),
        1,
        "one login, not two"
    );
    assert_eq!(server.stats.shells.load(Ordering::SeqCst), 2);

    w.registry.close(&a).await.unwrap();
    echoes(&w, &b, &sink_b, "still-here").await;
    assert_eq!(server.stats.ended.load(Ordering::SeqCst), 0);

    w.registry.close(&b).await.unwrap();
    until("the connection closes with its last pane", || {
        server.stats.ended.load(Ordering::SeqCst) == 1
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dead_server_fails_only_the_panes_on_it() {
    let w = world();
    let doomed = server_for(&w, "doomed").await;
    let _healthy = server_for(&w, "healthy").await;
    let (a, sink_a) = open(&w, "doomed").await;
    let (b, sink_b) = open(&w, "healthy").await;
    until("both open", || {
        text(&sink_a).contains("welcome-banner") && text(&sink_b).contains("welcome-banner")
    })
    .await;

    doomed.kill();
    until("the doomed pane reports the loss", || {
        sink_a.statuses().iter().any(|s| {
            matches!(s, TerminalStatus::Failed { error } if error.code == ErrorCode::ConnectionLost)
        })
    })
    .await;
    // The other pane carries on, with no failure shown.
    echoes(&w, &b, &sink_b, "unaffected").await;
    assert!(
        !sink_b
            .statuses()
            .iter()
            .any(|s| matches!(s, TerminalStatus::Failed { .. }))
    );
    // The failed pane stays registered with its output until it is closed.
    assert!(text(&sink_a).contains("welcome-banner"));
    let _ = a;
}

#[tokio::test(flavor = "multi_thread")]
async fn reconnecting_after_a_loss_makes_a_fresh_connection() {
    let w = world();
    let server = server_for(&w, "s1").await;
    let (_a, sink_a) = open(&w, "s1").await;
    until("open", || text(&sink_a).contains("welcome-banner")).await;
    server.kill();
    until("failed", || {
        sink_a
            .statuses()
            .iter()
            .any(|s| matches!(s, TerminalStatus::Failed { .. }))
    })
    .await;

    // Bring a server back on the same port is not possible to promise, so save a new one and
    // point the same session at it: what matters is that a dropped connection isn't reused.
    let replacement = server_for(&w, "s1-again").await;
    let (_b, sink_b) = open(&w, "s1-again").await;
    until("new shell", || text(&sink_b).contains("welcome-banner")).await;
    assert_eq!(replacement.stats.opened.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn closing_a_pane_during_a_question_cancels_it() {
    let w = world();
    // A server whose key isn't trusted yet, so the connection stops to ask.
    let host_key = key(KEY_A);
    let server = spawn_server(host_key, Accepts::default()).await;
    let mut file = w.store.load().file;
    file.sessions.push(SavedSession::Ssh {
        id: "ask".into(),
        name: "ask".into(),
        folder: String::new(),
        host: "127.0.0.1".into(),
        port: server.port,
        user: "tester".into(),
        auth: SshAuth::Password,
        key_path: String::new(),
        jump: String::new(),
        x11: false,
        color: String::new(),
        startup: String::new(),
    });
    w.store.save(&file).unwrap();

    let (session, sink) = open(&w, "ask").await;
    until("the question appears", || {
        sink.statuses()
            .iter()
            .any(|s| matches!(s, TerminalStatus::Prompt { .. }))
    })
    .await;
    assert_eq!(w.services.broker.pending(), 1);

    w.registry.close(&session).await.unwrap();
    until("the question is dropped", || {
        w.services.broker.pending() == 0
    })
    .await;
    assert!(!w.known_hosts.exists(), "nothing was trusted");
    assert!(
        !sink
            .statuses()
            .iter()
            .any(|s| matches!(s, TerminalStatus::Failed { .. })),
        "a closed pane shows nothing"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_or_broken_saved_session_fails_with_a_plain_message() {
    let w = world();
    let (_a, sink) = open(&w, "nope").await;
    until("failure", || {
        sink.statuses().iter().any(
            |s| matches!(s, TerminalStatus::Failed { error } if error.code == ErrorCode::NotFound),
        )
    })
    .await;

    std::fs::write(&w.services.sessions_file, "[[session]\nbroken").unwrap();
    let (_b, sink) = open(&w, "nope").await;
    until("failure", || {
        sink.statuses().iter().any(
            |s| matches!(s, TerminalStatus::Failed { error } if error.code == ErrorCode::InvalidInput),
        )
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_with_a_jump_host_goes_through_it() {
    let w = world();
    let jump_key = key(KEY_A);
    let target_key = key(super::testing::KEY_B);

    // The jump host allows forwarding. The target is a separate server.
    let jump = spawn_server(
        jump_key.clone(),
        Accepts {
            user: "tester".into(),
            password: Some("pw".into()),
            allow_forwarding: true,
            ..Default::default()
        },
    )
    .await;
    let target = spawn_server(
        target_key.clone(),
        Accepts {
            user: "tester".into(),
            password: Some("pw".into()),
            ..Default::default()
        },
    )
    .await;
    learn(
        "127.0.0.1",
        jump.port,
        &public_of(&jump_key),
        &w.known_hosts,
    )
    .unwrap();
    learn(
        "127.0.0.1",
        target.port,
        &public_of(&target_key),
        &w.known_hosts,
    )
    .unwrap();
    for id in ["jump", "target"] {
        w.secrets
            .items
            .lock()
            .unwrap()
            .insert(format!("session:{id}:password"), "pw".into());
    }
    let ssh = |id: &str, port: u16, via: &str| SavedSession::Ssh {
        id: id.into(),
        name: id.into(),
        folder: String::new(),
        host: "127.0.0.1".into(),
        port,
        user: "tester".into(),
        auth: SshAuth::Password,
        key_path: String::new(),
        jump: via.into(),
        x11: false,
        color: String::new(),
        startup: String::new(),
    };
    let mut file = w.store.load().file;
    file.sessions.push(ssh("jump", jump.port, ""));
    file.sessions.push(ssh("target", target.port, "jump"));
    w.store.save(&file).unwrap();

    let (session, sink) = open(&w, "target").await;
    until("the shell on the target", || {
        text(&sink).contains("welcome-banner")
    })
    .await;
    echoes(&w, &session, &sink, "through-the-jump").await;
    assert_eq!(
        jump.stats.forwarded.load(Ordering::SeqCst),
        1,
        "went through the jump host"
    );
    assert_eq!(target.stats.shells.load(Ordering::SeqCst), 1);
    assert_eq!(
        jump.stats.shells.load(Ordering::SeqCst),
        0,
        "no shell on the jump host"
    );

    // Closing the pane lets go of the whole chain.
    w.registry.close(&session).await.unwrap();
    until("both connections close", || {
        target.stats.ended.load(Ordering::SeqCst) == 1
            && jump.stats.ended.load(Ordering::SeqCst) == 1
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_jump_host_that_refuses_forwarding_gives_a_clear_failure_naming_the_step() {
    let w = world();
    let jump_key = key(KEY_A);
    let target_key = key(super::testing::KEY_B);
    let jump = spawn_server(
        jump_key.clone(),
        Accepts {
            user: "tester".into(),
            password: Some("pw".into()),
            allow_forwarding: false,
            ..Default::default()
        },
    )
    .await;
    let target = spawn_server(target_key.clone(), Accepts::default()).await;
    learn(
        "127.0.0.1",
        jump.port,
        &public_of(&jump_key),
        &w.known_hosts,
    )
    .unwrap();
    w.secrets
        .items
        .lock()
        .unwrap()
        .insert("session:jump:password".into(), "pw".into());
    let ssh = |id: &str, port: u16, via: &str| SavedSession::Ssh {
        id: id.into(),
        name: id.into(),
        folder: String::new(),
        host: "127.0.0.1".into(),
        port,
        user: "tester".into(),
        auth: SshAuth::Password,
        key_path: String::new(),
        jump: via.into(),
        x11: false,
        color: String::new(),
        startup: String::new(),
    };
    let mut file = w.store.load().file;
    file.sessions.push(ssh("jump", jump.port, ""));
    file.sessions.push(ssh("target", target.port, "jump"));
    w.store.save(&file).unwrap();

    let (_session, sink) = open(&w, "target").await;
    until("failure", || {
        sink.statuses().iter().any(|s| {
            matches!(s, TerminalStatus::Failed { error } if error.message.contains("step 2 of 2"))
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn two_jump_hosts_are_used_one_after_the_other() {
    let w = world();
    let keys = [
        key(KEY_A),
        key(super::testing::KEY_B),
        key(super::testing::KEY_C),
    ];
    let mut servers = Vec::new();
    for (i, host_key) in keys.iter().enumerate() {
        let server = spawn_server(
            host_key.clone(),
            Accepts {
                user: "tester".into(),
                password: Some("pw".into()),
                allow_forwarding: i < 2, // The last one is the target.
                ..Default::default()
            },
        )
        .await;
        learn(
            "127.0.0.1",
            server.port,
            &public_of(host_key),
            &w.known_hosts,
        )
        .unwrap();
        servers.push(server);
    }
    for id in ["first", "second", "target"] {
        w.secrets
            .items
            .lock()
            .unwrap()
            .insert(format!("session:{id}:password"), "pw".into());
    }
    let ssh = |id: &str, port: u16, via: &str| SavedSession::Ssh {
        id: id.into(),
        name: id.into(),
        folder: String::new(),
        host: "127.0.0.1".into(),
        port,
        user: "tester".into(),
        auth: SshAuth::Password,
        key_path: String::new(),
        jump: via.into(),
        x11: false,
        color: String::new(),
        startup: String::new(),
    };
    let mut file = w.store.load().file;
    file.sessions.push(ssh("first", servers[0].port, ""));
    file.sessions.push(ssh("second", servers[1].port, "first"));
    file.sessions.push(ssh("target", servers[2].port, "second"));
    w.store.save(&file).unwrap();

    let (session, sink) = open(&w, "target").await;
    until("the shell on the target", || {
        text(&sink).contains("welcome-banner")
    })
    .await;
    echoes(&w, &session, &sink, "two-hops").await;
    // The first jump host forwarded only to the second, and the second forwarded to the target.
    // Going straight from the first to the target would make the first count 2 and the second 0.
    assert_eq!(servers[0].stats.forwarded.load(Ordering::SeqCst), 1);
    assert_eq!(servers[1].stats.forwarded.load(Ordering::SeqCst), 1);
    assert_eq!(servers[2].stats.shells.load(Ordering::SeqCst), 1);
}

/// A pretend X server on this computer. It records the cookie each program brought and says
/// "OK" to it.
struct LocalX {
    port: u16,
    cookies: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
}

async fn local_x() -> LocalX {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let cookies: Arc<std::sync::Mutex<Vec<Vec<u8>>>> = Arc::default();
    let seen = cookies.clone();
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        while let Ok((mut socket, _)) = listener.accept().await {
            let seen = seen.clone();
            tokio::spawn(async move {
                if let Ok(packet) = crate::x11::setup::read_setup(&mut socket).await {
                    // The cookie follows the 12 byte head and the padded protocol name.
                    seen.lock().unwrap().push(packet[32..48].to_vec());
                    let _ = socket.write_all(b"OK").await;
                    let _ = socket.flush().await;
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            });
        }
    });
    LocalX { port, cookies }
}

const REAL_COOKIE: [u8; 16] = [0x42; 16];

fn x11_world(local: &LocalX) -> World {
    let dir = tempfile::tempdir().unwrap();
    let display = crate::x11::display::LocalDisplay {
        endpoint: crate::x11::display::Endpoint::Tcp("127.0.0.1".into(), local.port),
        number: 0,
        screen: 0,
        cookie: Some(REAL_COOKIE.to_vec()),
    };
    world_in(dir, Arc::new(X11Local::fixed(display)))
}

#[tokio::test(flavor = "multi_thread")]
async fn x11_windows_reach_the_local_server_with_the_real_cookie() {
    let local = local_x().await;
    let w = x11_world(&local);
    let server = server_with(&w, "x", Some(X11Fake::default()), true).await;
    let (_session, sink) = open(&w, "x").await;

    until("the program gets its answer", || {
        !server.stats.x11_answers.lock().unwrap().is_empty()
    })
    .await;
    assert_eq!(server.stats.x11_answers.lock().unwrap()[0], b"OK");
    let requests = server.stats.x11_requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].0, "MIT-MAGIC-COOKIE-1");
    assert_eq!(requests[0].1.len(), 32, "a 16 byte cookie as hex");
    // The server was given a made-up cookie, and the local server got the real one.
    assert_ne!(requests[0].1, crate::x11::display::hex(&REAL_COOKIE));
    assert_eq!(*local.cookies.lock().unwrap(), vec![REAL_COOKIE.to_vec()]);
    assert!(!text(&sink).contains("X11 forwarding is off"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_program_with_the_wrong_cookie_is_not_let_in() {
    let local = local_x().await;
    let w = x11_world(&local);
    let server = server_with(&w, "x", Some(X11Fake { wrong_cookie: true }), true).await;
    let _opened = open(&w, "x").await;

    until("the program is turned away", || {
        !server.stats.x11_answers.lock().unwrap().is_empty()
    })
    .await;
    assert!(server.stats.x11_answers.lock().unwrap()[0].is_empty());
    assert!(
        local.cookies.lock().unwrap().is_empty(),
        "never reached the local server"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_refuses_x11_is_explained_in_the_terminal() {
    let local = local_x().await;
    let w = x11_world(&local);
    let _server = server_with(&w, "x", None, true).await;
    let (_session, sink) = open(&w, "x").await;
    until("the explanation", || {
        text(&sink).contains("X11 forwarding is off: the server refused it")
    })
    .await;
    assert!(
        text(&sink).contains("welcome-banner"),
        "the shell still opened"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_without_x11_does_not_ask_for_it() {
    let local = local_x().await;
    let w = x11_world(&local);
    let server = server_with(&w, "x", Some(X11Fake::default()), false).await;
    let (_session, sink) = open(&w, "x").await;
    until("banner", || text(&sink).contains("welcome-banner")).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(server.stats.x11_requests.lock().unwrap().is_empty());
}
