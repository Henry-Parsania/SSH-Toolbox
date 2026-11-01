//! Library check: the app's Docker-over-SSH transport against a real server and a real Docker.
//! Needs the test SSH container with the Docker command line and the host's Docker socket, given
//! by environment variables: SSH_TEST_HOST, SSH_TEST_PORT, SSH_TEST_USER, SSH_TEST_PASSWORD, and
//! SSH_TEST_KNOWN_HOSTS (a known_hosts file holding the server's key; host keys are always checked).
use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use toolbox_core::docker::ssh::{Method, connect_ssh};
use toolbox_core::prompt::{PromptBroker, Prompter};
use toolbox_core::ssh::connection::connect;
use toolbox_core::ssh::pool::SharedConnection;
use toolbox_core::terminal::{OutputSink, TerminalStatus};

/// Nothing here should ask a question: the host key is already known and the password is given.
struct Silent;

impl OutputSink for Silent {
    fn send_data(&self, _chunk: Vec<u8>) -> Result<(), toolbox_core::error::AppError> {
        Ok(())
    }
    fn send_status(&self, status: TerminalStatus) -> Result<(), toolbox_core::error::AppError> {
        if matches!(status, TerminalStatus::Prompt { .. }) {
            panic!("the check was asked a question");
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"));
    let port: u16 = var("SSH_TEST_PORT").parse().unwrap();
    let prompter = Prompter::new(
        PromptBroker::new(),
        Arc::new(Silent),
        CancellationToken::new(),
    );
    let mut connection = connect(
        &var("SSH_TEST_HOST"),
        port,
        var("SSH_TEST_KNOWN_HOSTS").into(),
        prompter,
    )
    .await
    .expect("connect");
    let accepted = connection
        .authenticate_password(var("SSH_TEST_USER"), var("SSH_TEST_PASSWORD"))
        .await
        .expect("login");
    assert!(accepted.success(), "password login failed");

    let (docker, method) = connect_ssh(SharedConnection::new(connection))
        .await
        .expect("docker over ssh");
    assert_eq!(
        method,
        Method::DialStdio,
        "the docker command should be used first"
    );
    let containers = docker
        .list_containers(Some(
            bollard::query_parameters::ListContainersOptionsBuilder::default()
                .all(true)
                .build(),
        ))
        .await
        .expect("list containers");
    let found = containers.iter().any(|c| {
        c.names
            .as_ref()
            .is_some_and(|n| n.iter().any(|n| n == "/sshd"))
    });
    assert!(found, "the test server's own container should be listed");
    println!(
        "docker over ssh OK: {} container(s), method {method:?}",
        containers.len()
    );
}
