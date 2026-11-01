//! Docker over SSH, against a server and a Docker engine that both run inside the test.
//!
//! No real Docker is used here. The same two methods are tried against a real engine by the
//! Linux CI job, which has Docker.

use std::sync::Arc;

use bollard::query_parameters::ListContainersOptionsBuilder;
use tokio_util::sync::CancellationToken;

use super::explain_failure;
use super::ssh::{Method, connect_ssh};
use crate::error::ErrorCode;
use crate::prompt::{PromptBroker, Prompter};
use crate::ssh::connection::connect;
use crate::ssh::hostkeys::learn;
use crate::ssh::pool::SharedConnection;
use crate::ssh::testing::{
    Accepts, DockerCli, DockerFake, KEY_A, fake_docker_engine, key, public_of, spawn_server,
};
use crate::terminal::testing::RecordingSink;

const USER: &str = "tester";
const PASSWORD: &str = "right-password";

/// A logged-in connection to a server with Docker set up as `cli` and `socket` say.
async fn server_with_docker(
    cli: DockerCli,
    socket: bool,
) -> (Arc<SharedConnection>, tempfile::TempDir) {
    let engine = fake_docker_engine().await;
    let host_key = key(KEY_A);
    let server = spawn_server(
        host_key.clone(),
        Accepts {
            user: USER.into(),
            password: Some(PASSWORD.into()),
            docker: Some(DockerFake {
                daemon: engine,
                cli,
                socket,
            }),
            ..Default::default()
        },
    )
    .await;
    let port = server.port;
    // The server lives as long as the test does.
    std::mem::forget(server);

    let dir = tempfile::tempdir().unwrap();
    let known_hosts = dir.path().join("known_hosts");
    learn("127.0.0.1", port, &public_of(&host_key), &known_hosts).unwrap();
    let prompter = Prompter::new(
        PromptBroker::new(),
        Arc::new(RecordingSink::default()),
        CancellationToken::new(),
    );
    let mut connection = connect("127.0.0.1", port, known_hosts, prompter)
        .await
        .unwrap();
    assert!(
        connection
            .authenticate_password(USER, PASSWORD)
            .await
            .unwrap()
            .success()
    );
    (SharedConnection::new(connection), dir)
}

#[tokio::test]
async fn the_docker_command_is_used_first() {
    let (shared, _dir) = server_with_docker(DockerCli::Works, true).await;
    let (docker, method) = connect_ssh(shared).await.unwrap();
    assert_eq!(method, Method::DialStdio);

    let containers = docker
        .list_containers(Some(
            ListContainersOptionsBuilder::default().all(true).build(),
        ))
        .await
        .unwrap();
    assert_eq!(containers.len(), 1);
    assert_eq!(
        containers[0].names.as_deref(),
        Some(&["/web".to_string()][..])
    );
}

#[tokio::test]
async fn the_socket_is_used_when_the_server_has_no_docker_command() {
    let (shared, _dir) = server_with_docker(DockerCli::Missing, true).await;
    let (docker, method) = connect_ssh(shared).await.unwrap();
    assert_eq!(method, Method::Socket);
    let containers = docker
        .list_containers(None::<bollard::query_parameters::ListContainersOptions>)
        .await
        .unwrap();
    assert_eq!(containers.len(), 1);
}

#[tokio::test]
async fn many_requests_in_a_row_each_get_their_own_channel() {
    let (shared, _dir) = server_with_docker(DockerCli::Works, false).await;
    let (docker, _) = connect_ssh(shared).await.unwrap();
    for _ in 0..5 {
        docker.ping().await.unwrap();
    }
    // Two at once, as a screen showing a list and a log would.
    let (a, b) = tokio::join!(docker.ping(), docker.version());
    a.unwrap();
    b.unwrap();
}

#[tokio::test]
async fn a_user_without_access_is_told_to_join_the_docker_group() {
    let (shared, _dir) = server_with_docker(DockerCli::PermissionDenied, false).await;
    let error = connect_ssh(shared).await.err().unwrap();
    assert_eq!(error.code, ErrorCode::PermissionDenied);
    assert!(error.message.contains("docker group"), "{}", error.message);
    assert!(error.message.contains("never uses sudo"));
}

#[tokio::test]
async fn a_server_without_docker_is_told_so() {
    let (shared, _dir) = server_with_docker(DockerCli::Missing, false).await;
    let error = connect_ssh(shared).await.err().unwrap();
    assert_eq!(error.code, ErrorCode::NotFound);
    assert!(error.message.contains("installed"), "{}", error.message);
}

#[tokio::test]
async fn an_engine_that_is_stopped_is_told_so() {
    let (shared, _dir) = server_with_docker(DockerCli::NotRunning, false).await;
    let error = connect_ssh(shared).await.err().unwrap();
    assert!(
        error.message.contains("engine isn't running"),
        "{}",
        error.message
    );
}

#[test]
fn what_the_server_printed_is_never_shown() {
    let hostile = "permission denied \u{1b}[31m password=hunter2";
    let error = explain_failure(Some(1), hostile);
    assert!(!error.message.contains("hunter2"));
    assert!(!error.message.contains('\u{1b}'));
}
