//! The host key rules, end to end against a server running inside the test.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use russh::keys::parse_public_key_base64;
use tokio_util::sync::CancellationToken;

use super::connection::connect;
use super::hostkeys::{HostKeyDecision, decide, fingerprint_of, learn};
use super::testing::{KEY_A, KEY_B, key, start_server};
use crate::error::ErrorCode;
use crate::prompt::{Prompt, PromptBroker, PromptResponse, Prompter};
use crate::terminal::TerminalStatus;
use crate::terminal::testing::{RecordingSink, Sent};

const ECDSA: &str = "AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBGnaa8PYp9RS/n5ZoYO9Cwo9OFOdo3kAXbeuOc2PmnB4lv/DWGX61XoFcZTyvRGi39kPx133ETu3IsZLyMxLfhw=";

struct Setup {
    broker: Arc<PromptBroker>,
    sink: Arc<RecordingSink>,
    cancel: CancellationToken,
    prompter: Prompter,
    _dir: tempfile::TempDir,
    known_hosts: PathBuf,
}

fn setup() -> Setup {
    let broker = PromptBroker::new();
    let sink = Arc::new(RecordingSink::default());
    let cancel = CancellationToken::new();
    let prompter = Prompter::new(broker.clone(), sink.clone(), cancel.clone());
    let dir = tempfile::tempdir().unwrap();
    let known_hosts = dir.path().join("known_hosts");
    Setup {
        broker,
        sink,
        cancel,
        prompter,
        _dir: dir,
        known_hosts,
    }
}

fn prompts(sink: &RecordingSink) -> Vec<Prompt> {
    sink.all()
        .into_iter()
        .filter_map(|s| match s {
            Sent::Status(TerminalStatus::Prompt { prompt, .. }) => Some(prompt),
            _ => None,
        })
        .collect()
}

