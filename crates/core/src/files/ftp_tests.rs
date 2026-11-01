//! FTP and FTPS against a small server running inside the test.
//!
//! The server keeps its files in memory. It speaks just enough FTP for these tests: passive
//! mode, `MLSD` (which can be turned off, to make the client use `LIST`), and explicit or
//! implicit TLS with a self-signed certificate it makes up.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;

use super::{FileBackend, FileKind, FtpFiles, FtpTarget};
use crate::error::{AppError, ErrorCode};
use crate::keychain::{Keychain, SecretStore};
use crate::prompt::{Prompt, PromptBroker, PromptResponse, Prompter};
use crate::secret::Secret;
use crate::sessions::FtpSecurity;
use crate::ssh::testing::MemoryStore;
use crate::terminal::{OutputSink, TerminalStatus};

// ---------------------------------------------------------------- the server

trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}
type BoxIo = Box<dyn Io>;

#[derive(Default)]
struct Fs {
    files: BTreeMap<String, Vec<u8>>,
    dirs: BTreeSet<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Tls {
    None,
    Explicit,
    Implicit,
}

struct Options {
    tls: Tls,
    mlsd: bool,
    /// Answers PASV with an address that isn't this server's, as a hostile server might.
    bounce: bool,
}

struct Server {
    port: u16,
    fs: Arc<Mutex<Fs>>,
    /// Every command the server received, with passwords left out.
    log: Arc<Mutex<Vec<String>>>,
    /// SHA-256 of the certificate it presents, in hex.
    fingerprint: String,
    cert_der: Vec<u8>,
    cancel: CancellationToken,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

struct Ctx {
    fs: Arc<Mutex<Fs>>,
    log: Arc<Mutex<Vec<String>>>,
    acceptor: TlsAcceptor,
    options: Options,
}

fn resolve(cwd: &str, arg: &str) -> String {
    let joined = if arg.starts_with('/') {
        arg.to_string()
    } else {
        format!("{}/{arg}", cwd.trim_end_matches('/'))
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    format!("/{}", parts.join("/"))
}

fn parent_and_name(path: &str) -> (String, String) {
    match path.rsplit_once('/') {
        Some(("", name)) => ("/".into(), name.into()),
        Some((dir, name)) => (dir.into(), name.into()),
        None => ("/".into(), path.into()),
    }
}

async fn send(io: &mut BoxIo, line: &str) {
    let _ = io.write_all(format!("{line}\r\n").as_bytes()).await;
    let _ = io.flush().await;
}

/// Reads one line a byte at a time, so nothing is held back when TLS starts partway through.
async fn read_line(io: &mut BoxIo) -> Option<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match io.read(&mut byte).await {
            Ok(1) => {
                if byte[0] == b'\n' {
                    return Some(String::from_utf8_lossy(&line).trim_end().to_string());
                }
                line.push(byte[0]);
            }
            _ => return None,
        }
    }
}

fn listing(ctx: &Ctx, dir: &str, machine: bool) -> Option<String> {
    let fs = ctx.fs.lock().unwrap();
    if !fs.dirs.contains(dir) {
        return None;
    }
    let mut out = String::new();
    let mut entries: Vec<(String, bool, usize)> = Vec::new();
    for sub in &fs.dirs {
        let (parent, name) = parent_and_name(sub);
        if sub != "/" && parent == dir {
            entries.push((name, true, 0));
        }
    }
    for (path, bytes) in &fs.files {
        let (parent, name) = parent_and_name(path);
        if parent == dir {
            entries.push((name, false, bytes.len()));
        }
    }
    for (name, is_dir, size) in entries {
        if machine {
            let kind = if is_dir { "dir" } else { "file" };
            out.push_str(&format!(
                "type={kind};size={size};modify=20250102030405; {name}\r\n"
            ));
        } else {
            let kind = if is_dir { 'd' } else { '-' };
            out.push_str(&format!(
                "{kind}rwxr-xr-x 1 owner group {size} Jan 02 2025 {name}\r\n"
            ));
        }
    }
    Some(out)
}

async fn session(mut io: BoxIo, ctx: Arc<Ctx>) {
    send(&mut io, "220 test server ready").await;
    let mut cwd = "/".to_string();
    let mut user_ok = false;
    let mut authed = false;
    let mut passive: Option<TcpListener> = None;
    let mut rename_from: Option<String> = None;
    let mut protect = false;

    while let Some(line) = read_line(&mut io).await {
        let (command, arg) = line.split_once(' ').unwrap_or((line.as_str(), ""));
        let command = command.to_uppercase();
        ctx.log.lock().unwrap().push(if command == "PASS" {
            "PASS".into()
        } else {
            line.clone()
        });

        match command.as_str() {
            "AUTH" if ctx.options.tls == Tls::Explicit => {
                send(&mut io, "234 go ahead").await;
                match ctx.acceptor.accept(io).await {
                    Ok(tls) => io = Box::new(tls),
                    Err(_) => return,
                }
                continue;
            }
            "AUTH" => send(&mut io, "502 no TLS here").await,
            "PBSZ" => send(&mut io, "200 ok").await,
            "PROT" => {
                protect = arg == "P";
                send(&mut io, "200 ok").await;
            }
            "USER" => {
                user_ok = arg == "alice";
                send(&mut io, "331 password please").await;
            }
            "PASS" => {
                authed = user_ok && arg == "secret";
                if authed {
                    send(&mut io, "230 welcome").await;
                } else {
                    send(&mut io, "530 login incorrect").await;
                }
            }
            "QUIT" => {
                send(&mut io, "221 bye").await;
                return;
            }
            _ if !authed => send(&mut io, "530 log in first").await,
            "SYST" => send(&mut io, "215 UNIX Type: L8").await,
            "FEAT" => {
                if ctx.options.mlsd {
                    send(&mut io, "211-Features:\r\n MLSD\r\n211 End").await;
                } else {
                    send(&mut io, "211 No features").await;
                }
            }
            "TYPE" | "OPTS" | "NOOP" => send(&mut io, "200 ok").await,
            "PWD" => send(&mut io, &format!("257 \"{cwd}\" is the current folder")).await,
            "CWD" => {
                let target = resolve(&cwd, arg);
                if ctx.fs.lock().unwrap().dirs.contains(&target) {
                    cwd = target;
                    send(&mut io, "250 ok").await;
                } else {
                    send(&mut io, "550 no such folder").await;
                }
            }
            "PASV" | "EPSV" => {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let port = listener.local_addr().unwrap().port();
                passive = Some(listener);
                if command == "PASV" {
                    let address = if ctx.options.bounce {
                        "10,255,255,1"
                    } else {
                        "127,0,0,1"
                    };
                    send(
                        &mut io,
                        &format!(
                            "227 Entering Passive Mode ({address},{},{})",
                            port / 256,
                            port % 256
                        ),
                    )
                    .await;
                } else {
                    send(
                        &mut io,
                        &format!("229 Entering Extended Passive Mode (|||{port}|)"),
                    )
                    .await;
                }
            }
            "MLSD" if !ctx.options.mlsd => send(&mut io, "500 unknown command").await,
            "LIST" | "MLSD" | "RETR" | "STOR" => {
                let Some(listener) = passive.take() else {
                    send(&mut io, "425 use PASV first").await;
                    continue;
                };
                let target = resolve(&cwd, arg);
                // Check before opening the data connection, as real servers do.
                let ready = match command.as_str() {
                    "RETR" => ctx.fs.lock().unwrap().files.contains_key(&target),
                    "STOR" => ctx
                        .fs
                        .lock()
                        .unwrap()
                        .dirs
                        .contains(&parent_and_name(&target).0),
                    _ => ctx.fs.lock().unwrap().dirs.contains(&target),
                };
                if !ready {
                    send(&mut io, "550 can't do that").await;
                    continue;
                }
                send(&mut io, "150 opening data connection").await;
                let Ok((tcp, _)) = listener.accept().await else {
                    send(&mut io, "425 no data connection").await;
                    continue;
                };
                let mut data: BoxIo = if protect {
                    match ctx.acceptor.accept(tcp).await {
                        Ok(tls) => Box::new(tls),
                        Err(_) => {
                            send(&mut io, "425 TLS failed").await;
                            continue;
                        }
                    }
                } else {
                    Box::new(tcp)
                };
                match command.as_str() {
                    "RETR" => {
                        let bytes = ctx.fs.lock().unwrap().files[&target].clone();
                        let _ = data.write_all(&bytes).await;
                    }
                    "STOR" => {
                        let mut bytes = Vec::new();
                        let _ = data.read_to_end(&mut bytes).await;
                        ctx.fs.lock().unwrap().files.insert(target, bytes);
                    }
                    other => {
                        let text = listing(&ctx, &target, other == "MLSD").unwrap_or_default();
                        let _ = data.write_all(text.as_bytes()).await;
                    }
                }
                let _ = data.shutdown().await;
                drop(data);
                send(&mut io, "226 done").await;
            }
            "MKD" => {
                let target = resolve(&cwd, arg);
                let reply = {
                    let mut fs = ctx.fs.lock().unwrap();
                    let (parent, _) = parent_and_name(&target);
                    if fs.dirs.contains(&target)
                        || fs.files.contains_key(&target)
                        || !fs.dirs.contains(&parent)
                    {
                        "550 can't make that".to_string()
                    } else {
                        fs.dirs.insert(target.clone());
                        format!("257 \"{target}\" created")
                    }
                };
                send(&mut io, &reply).await;
            }
            "RMD" => {
                let target = resolve(&cwd, arg);
                let removed = {
                    let mut fs = ctx.fs.lock().unwrap();
                    let busy = fs
                        .dirs
                        .iter()
                        .any(|d| d != &target && parent_and_name(d).0 == target)
                        || fs.files.keys().any(|f| parent_and_name(f).0 == target);
                    fs.dirs.contains(&target) && !busy && target != "/" && fs.dirs.remove(&target)
                };
                send(
                    &mut io,
                    if removed {
                        "250 removed"
                    } else {
                        "550 can't remove that"
                    },
                )
                .await;
            }
            "DELE" => {
                let target = resolve(&cwd, arg);
                let removed = ctx.fs.lock().unwrap().files.remove(&target).is_some();
                send(
                    &mut io,
                    if removed {
                        "250 deleted"
                    } else {
                        "550 no such file"
                    },
                )
                .await;
            }
            "RNFR" => {
                let target = resolve(&cwd, arg);
                let found = {
                    let fs = ctx.fs.lock().unwrap();
                    fs.files.contains_key(&target) || fs.dirs.contains(&target)
                };
                if found {
                    rename_from = Some(target);
                    send(&mut io, "350 ready for the new name").await;
                } else {
                    send(&mut io, "550 no such file").await;
                }
            }
            "RNTO" => {
                let to = resolve(&cwd, arg);
                let from = rename_from.take();
                let moved = {
                    let mut fs = ctx.fs.lock().unwrap();
                    match from {
                        Some(from) if fs.files.contains_key(&from) => {
                            let bytes = fs.files.remove(&from).unwrap();
                            fs.files.insert(to, bytes);
                            true
                        }
                        Some(from) if fs.dirs.remove(&from) => {
                            fs.dirs.insert(to);
                            true
                        }
                        _ => false,
                    }
                };
                send(
                    &mut io,
                    if moved {
                        "250 renamed"
                    } else {
                        "550 can't rename"
                    },
                )
                .await;
            }
            _ => send(&mut io, "502 not implemented").await,
        }
    }
}

async fn spawn(options: Options) -> Server {
    let certified =
        rcgen::generate_simple_self_signed(vec!["localhost".to_string(), "127.0.0.1".to_string()])
            .unwrap();
    let cert_der = certified.cert.der().to_vec();
    let key = rustls::pki_types::PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![rustls::pki_types::CertificateDer::from(cert_der.clone())],
            key.into(),
        )
        .unwrap();

