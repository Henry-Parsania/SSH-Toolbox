//! Key installation: the real script on a real `sh`, and the whole flow against a server running
//! inside the test. Tests that need `sh` do nothing if there isn't one (plain Windows).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use tokio_util::sync::CancellationToken;

use super::backend::SshServices;
use super::connection::{Connection, connect};
use super::hostkeys::learn;
use super::install::{
    InstallOutcome, ServerOs, UNIX_SCRIPT, detect_os, install_key, powershell_command,
    setup_key_login, validate_key_line, windows_script,
};
use super::keygen::generate_key;
use super::pool::ConnectionPool;
use super::testing::{Accepts, KEY_A, MemoryStore, ServerHandle, key, public_of, spawn_server};
use crate::error::ErrorCode;
use crate::keychain::{Keychain, SecretStore};
use crate::prompt::{PromptBroker, Prompter};
use crate::sessions::{SavedSession, SessionStore, SessionsFile, SshAuth};
use crate::terminal::testing::RecordingSink;

const KEY_LINE: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKgqmK9ropguFO7dbtvnHkHDGg1e+OzTvNO7hWlcj5yf sshtoolbox";
const OTHER_KEY: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFD7C7zMBtOyZdIBLxajc17aZb7tI2Wqz3jw0uDu+gvo someone@laptop";

