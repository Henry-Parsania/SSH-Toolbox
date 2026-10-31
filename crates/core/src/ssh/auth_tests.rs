//! Each login method, against a server running inside the test.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::auth::{Login, LoginMethod, login};
use super::connection::{Connection, connect};
use super::testing::{
    Accepts, ENCRYPTED_PASSPHRASE, KEY_A, KEY_B, KEY_ENCRYPTED, MemoryStore, key, public_of,
    start_server_with,
};
use crate::error::ErrorCode;
use crate::keychain::{Keychain, SecretStore};
use crate::prompt::{Prompt, PromptBroker, PromptResponse, Prompter};
use crate::terminal::TerminalStatus;
use crate::terminal::testing::{RecordingSink, Sent};

const USER: &str = "tester";
const PASSWORD: &str = "right-password";

struct Setup {
    broker: Arc<PromptBroker>,
    sink: Arc<RecordingSink>,
    prompter: Prompter,
    store: Arc<MemoryStore>,
    keychain: Keychain,
    dir: tempfile::TempDir,
}

fn setup() -> Setup {
    let broker = PromptBroker::new();
    let sink = Arc::new(RecordingSink::default());
    let prompter = Prompter::new(broker.clone(), sink.clone(), CancellationToken::new());
    let store = Arc::new(MemoryStore::default());
    let dyn_store: Arc<dyn SecretStore> = store.clone();
    let keychain = Keychain::with_store(Some(dyn_store), Duration::from_millis(500));
    Setup {
        broker,
        sink,
        prompter,
        store,
        keychain,
        dir: tempfile::tempdir().unwrap(),
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

/// Answers each prompt that appears with the next answer in the list: `(accepted, text, save)`.
fn answer_with(s: &Setup, answers: Vec<(bool, Option<&'static str>, bool)>) {
    let broker = s.broker.clone();
    let sink = s.sink.clone();
    tokio::spawn(async move {
        let mut answers = answers.into_iter();
        let mut done = std::collections::HashSet::new();
        loop {
            for item in sink.all() {
                if let Sent::Status(TerminalStatus::Prompt { prompt_id, .. }) = item
                    && done.insert(prompt_id.clone())
                    && let Some((accepted, text, save)) = answers.next()
                {
                    let response: PromptResponse = serde_json::from_value(serde_json::json!({
                        "accepted": accepted, "text": text, "save": save
                    }))
                    .unwrap();
                    let _ = broker.respond(&prompt_id, response);
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });
}

/// A connection to a server that accepts the given logins. Host keys are trusted up front.
async fn connected(s: &Setup, accepts: Accepts) -> Connection {
    let host_key = key(KEY_A);
    let known_hosts = s.dir.path().join("known_hosts");
    let port = start_server_with(host_key.clone(), accepts).await;
    super::hostkeys::learn("127.0.0.1", port, &public_of(&host_key), &known_hosts).unwrap();
    connect("127.0.0.1", port, known_hosts, s.prompter.clone())
        .await
        .unwrap()
}

fn who(method: LoginMethod) -> Login {
    Login {
        session_id: "s1".into(),
        host: "127.0.0.1".into(),
        user: USER.into(),
        method,
    }
}

fn password_server() -> Accepts {
    Accepts {
        user: USER.into(),
        password: Some(PASSWORD.into()),
        keys: vec![],
        ..Default::default()
    }
}

fn key_server(pem: &str) -> Accepts {
    Accepts {
        user: USER.into(),
        password: None,
        keys: vec![public_of(&key(pem))],
        ..Default::default()
    }
}

fn write_key(s: &Setup, name: &str, pem: &str) -> String {
    let path = s.dir.path().join(name);
    std::fs::write(&path, pem).unwrap();
    path.display().to_string()
}

#[tokio::test]
async fn a_typed_password_logs_in() {
    let s = setup();
    let mut c = connected(&s, password_server()).await;
    answer_with(&s, vec![(true, Some(PASSWORD), false)]);
    login(
        &mut c,
        &who(LoginMethod::Password),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap();
    match &prompts(&s.sink)[..] {
        [
            Prompt::Password {
                user,
                host,
                offer_save,
            },
        ] => {
            assert_eq!(user, USER);
            assert_eq!(host, "127.0.0.1");
            assert!(*offer_save);
        }
        other => panic!("{other:?}"),
    }
    assert!(
        s.store.items.lock().unwrap().is_empty(),
        "not saved unless asked"
    );
}

#[tokio::test]
async fn a_wrong_password_is_auth_failed() {
    let s = setup();
    let mut c = connected(&s, password_server()).await;
    answer_with(&s, vec![(true, Some("wrong"), false)]);
    let err = login(
        &mut c,
        &who(LoginMethod::Password),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthFailed);
    assert!(!err.message.contains("wrong"), "never echo the password");
}

#[tokio::test]
async fn cancelling_the_password_prompt_cancels_the_login() {
    let s = setup();
    let mut c = connected(&s, password_server()).await;
    answer_with(&s, vec![(false, None, false)]);
    let err = login(
        &mut c,
        &who(LoginMethod::Password),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::Cancelled);
}

#[tokio::test]
async fn saving_the_password_means_the_next_login_does_not_ask() {
    let s = setup();
    let mut c = connected(&s, password_server()).await;
    answer_with(&s, vec![(true, Some(PASSWORD), true)]);
    login(
        &mut c,
        &who(LoginMethod::Password),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap();
    assert_eq!(
        s.store
            .items
            .lock()
            .unwrap()
            .get("session:s1:password")
            .map(String::as_str),
        Some(PASSWORD)
    );

    let before = prompts(&s.sink).len();
    let mut again = connected(&s, password_server()).await;
    login(
        &mut again,
        &who(LoginMethod::Password),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap();
    assert_eq!(prompts(&s.sink).len(), before, "no new question");
}

#[tokio::test]
async fn a_stale_saved_password_falls_back_to_asking() {
    let s = setup();
    s.store
        .items
        .lock()
        .unwrap()
        .insert("session:s1:password".into(), "old-password".into());
    let mut c = connected(&s, password_server()).await;
    answer_with(&s, vec![(true, Some(PASSWORD), false)]);
    login(
        &mut c,
        &who(LoginMethod::Password),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap();
    assert_eq!(prompts(&s.sink).len(), 1);
}

#[tokio::test]
async fn without_a_keychain_the_user_is_asked_each_time_and_no_save_is_offered() {
    let s = setup();
    let unavailable = Keychain::with_store(None, Duration::from_millis(500));
    let mut c = connected(&s, password_server()).await;
    answer_with(&s, vec![(true, Some(PASSWORD), true)]);
    login(
        &mut c,
        &who(LoginMethod::Password),
        &unavailable,
        &s.prompter,
    )
    .await
    .unwrap();
    match &prompts(&s.sink)[..] {
        [Prompt::Password { offer_save, .. }] => assert!(!*offer_save),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_failing_keychain_does_not_stop_a_login_that_works() {
    let s = setup();
    s.store
        .fail
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut c = connected(&s, password_server()).await;
    answer_with(&s, vec![(true, Some(PASSWORD), true)]);
    login(
        &mut c,
        &who(LoginMethod::Password),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap();
    assert!(!s.keychain.is_available());
}

#[tokio::test]
async fn a_key_file_logs_in_without_a_question() {
    let s = setup();
    let path = write_key(&s, "id_test", KEY_B);
    let mut c = connected(&s, key_server(KEY_B)).await;
    login(
        &mut c,
        &who(LoginMethod::Key { path }),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap();
    assert!(prompts(&s.sink).is_empty());
}

#[tokio::test]
async fn a_key_the_server_does_not_know_is_auth_failed() {
    let s = setup();
    let path = write_key(&s, "id_test", KEY_B);
    let mut c = connected(&s, key_server(KEY_A)).await;
    let err = login(
        &mut c,
        &who(LoginMethod::Key { path }),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthFailed);
}

#[tokio::test]
async fn a_missing_key_file_says_so_and_a_garbage_file_says_it_cannot_be_read() {
    let s = setup();
    let mut c = connected(&s, key_server(KEY_B)).await;
    let missing = s.dir.path().join("nope").display().to_string();
    let err = login(
        &mut c,
        &who(LoginMethod::Key { path: missing }),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);

    let garbage = write_key(&s, "garbage", "this is not a key");
    let err = login(
        &mut c,
        &who(LoginMethod::Key { path: garbage }),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

#[tokio::test]
async fn an_encrypted_key_asks_for_the_passphrase() {
    let s = setup();
    let path = write_key(&s, "id_enc", KEY_ENCRYPTED);
    let mut c = connected(&s, key_server(KEY_ENCRYPTED)).await;
    answer_with(&s, vec![(true, Some(ENCRYPTED_PASSPHRASE), false)]);
    login(
        &mut c,
        &who(LoginMethod::Key { path: path.clone() }),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap();
    match &prompts(&s.sink)[..] {
        [
            Prompt::Passphrase {
                key_path,
                offer_save,
            },
        ] => {
            assert_eq!(*key_path, path);
            assert!(*offer_save);
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_wrong_passphrase_is_auth_failed() {
    let s = setup();
    let path = write_key(&s, "id_enc", KEY_ENCRYPTED);
    let mut c = connected(&s, key_server(KEY_ENCRYPTED)).await;
    answer_with(&s, vec![(true, Some("not-it"), false)]);
    let err = login(
        &mut c,
        &who(LoginMethod::Key { path }),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthFailed);
}

#[tokio::test]
async fn a_saved_passphrase_is_used_without_asking_and_a_new_one_can_be_saved() {
    let s = setup();
    let path = write_key(&s, "id_enc", KEY_ENCRYPTED);

    answer_with(&s, vec![(true, Some(ENCRYPTED_PASSPHRASE), true)]);
    let mut c = connected(&s, key_server(KEY_ENCRYPTED)).await;
    login(
        &mut c,
        &who(LoginMethod::Key { path: path.clone() }),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap();
    assert_eq!(
        s.store
            .items
            .lock()
            .unwrap()
            .get("session:s1:passphrase")
            .map(String::as_str),
        Some(ENCRYPTED_PASSPHRASE)
    );

    let before = prompts(&s.sink).len();
    let mut again = connected(&s, key_server(KEY_ENCRYPTED)).await;
    login(
        &mut again,
        &who(LoginMethod::Key { path }),
        &s.keychain,
        &s.prompter,
    )
    .await
    .unwrap();
    assert_eq!(prompts(&s.sink).len(), before);
}

#[test]
fn a_key_path_starting_with_a_tilde_means_the_home_folder() {
    let expanded = super::auth::expand_home("~/.ssh/id_ed25519");
    assert!(!expanded.to_string_lossy().starts_with('~'));
    assert!(expanded.ends_with(".ssh/id_ed25519") || expanded.ends_with(".ssh\\id_ed25519"));
    assert_eq!(
        super::auth::expand_home("/etc/key"),
        std::path::PathBuf::from("/etc/key")
    );
}

/// Uses the Windows OpenSSH agent if it is running and holds a key; otherwise there is nothing
/// to test, and this passes without doing anything.
#[cfg(windows)]
#[tokio::test]
async fn the_windows_agent_logs_in_when_it_has_a_key() {
    use russh::keys::agent::AgentIdentity;
    use russh::keys::agent::client::AgentClient;

    let Ok(mut agent) = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await else {
        return;
    };
    let Ok(identities) = agent.request_identities().await else {
        return;
    };
    let keys: Vec<_> = identities
        .into_iter()
        .filter_map(|i| match i {
            AgentIdentity::PublicKey { key, .. } => Some(key),
            _ => None,
        })
        .collect();
    if keys.is_empty() {
        return;
    }
    let s = setup();
    let mut c = connected(
        &s,
        Accepts {
            user: USER.into(),
            password: None,
            keys,
            ..Default::default()
        },
    )
    .await;
    login(&mut c, &who(LoginMethod::Agent), &s.keychain, &s.prompter)
        .await
        .unwrap();
}
