//! The agent's server tools against an SSH server that runs inside the test.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::tools::{read_text_file, run_command, write_text_file};
use crate::error::ErrorCode;
use crate::files::SftpFiles;
use crate::prompt::{PromptBroker, Prompter};
use crate::ssh::connection::connect;
use crate::ssh::hostkeys::learn;
use crate::ssh::pool::SharedConnection;
use crate::ssh::testing::{Accepts, KEY_A, key, public_of, spawn_server};
use crate::terminal::testing::RecordingSink;

const USER: &str = "tester";
const PASSWORD: &str = "right-password";

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

/// A logged-in connection to a server that runs commands in `home` and serves it over SFTP.
async fn server(home: &std::path::Path, sh: String) -> Arc<SharedConnection> {
    let host_key = key(KEY_A);
    let server = spawn_server(
        host_key.clone(),
        Accepts {
            user: USER.into(),
            password: Some(PASSWORD.into()),
            home: Some(home.to_path_buf()),
            sh: Some(sh),
            sftp_root: Some(home.to_path_buf()),
            ..Default::default()
        },
    )
    .await;
    let port = server.port;
    std::mem::forget(server);
    let known = home.join("known_hosts_for_test");
    learn("127.0.0.1", port, &public_of(&host_key), &known).unwrap();
    let prompter = Prompter::new(
        PromptBroker::new(),
        Arc::new(RecordingSink::default()),
        CancellationToken::new(),
    );
    let mut connection = connect("127.0.0.1", port, known, prompter)
        .await
        .unwrap();
    assert!(
        connection
            .authenticate_password(USER, PASSWORD)
            .await
            .unwrap()
            .success()
    );
    SharedConnection::new(connection)
}

#[tokio::test]
async fn commands_follow_the_rules_and_their_output_loses_its_secrets() {
    let Some(sh) = find_sh() else { return };
    let home = tempfile::tempdir().unwrap();
    let shared = server(home.path(), sh).await;
    let connection = &shared.handle;

    // Looking is allowed without approval, and what comes back has no secrets in it.
    let out = run_command(connection, "echo hello", false).await.unwrap();
    assert_eq!((out.code, out.output.trim()), (Some(0), "hello"));
    let out = run_command(connection, "echo DB_PASSWORD=hunter2", false)
        .await
        .unwrap();
    assert!(out.output.contains("[hidden]") && !out.output.contains("hunter2"), "{}", out.output);

    // A change needs approval; with it, it runs.
    let made = home.path().join("made.txt");
    let err = run_command(connection, "touch \"$HOME/made.txt\"", false)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::PermissionDenied);
    assert!(!made.exists());
    run_command(connection, "touch \"$HOME/made.txt\"", true)
        .await
        .unwrap();
    assert!(made.exists());

    // A blocked command never runs, approved or not.
    let err = run_command(connection, "rm -rf / --no-preserve-root", true)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::PermissionDenied);
    assert!(made.exists());
}

#[tokio::test]
async fn files_are_written_after_approval_and_read_back_without_secrets() {
    let Some(sh) = find_sh() else { return };
    let home = tempfile::tempdir().unwrap();
    let shared = server(home.path(), sh).await;
    let files = SftpFiles::open(shared.clone()).await.unwrap();

    let path = "/app.conf";
    assert!(write_text_file(&files, path, "x", false).await.is_err());
    write_text_file(&files, path, "port=80\nTOKEN=abc123\n", true)
        .await
        .unwrap();
    let on_disk = std::fs::read_to_string(home.path().join("app.conf")).unwrap();
    assert!(on_disk.contains("abc123"), "the real file keeps its content");
    let seen = read_text_file(&files, path, false).await.unwrap();
    assert!(seen.contains("port=80") && !seen.contains("abc123"), "{seen}");
}