/// A `sh` to run the real script with, if this computer has one.
fn find_sh() -> Option<String> {
    let candidates = [
        "sh".to_string(),
        r"C:\Program Files\Git\bin\sh.exe".to_string(),
        r"C:\Program Files\Git\usr\bin\sh.exe".to_string(),
    ];
    candidates.into_iter().find(|c| {
        std::process::Command::new(c)
            .args(["-c", "exit 0"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

/// Runs the install script the way the server does: key on standard input, `HOME` set.
fn run_script(sh: &str, home: &Path, key: &str) -> (i32, String, String) {
    use std::io::Write;
    let mut child = std::process::Command::new(sh)
        .args(["-c", UNIX_SCRIPT])
        .env("HOME", home)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(key.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    )
}

fn authorized(home: &Path) -> String {
    std::fs::read_to_string(home.join(".ssh").join("authorized_keys")).unwrap_or_default()
}

#[test]
fn a_fresh_user_gets_the_folder_and_file_with_the_right_permissions() {
    let Some(sh) = find_sh() else { return };
    let home = tempfile::tempdir().unwrap();
    let (code, out, err) = run_script(&sh, home.path(), KEY_LINE);
    assert_eq!((code, out.as_str()), (0, "installed"), "{err}");
    assert_eq!(authorized(home.path()), format!("{KEY_LINE}\n"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: PathBuf| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(home.path().join(".ssh")), 0o700);
        assert_eq!(
            mode(home.path().join(".ssh").join("authorized_keys")),
            0o600
        );
    }
}

#[test]
fn an_existing_file_keeps_its_other_keys() {
    let Some(sh) = find_sh() else { return };
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".ssh")).unwrap();
    std::fs::write(
        home.path().join(".ssh").join("authorized_keys"),
        format!("{OTHER_KEY}\n"),
    )
    .unwrap();
    let (code, out, _) = run_script(&sh, home.path(), KEY_LINE);
    assert_eq!((code, out.as_str()), (0, "installed"));
    assert_eq!(
        authorized(home.path()),
        format!("{OTHER_KEY}\n{KEY_LINE}\n")
    );
}

#[test]
fn a_file_with_no_final_line_ending_does_not_join_the_new_key_to_the_last_line() {
    let Some(sh) = find_sh() else { return };
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".ssh")).unwrap();
    std::fs::write(home.path().join(".ssh").join("authorized_keys"), OTHER_KEY).unwrap();
    let (code, out, _) = run_script(&sh, home.path(), KEY_LINE);
    assert_eq!((code, out.as_str()), (0, "installed"));
    let lines: Vec<String> = authorized(home.path()).lines().map(String::from).collect();
    assert_eq!(lines, vec![OTHER_KEY.to_string(), KEY_LINE.to_string()]);
}

#[test]
fn running_twice_adds_no_duplicate() {
    let Some(sh) = find_sh() else { return };
    let home = tempfile::tempdir().unwrap();
    assert_eq!(run_script(&sh, home.path(), KEY_LINE).1, "installed");
    let (code, out, _) = run_script(&sh, home.path(), KEY_LINE);
    assert_eq!((code, out.as_str()), (0, "already-present"));
    assert_eq!(
        authorized(home.path())
            .matches("AAAAC3NzaC1lZDI1NTE5AAAAIKgq")
            .count(),
        1
    );
}

#[test]
fn a_key_is_never_part_of_a_shell_command() {
    // The script reads the key from standard input, so odd characters can't run anything.
    assert!(!UNIX_SCRIPT.contains("{key}"));
    assert!(UNIX_SCRIPT.contains("key=$(cat)"));
}

#[test]
fn only_a_clean_ed25519_line_is_accepted() {
    assert_eq!(validate_key_line(KEY_LINE).unwrap(), KEY_LINE);
    // Any comment is replaced by the app's own, so a key made elsewhere can be installed.
    assert_eq!(
        validate_key_line(OTHER_KEY).unwrap(),
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFD7C7zMBtOyZdIBLxajc17aZb7tI2Wqz3jw0uDu+gvo sshtoolbox"
    );
    assert_eq!(
        validate_key_line("ssh-ed25519 AAAA+/= has a long comment here").unwrap(),
        "ssh-ed25519 AAAA+/= sshtoolbox"
    );
    // A missing or app comment is fine, and the app's comment is always the one used.
    assert_eq!(
        validate_key_line("ssh-ed25519 AAAA+/=\n").unwrap(),
        "ssh-ed25519 AAAA+/= sshtoolbox"
    );
    for bad in [
        "",
        "ssh-rsa AAAAB3NzaC1yc2E sshtoolbox",
        "ssh-ed25519",
        "ssh-ed25519 AAAA'; rm -rf / ; echo ' sshtoolbox",
        "ssh-ed25519 AAAA\nssh-ed25519 BBBB",
        "ssh-ed25519 AA\"AA sshtoolbox",
        "ssh-ed25519 $(whoami) sshtoolbox",
        "command=\"x\" ssh-ed25519 AAAA sshtoolbox",
    ] {
        assert!(validate_key_line(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn the_windows_script_holds_the_key_and_is_encoded_as_utf16() {
    let script = windows_script(KEY_LINE);
    assert!(script.starts_with(&format!("$key = '{KEY_LINE}'")));
    for needle in [
        "S-1-5-32-544",
        "S-1-5-18",
        "administrators_authorized_keys",
        "already-present",
        "acl-failed",
        "icacls.exe",
        "/inheritance:r",
    ] {
        assert!(script.contains(needle), "missing {needle}");
    }
    // A file that doesn't end with a line break gets one before the key is added.
    assert!(script.contains("$bytes[$bytes.Length - 1] -ne 10"));
    // The format placeholders were all filled in.
    assert!(!script.contains("{{") && !script.contains("}}"));
    assert!(script.contains("\"*${sid}:F\""));

    let command = powershell_command(&script);
    let encoded = command
        .strip_prefix("powershell -NoProfile -NonInteractive -EncodedCommand ")
        .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap();
    assert_eq!(bytes.len() % 2, 0);
    let units: Vec<u16> = bytes
        .chunks(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .collect();
    assert_eq!(String::from_utf16(&units).unwrap(), script);
    // Nothing in the command line needs quoting for cmd or PowerShell.
    assert!(
        encoded
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+/=".contains(c))
    );
}

// --- Against a server --------------------------------------------------------------------

struct World {
    services: Arc<SshServices>,
    dir: tempfile::TempDir,
    sink: Arc<RecordingSink>,
    secrets: Arc<MemoryStore>,
}

fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(MemoryStore::default());
    let dyn_secrets: Arc<dyn SecretStore> = secrets.clone();
    let services = Arc::new(SshServices {
        keychain: Arc::new(Keychain::with_store(
            Some(dyn_secrets),
            Duration::from_millis(500),
        )),
        broker: PromptBroker::new(),
        pool: ConnectionPool::new(),
        sessions_file: dir.path().join("sessions.toml"),
        known_hosts: dir.path().join("known_hosts"),
        x11: Arc::new(crate::x11::X11Local::new(dir.path().join("x11"))),
    });
    World {
        services,
        dir,
        sink: Arc::new(RecordingSink::default()),
        secrets,
    }
}

/// A server with a home folder, `sh`, a password, and no keys until one is installed.
async fn server_with_home(w: &World, sh: &str) -> (ServerHandle, PathBuf) {
    let home = w.dir.path().join("server-home");
    std::fs::create_dir_all(&home).unwrap();
    let host_key = key(KEY_A);
    let server = spawn_server(
        host_key.clone(),
        Accepts {
            user: "tester".into(),
            password: Some("pw".into()),
            home: Some(home.clone()),
            sh: Some(sh.into()),
            ..Default::default()
        },
    )
    .await;
    learn(
        "127.0.0.1",
        server.port,
        &public_of(&host_key),
        &w.services.known_hosts,
    )
    .unwrap();
    (server, home)
}

fn save_session(w: &World, id: &str, port: u16) {
    let store = SessionStore::new(w.services.sessions_file.clone());
    let mut file: SessionsFile = store.load().file;
    file.sessions.push(SavedSession::Ssh {
        id: id.into(),
        name: "box".into(),
        folder: String::new(),
        host: "127.0.0.1".into(),
        port,
        user: "tester".into(),
        auth: SshAuth::Password,
        key_path: String::new(),
        jump: String::new(),
        x11: false,
        color: String::new(),
        startup: String::new(),
    });
    store.save(&file).unwrap();
    w.secrets
        .items
        .lock()
        .unwrap()
        .insert(format!("session:{id}:password"), "pw".into());
}

async fn logged_in(w: &World, port: u16) -> Connection {
    let prompter = Prompter::new(
        w.services.broker.clone(),
        w.sink.clone(),
        CancellationToken::new(),
    );
    let mut c = connect(
        "127.0.0.1",
        port,
        w.services.known_hosts.clone(),
        prompter.clone(),
    )
    .await
    .unwrap();
    super::auth::login(
        &mut c,
        &super::auth::Login {
            session_id: "s1".into(),
            host: "127.0.0.1".into(),
            user: "tester".into(),
            method: super::auth::LoginMethod::Password,
        },
        &w.services.keychain,
        &prompter,
    )
    .await
    .unwrap();
    c
}

#[tokio::test(flavor = "multi_thread")]
async fn a_unix_server_is_recognised_and_the_key_is_installed_once() {
    let Some(sh) = find_sh() else { return };
    let w = world();
    let (server, home) = server_with_home(&w, &sh).await;
    save_session(&w, "s1", server.port);
    let c = logged_in(&w, server.port).await;

    assert_eq!(detect_os(&c).await.unwrap(), ServerOs::Unix);
    assert_eq!(
        install_key(&c, ServerOs::Unix, KEY_LINE).await.unwrap(),
        InstallOutcome::Installed
    );
    assert_eq!(
        install_key(&c, ServerOs::Unix, KEY_LINE).await.unwrap(),
        InstallOutcome::AlreadyPresent
    );
    assert_eq!(authorized(&home).lines().count(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_runs_nothing_is_reported_not_guessed() {
    let w = world();
    // No home folder means the test server refuses to run commands at all.
    let host_key = key(KEY_A);
    let server = spawn_server(
        host_key.clone(),
        Accepts {
            user: "tester".into(),
            password: Some("pw".into()),
            ..Default::default()
        },
    )
    .await;
    learn(
        "127.0.0.1",
        server.port,
        &public_of(&host_key),
        &w.services.known_hosts,
    )
    .unwrap();
    save_session(&w, "s1", server.port);
    let c = logged_in(&w, server.port).await;
    let err = detect_os(&c).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Unsupported);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_whole_flow_installs_the_key_and_proves_it_works_on_a_new_connection() {
    let Some(sh) = find_sh() else { return };
    let w = world();
    let (server, home) = server_with_home(&w, &sh).await;
    save_session(&w, "s1", server.port);
    let ssh_dir = w.dir.path().join("client-ssh");
    let made = generate_key(&ssh_dir, None).unwrap();

    let result = setup_key_login(
        &w.services,
        "s1",
        made.private_path.clone(),
        &made.public_line,
        w.sink.clone(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(result.outcome, InstallOutcome::Installed);
    assert!(result.verified);
    assert!(authorized(&home).contains(made.public_line.split(' ').nth(1).unwrap()));
    // Two separate logins happened: the password one, then the key-only one.
    assert!(
        server
            .stats
            .opened
            .load(std::sync::atomic::Ordering::SeqCst)
            >= 2
    );
    // The saved password is still there: deleting it is the user's choice, offered afterwards.
    assert!(
        w.secrets
            .items
            .lock()
            .unwrap()
            .contains_key("session:s1:password")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn success_is_not_reported_if_the_key_login_does_not_work() {
    let Some(sh) = find_sh() else { return };
    let w = world();
    let (server, home) = server_with_home(&w, &sh).await;
    save_session(&w, "s1", server.port);
    let ssh_dir = w.dir.path().join("client-ssh");
    let made = generate_key(&ssh_dir, None).unwrap();
    // Install a different key than the one the app will log in with, so the test login fails.
    let other = generate_key(&w.dir.path().join("other"), None).unwrap();

    let err = setup_key_login(
        &w.services,
        "s1",
        made.private_path.clone(),
        &other.public_line,
        w.sink.clone(),
        CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthFailed);
    assert!(
        err.message.contains("password still works"),
        "{}",
        err.message
    );
    // The key did get installed; only the proof failed.
    assert!(authorized(&home).contains(other.public_line.split(' ').nth(1).unwrap()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bad_key_line_stops_before_any_connection() {
    let w = world();
    let err = setup_key_login(
        &w.services,
        "s1",
        PathBuf::from("x"),
        "ssh-ed25519 AAAA'; rm -rf ~ ; '",
        w.sink.clone(),
        CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}