/// Answers every prompt that appears, the way a user would.
fn answer_prompts(s: &Setup, accept: bool) {
    let broker = s.broker.clone();
    let sink = s.sink.clone();
    tokio::spawn(async move {
        let mut done = std::collections::HashSet::new();
        loop {
            for item in sink.all() {
                if let Sent::Status(TerminalStatus::Prompt { prompt_id, .. }) = item
                    && done.insert(prompt_id.clone())
                {
                    let _ = broker.respond(
                        &prompt_id,
                        PromptResponse {
                            accepted: accept,
                            text: None,
                            save: false,
                        },
                    );
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });
}

/// The key as a server sends it. A key read from a private key file also carries its comment,
/// which isn't sent and would make equal keys compare as different.
fn public(pem: &str) -> russh::keys::PublicKey {
    russh::keys::PublicKey::from(key(pem).public_key().key_data().clone())
}

async fn go(s: &Setup, port: u16) -> Result<(), crate::error::AppError> {
    connect("127.0.0.1", port, s.known_hosts.clone(), s.prompter.clone())
        .await
        .map(|_| ())
}

fn recorded(known_hosts: &Path, port: u16, pem: &str) -> HostKeyDecision {
    decide("127.0.0.1", port, &public(pem), known_hosts).unwrap()
}

#[tokio::test]
async fn first_connection_asks_then_remembers_the_key() {
    let s = setup();
    let port = start_server(key(KEY_A)).await;
    answer_prompts(&s, true);

    go(&s, port).await.unwrap();
    let asked = prompts(&s.sink);
    assert_eq!(asked.len(), 1);
    match &asked[0] {
        Prompt::HostKeyUnknown {
            host,
            port: p,
            algorithm,
            fingerprint,
        } => {
            assert_eq!(host, "127.0.0.1");
            assert_eq!(*p, port);
            assert_eq!(algorithm, "ssh-ed25519");
            assert_eq!(*fingerprint, fingerprint_of(&public(KEY_A)));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        recorded(&s.known_hosts, port, KEY_A),
        HostKeyDecision::Trusted
    );

    // The second time is silent.
    go(&s, port).await.unwrap();
    assert_eq!(prompts(&s.sink).len(), 1);
}

#[tokio::test]
async fn saying_no_on_first_connection_closes_it_and_saves_nothing() {
    let s = setup();
    let port = start_server(key(KEY_A)).await;
    answer_prompts(&s, false);

    let err = go(&s, port).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::HostKeyRejected);
    assert!(!s.known_hosts.exists());
}

#[tokio::test]
async fn a_changed_key_is_blocked_without_any_prompt() {
    let s = setup();
    let port = start_server(key(KEY_B)).await;
    learn("127.0.0.1", port, &public(KEY_A), &s.known_hosts).unwrap();

    let err = go(&s, port).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::HostKeyChanged);
    assert!(
        prompts(&s.sink).is_empty(),
        "there must be no way to continue"
    );
    // The recorded key is untouched.
    assert_eq!(
        recorded(&s.known_hosts, port, KEY_A),
        HostKeyDecision::Trusted
    );
}

#[tokio::test]
async fn a_new_key_type_gets_the_warning_prompt_and_no_changes_if_refused() {
    let s = setup();
    let port = start_server(key(KEY_A)).await;
    let ecdsa = parse_public_key_base64(ECDSA).unwrap();
    learn("127.0.0.1", port, &ecdsa, &s.known_hosts).unwrap();
    let before = std::fs::read_to_string(&s.known_hosts).unwrap();
    answer_prompts(&s, false);

    let err = go(&s, port).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::HostKeyRejected);
    match &prompts(&s.sink)[..] {
        [
            Prompt::HostKeyNewType {
                known_algorithms, ..
            },
        ] => {
            assert_eq!(known_algorithms.len(), 1);
            assert!(known_algorithms[0].starts_with("ecdsa"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(std::fs::read_to_string(&s.known_hosts).unwrap(), before);
}

#[tokio::test]
async fn accepting_a_new_key_type_adds_it() {
    let s = setup();
    let port = start_server(key(KEY_A)).await;
    let ecdsa = parse_public_key_base64(ECDSA).unwrap();
    learn("127.0.0.1", port, &ecdsa, &s.known_hosts).unwrap();
    answer_prompts(&s, true);

    go(&s, port).await.unwrap();
    assert_eq!(
        recorded(&s.known_hosts, port, KEY_A),
        HostKeyDecision::Trusted
    );
}

#[tokio::test]
async fn an_unreadable_known_hosts_fails_without_asking() {
    let s = setup();
    let port = start_server(key(KEY_A)).await;
    let unreadable = s.known_hosts.parent().unwrap().join("locked");
    #[cfg(windows)]
    std::fs::create_dir(&unreadable).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&unreadable, "x").unwrap();
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    if std::fs::read(&unreadable).is_ok() {
        return; // Running as a user that can read anything.
    }
    let err = connect("127.0.0.1", port, unreadable, s.prompter.clone())
        .await
        .map(|_| ())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ConnectionFailed);
    assert!(prompts(&s.sink).is_empty());
}

#[tokio::test]
async fn closing_the_session_while_the_question_is_open_cancels_the_connection() {
    let s = setup();
    let port = start_server(key(KEY_A)).await;
    let cancel = s.cancel.clone();
    let sink = s.sink.clone();
    tokio::spawn(async move {
        while prompts(&sink).is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        cancel.cancel();
    });
    let err = go(&s, port).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Cancelled);
    assert!(!s.known_hosts.exists());
}

#[tokio::test]
async fn a_server_that_is_not_there_gives_a_plain_error() {
    let s = setup();
    // Bind and drop a listener to find a port nothing listens on.
    let port = {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap().port()
    };
    let err = go(&s, port).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::ConnectionFailed);
    assert!(err.message.contains("127.0.0.1"), "{}", err.message);
}

#[tokio::test]
async fn taking_longer_than_the_connect_timeout_to_answer_a_question_does_not_fail_it() {
    // In tests the timeout is 0.6 seconds, and this answer takes 1.5.
    let s = setup();
    let port = start_server(key(KEY_A)).await;
    let broker = s.broker.clone();
    let sink = s.sink.clone();
    tokio::spawn(async move {
        while prompts(&sink).is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        tokio::time::sleep(Duration::from_millis(1500)).await;
        for item in sink.all() {
            if let Sent::Status(TerminalStatus::Prompt { prompt_id, .. }) = item {
                let _ = broker.respond(
                    &prompt_id,
                    PromptResponse {
                        accepted: true,
                        text: None,
                        save: false,
                    },
                );
            }
        }
    });
    go(&s, port).await.unwrap();
    assert_eq!(
        recorded(&s.known_hosts, port, KEY_A),
        HostKeyDecision::Trusted
    );
}
