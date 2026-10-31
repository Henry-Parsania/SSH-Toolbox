//! Library check: russh logs in with a password, a key and an agent, and russh-sftp lists a
//! folder on the same connection. Needs a test server, given by environment variables:
//!   SSH_TEST_HOST, SSH_TEST_PORT, SSH_TEST_USER (required)
//!   SSH_TEST_PASSWORD   -> password login, then SFTP listing on that same connection
//!   SSH_TEST_KEY        -> key login with the private key file at this path
//!   SSH_TEST_AGENT=1    -> agent login (Unix: SSH_AUTH_SOCK)
//! Host key checking is skipped here on purpose: this only talks to the throwaway test container.
//! The host key rules are checked separately in check_hostkeys.
use std::sync::Arc;

use russh::client::{self, Handle};
use russh::keys::agent::client::AgentClient;
use russh::keys::{PrivateKeyWithHashAlg, PublicKeyOrCertificate, load_secret_key};
use russh_sftp::client::SftpSession;

struct Client;

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

async fn connect(host: &str, port: u16) -> Handle<Client> {
    let config = Arc::new(client::Config::default());
    client::connect(config, (host, port), Client)
        .await
        .expect("connect")
}

#[tokio::main]
async fn main() {
    let host = std::env::var("SSH_TEST_HOST").expect("SSH_TEST_HOST");
    let port: u16 = std::env::var("SSH_TEST_PORT")
        .expect("SSH_TEST_PORT")
        .parse()
        .unwrap();
    let user = std::env::var("SSH_TEST_USER").expect("SSH_TEST_USER");
    let mut ran = 0;

    if let Ok(password) = std::env::var("SSH_TEST_PASSWORD") {
        let mut session = connect(&host, port).await;
        let result = session
            .authenticate_password(&user, password)
            .await
            .expect("auth");
        assert!(result.success(), "password login failed");
        println!("russh password login: OK");

        // SFTP on the same connection: a second channel, not a second login.
        let channel = session.channel_open_session().await.expect("channel");
        channel
            .request_subsystem(true, "sftp")
            .await
            .expect("sftp subsystem");
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .expect("sftp session");
        let home = sftp.canonicalize(".").await.expect("canonicalize");
        let entries: Vec<String> = sftp
            .read_dir(".")
            .await
            .expect("read_dir")
            .map(|e| e.file_name())
            .collect();
        println!(
            "russh-sftp OK on the same connection: home={home}, {} entries",
            entries.len()
        );
        ran += 1;
    }

    if let Ok(key_path) = std::env::var("SSH_TEST_KEY") {
        let key = load_secret_key(&key_path, None).expect("load key");
        let mut session = connect(&host, port).await;
        let hash = session
            .best_supported_rsa_hash()
            .await
            .expect("hash")
            .flatten();
        let result = session
            .authenticate_publickey(&user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
            .await
            .expect("auth");
        assert!(result.success(), "key login failed");
        println!("russh key login: OK");
        ran += 1;
    }

    if std::env::var("SSH_TEST_AGENT").is_ok() {
        #[cfg(unix)]
        {
            let mut agent = AgentClient::connect_env().await.expect("connect to agent");
            let identities = agent.request_identities().await.expect("identities");
            assert!(!identities.is_empty(), "agent has no keys");
            let mut session = connect(&host, port).await;
            let hash = session
                .best_supported_rsa_hash()
                .await
                .expect("hash")
                .flatten();
            let key = match &identities[0] {
                russh::keys::agent::AgentIdentity::PublicKey { key, .. } => key.clone(),
                _ => panic!("expected a plain public key in the agent"),
            };
            let result = session
                .authenticate_publickey_with(&user, key, hash, &mut agent)
                .await
                .expect("auth");
            assert!(result.success(), "agent login failed");
            println!(
                "russh agent login: OK ({} key(s) in agent)",
                identities.len()
            );
            ran += 1;
        }
        #[cfg(not(unix))]
        {
            let _ = AgentClient::connect_pageant;
            println!("agent login against a server is only checked on Linux; see check_win_agent");
        }
    }

    assert!(
        ran > 0,
        "set SSH_TEST_PASSWORD, SSH_TEST_KEY or SSH_TEST_AGENT"
    );
}