    let fs = Arc::new(Mutex::new(Fs::default()));
    fs.lock().unwrap().dirs.insert("/".into());
    let log = Arc::new(Mutex::new(Vec::new()));
    let ctx = Arc::new(Ctx {
        fs: fs.clone(),
        log: log.clone(),
        acceptor: TlsAcceptor::from(Arc::new(config)),
        options,
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    tokio::spawn(async move {
        loop {
            let accepted = tokio::select! {
                accepted = listener.accept() => accepted,
                _ = stop.cancelled() => return,
            };
            let Ok((tcp, _)) = accepted else { return };
            let ctx = ctx.clone();
            tokio::spawn(async move {
                let io: BoxIo = if ctx.options.tls == Tls::Implicit {
                    match ctx.acceptor.accept(tcp).await {
                        Ok(tls) => Box::new(tls),
                        Err(_) => return,
                    }
                } else {
                    Box::new(tcp)
                };
                session(io, ctx).await;
            });
        }
    });
    Server {
        port,
        fs,
        log,
        fingerprint: hex(&Sha256::digest(&cert_der)),
        cert_der,
        cancel,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------- the user

enum Answer {
    Yes,
    No,
    Text(&'static str),
}

/// Answers questions as scripted and keeps them, so a test can see what was asked.
struct Script {
    broker: Arc<PromptBroker>,
    answers: Mutex<VecDeque<Answer>>,
    asked: Mutex<Vec<Prompt>>,
}

impl OutputSink for Script {
    fn send_data(&self, _chunk: Vec<u8>) -> Result<(), AppError> {
        Ok(())
    }

    fn send_status(&self, status: TerminalStatus) -> Result<(), AppError> {
        if let TerminalStatus::Prompt { prompt_id, prompt } = status {
            self.asked.lock().unwrap().push(prompt);
            let answer = self
                .answers
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Answer::No);
            let response = match answer {
                Answer::Yes => PromptResponse {
                    accepted: true,
                    text: None,
                    save: false,
                },
                Answer::No => PromptResponse {
                    accepted: false,
                    text: None,
                    save: false,
                },
                Answer::Text(text) => PromptResponse {
                    accepted: true,
                    text: Some(Secret::new(text)),
                    save: false,
                },
            };
            let _ = self.broker.respond(&prompt_id, response);
        }
        Ok(())
    }
}

struct Setup {
    script: Arc<Script>,
    prompter: Prompter,
    keychain: Keychain,
}

fn setup(answers: Vec<Answer>, saved_password: Option<&str>) -> Setup {
    let broker = PromptBroker::new();
    let script = Arc::new(Script {
        broker: broker.clone(),
        answers: Mutex::new(answers.into()),
        asked: Mutex::default(),
    });
    let prompter = Prompter::new(broker, script.clone(), CancellationToken::new());
    let secrets = Arc::new(MemoryStore::default());
    if let Some(password) = saved_password {
        secrets
            .items
            .lock()
            .unwrap()
            .insert("session:f1:ftp_password".into(), password.into());
    }
    let store: Arc<dyn SecretStore> = secrets;
    Setup {
        script,
        prompter,
        keychain: Keychain::with_store(Some(store), Duration::from_millis(500)),
    }
}

fn target(server: &Server, security: FtpSecurity, trusted: &str) -> FtpTarget {
    FtpTarget {
        session_id: "f1".into(),
        host: "127.0.0.1".into(),
        port: server.port,
        user: "alice".into(),
        security,
        trusted_cert_sha256: trusted.into(),
    }
}

fn roots_with(cert: &[u8]) -> rustls::RootCertStore {
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(rustls::pki_types::CertificateDer::from(cert.to_vec()))
        .unwrap();
    roots
}

fn data(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

async fn put(files: &FtpFiles, path: &str, bytes: &[u8]) {
    let mut src = std::io::Cursor::new(bytes.to_vec());
    files
        .write_from(path, &mut src, &|_| {}, CancellationToken::new())
        .await
        .unwrap();
}

async fn get(files: &FtpFiles, path: &str) -> Vec<u8> {
    let mut out = Vec::new();
    files
        .read_to(path, &mut out, &|_| {}, CancellationToken::new())
        .await
        .unwrap();
    out
}

/// What a full round of file work should do on any kind of connection.
async fn exercise(files: &FtpFiles, server: &Server) {
    assert_eq!(files.home().await.unwrap(), "/");
    files.mkdir("/docs").await.unwrap();
    let again = files.mkdir("/docs").await.unwrap_err();
    assert_eq!(again.code, ErrorCode::AlreadyExists);

    let big = data(300_000);
    put(files, "/docs/big.bin", &big).await;
    put(files, "/docs/small.txt", b"hello").await;
    assert_eq!(get(files, "/docs/big.bin").await, big);
    assert_eq!(server.fs.lock().unwrap().files["/docs/small.txt"], b"hello");

    let mut listed = files.list("/docs").await.unwrap();
    listed.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].name, "big.bin");
    assert_eq!(listed[0].size, Some(300_000));
    assert_eq!(listed[0].kind, FileKind::File);
    assert_eq!(listed[0].path, "/docs/big.bin");
    let root = files.list("/").await.unwrap();
    assert_eq!(root.len(), 1);
    assert_eq!(root[0].kind, FileKind::Dir);

    assert_eq!(files.stat("/docs/small.txt").await.unwrap().size, Some(5));
    assert_eq!(
        files.stat("/docs/missing").await.unwrap_err().code,
        ErrorCode::NotFound
    );

    files
        .rename("/docs/small.txt", "/docs/renamed.txt")
        .await
        .unwrap();
    assert!(files.stat("/docs/renamed.txt").await.is_ok());
    files.remove_file("/docs/renamed.txt").await.unwrap();
    files.remove_file("/docs/big.bin").await.unwrap();
    files.remove_dir("/docs").await.unwrap();
    assert!(files.list("/").await.unwrap().is_empty());
    assert_eq!(
        files
            .read_to(
                "/nothing",
                &mut Vec::new(),
                &|_| {},
                CancellationToken::new()
            )
            .await
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
}

fn asked(script: &Script) -> Vec<Prompt> {
    script.asked.lock().unwrap().clone()
}

fn sent_user(server: &Server) -> bool {
    server
        .log
        .lock()
        .unwrap()
        .iter()
        .any(|l| l.starts_with("USER"))
}

// ---------------------------------------------------------------- plain FTP

#[tokio::test]
async fn plain_ftp_warns_first_and_then_does_everything() {
    let server = spawn(Options {
        tls: Tls::None,
        mlsd: true,
        bounce: false,
    })
    .await;
    let s = setup(vec![Answer::Yes], Some("secret"));
    let opened = FtpFiles::connect(
        &target(&server, FtpSecurity::Plain, ""),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap();
    assert_eq!(
        asked(&s.script),
        vec![Prompt::PlainFtp {
            host: "127.0.0.1".into()
        }]
    );
    assert!(opened.accepted_cert.is_none());
    assert!(!opened.files.capabilities().chmod);
    exercise(&opened.files, &server).await;
}

#[tokio::test]
async fn declining_the_plain_ftp_warning_sends_nothing() {
    let server = spawn(Options {
        tls: Tls::None,
        mlsd: true,
        bounce: false,
    })
    .await;
    let s = setup(vec![Answer::No], Some("secret"));
    let err = FtpFiles::connect(
        &target(&server, FtpSecurity::Plain, ""),
        &s.keychain,
        &s.prompter,
    )
    .await
    .err()
    .unwrap();
    assert_eq!(err.code, ErrorCode::Cancelled);
    assert!(
        !sent_user(&server),
        "no login may be attempted after a refusal"
    );
    assert!(server.log.lock().unwrap().iter().all(|l| l != "PASS"));
}

#[tokio::test]
async fn the_warning_comes_every_time() {
    let server = spawn(Options {
        tls: Tls::None,
        mlsd: true,
        bounce: false,
    })
    .await;
    let s = setup(vec![Answer::Yes, Answer::Yes], Some("secret"));
    for _ in 0..2 {
        FtpFiles::connect(
            &target(&server, FtpSecurity::Plain, ""),
            &s.keychain,
            &s.prompter,
        )
        .await
        .unwrap();
    }
    let warnings = asked(&s.script)
        .into_iter()
        .filter(|p| matches!(p, Prompt::PlainFtp { .. }))
        .count();
    assert_eq!(warnings, 2);
}

#[tokio::test]
async fn a_wrong_saved_password_leads_to_a_question() {
    let server = spawn(Options {
        tls: Tls::None,
        mlsd: true,
        bounce: false,
    })
    .await;
    let s = setup(
        vec![Answer::Yes, Answer::Text("secret")],
        Some("old-password"),
    );
    FtpFiles::connect(
        &target(&server, FtpSecurity::Plain, ""),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap();
    assert!(matches!(asked(&s.script)[1], Prompt::Password { .. }));
}

#[tokio::test]
async fn a_refused_password_is_an_auth_failure() {
    let server = spawn(Options {
        tls: Tls::None,
        mlsd: true,
        bounce: false,
    })
    .await;
    let s = setup(vec![Answer::Yes, Answer::Text("nope")], None);
    let err = FtpFiles::connect(
        &target(&server, FtpSecurity::Plain, ""),
        &s.keychain,
        &s.prompter,
    )
    .await
    .err()
    .unwrap();
    assert_eq!(err.code, ErrorCode::AuthFailed);
    assert!(!err.message.contains("nope"));
}

#[tokio::test]
async fn servers_without_mlsd_are_read_through_list() {
    let server = spawn(Options {
        tls: Tls::None,
        mlsd: false,
        bounce: false,
    })
    .await;
    let s = setup(vec![Answer::Yes], Some("secret"));
    let opened = FtpFiles::connect(
        &target(&server, FtpSecurity::Plain, ""),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap();
    exercise(&opened.files, &server).await;
    assert!(
        server
            .log
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.starts_with("LIST"))
    );
}

// ---------------------------------------------------------------- FTPS

#[tokio::test]
async fn explicit_ftps_with_an_unknown_certificate_asks_and_then_connects() {
    let server = spawn(Options {
        tls: Tls::Explicit,
        mlsd: true,
        bounce: false,
    })
    .await;
    let s = setup(vec![Answer::Yes], Some("secret"));
    let opened = FtpFiles::connect_with_roots(
        &target(&server, FtpSecurity::Explicit, ""),
        &s.keychain,
        &s.prompter,
        rustls::RootCertStore::empty(),
    )
    .await
    .unwrap();
    let questions = asked(&s.script);
    assert_eq!(questions.len(), 1);
    match &questions[0] {
        Prompt::FtpsCertificate {
            fingerprint,
            reason,
            subject,
            host,
            ..
        } => {
            assert_eq!(fingerprint, &server.fingerprint);
            assert!(reason.contains("self-signed"), "{reason}");
            assert!(!subject.is_empty());
            assert_eq!(host, "127.0.0.1");
        }
        other => panic!("unexpected question {other:?}"),
    }
    assert_eq!(
        opened.accepted_cert.as_deref(),
        Some(server.fingerprint.as_str())
    );
    // Files and the data channel work over TLS.
    exercise(&opened.files, &server).await;
}

#[tokio::test]
async fn refusing_the_certificate_stops_before_any_login() {
    let server = spawn(Options {
        tls: Tls::Explicit,
        mlsd: true,
        bounce: false,
    })
    .await;
    let s = setup(vec![Answer::No], Some("secret"));
    let err = FtpFiles::connect_with_roots(
        &target(&server, FtpSecurity::Explicit, ""),
        &s.keychain,
        &s.prompter,
        rustls::RootCertStore::empty(),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(err.code, ErrorCode::CertificateRejected);
    assert!(
        !sent_user(&server),
        "the password must not be sent to a refused server"
    );
}

#[tokio::test]
async fn a_trusted_authority_needs_no_question() {
    let server = spawn(Options {
        tls: Tls::Explicit,
        mlsd: true,
        bounce: false,
    })
    .await;
    let s = setup(vec![], Some("secret"));
    let opened = FtpFiles::connect_with_roots(
        &target(&server, FtpSecurity::Explicit, ""),
        &s.keychain,
        &s.prompter,
        roots_with(&server.cert_der),
    )
    .await
    .unwrap();
    assert!(asked(&s.script).is_empty());
    assert!(opened.accepted_cert.is_none());
    exercise(&opened.files, &server).await;
}

#[tokio::test]
async fn an_accepted_fingerprint_is_trusted_next_time() {
    let server = spawn(Options {
        tls: Tls::Explicit,
        mlsd: true,
        bounce: false,
    })
    .await;
    let s = setup(vec![], Some("secret"));
    FtpFiles::connect_with_roots(
        &target(&server, FtpSecurity::Explicit, &server.fingerprint),
        &s.keychain,
        &s.prompter,
        rustls::RootCertStore::empty(),
    )
    .await
    .unwrap();
    assert!(asked(&s.script).is_empty());
}

#[tokio::test]
async fn a_changed_certificate_is_asked_about_again() {
    let server = spawn(Options {
        tls: Tls::Explicit,
        mlsd: true,
        bounce: false,
    })
    .await;
    let s = setup(vec![Answer::No], Some("secret"));
    let old = "00".repeat(32);
    let err = FtpFiles::connect_with_roots(
        &target(&server, FtpSecurity::Explicit, &old),
        &s.keychain,
        &s.prompter,
        rustls::RootCertStore::empty(),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(err.code, ErrorCode::CertificateRejected);
    match &asked(&s.script)[0] {
        Prompt::FtpsCertificate {
            reason,
            fingerprint,
            ..
        } => {
            assert!(
                reason.contains("not the certificate you accepted before"),
                "{reason}"
            );
            assert_eq!(fingerprint, &server.fingerprint);
        }
        other => panic!("unexpected question {other:?}"),
    }
    assert!(!sent_user(&server));
}

#[tokio::test]
async fn implicit_ftps_works_the_same_way() {
    let server = spawn(Options {
        tls: Tls::Implicit,
        mlsd: true,
        bounce: false,
    })
    .await;
    let s = setup(vec![Answer::Yes], Some("secret"));
    let opened = FtpFiles::connect_with_roots(
        &target(&server, FtpSecurity::Implicit, ""),
        &s.keychain,
        &s.prompter,
        rustls::RootCertStore::empty(),
    )
    .await
    .unwrap();
    assert_eq!(
        opened.accepted_cert.as_deref(),
        Some(server.fingerprint.as_str())
    );
    exercise(&opened.files, &server).await;
}

#[tokio::test]
async fn a_server_that_isnt_there_gives_a_plain_error() {
    let server = spawn(Options {
        tls: Tls::None,
        mlsd: true,
        bounce: false,
    })
    .await;
    let mut gone = target(&server, FtpSecurity::Plain, "");
    gone.port = 1;
    let s = setup(vec![Answer::Yes], Some("secret"));
    let err = FtpFiles::connect(&gone, &s.keychain, &s.prompter)
        .await
        .err()
        .unwrap();
    assert_eq!(err.code, ErrorCode::ConnectionFailed);
}

#[tokio::test]
async fn a_plain_server_refused_for_tls_gives_a_connection_error() {
    // The session says explicit TLS, but the server only knows plain FTP.
    let server = spawn(Options {
        tls: Tls::None,
        mlsd: true,
        bounce: false,
    })
    .await;
    let s = setup(vec![Answer::Yes], Some("secret"));
    let err = FtpFiles::connect_with_roots(
        &target(&server, FtpSecurity::Explicit, ""),
        &s.keychain,
        &s.prompter,
        rustls::RootCertStore::empty(),
    )
    .await
    .err()
    .unwrap();
    assert!(asked(&s.script).is_empty());
    assert!(!sent_user(&server));
    assert_ne!(err.code, ErrorCode::Cancelled);
}

#[tokio::test]
async fn a_server_cannot_send_data_connections_to_another_address() {
    // The server's PASV answer names 10.255.255.1. The data connection must still go to the
    // address the control connection is on, so the transfer works and nothing is sent elsewhere.
    let server = spawn(Options {
        tls: Tls::None,
        mlsd: true,
        bounce: true,
    })
    .await;
    let s = setup(vec![Answer::Yes], Some("secret"));
    let opened = FtpFiles::connect(
        &target(&server, FtpSecurity::Plain, ""),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap();
    put(&opened.files, "/x.bin", b"data").await;
    assert_eq!(get(&opened.files, "/x.bin").await, b"data");
}
