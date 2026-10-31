//! A small SSH server that runs inside the tests, so they need no container.
//!
//! The keys below are throwaway keys made only for these tests. They open nothing anywhere.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use russh::keys::{PrivateKey, PublicKey};
use russh::server::{self, Auth, Msg, Server as _, Session, VerifiedPublicKey};
use russh::{Channel, ChannelId};
use tokio::net::TcpListener;

use crate::keychain::SecretStore;
use crate::secret::Secret;

pub const KEY_A: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACAkIYnPv+yrQ1FB9mG+4STYvx04QJsVP5PgPNZeeGdjoAAAAJB7awaDe2sG
gwAAAAtzc2gtZWQyNTUxOQAAACAkIYnPv+yrQ1FB9mG+4STYvx04QJsVP5PgPNZeeGdjoA
AAAECbsdHWlevSN4Q0x5RPgwi+SKvHwBkB1yoW5BOtsoFFJyQhic+/7KtDUUH2Yb7hJNi/
HThAmxU/k+A81l54Z2OgAAAACXRlc3Qtb25seQECAwQ=
-----END OPENSSH PRIVATE KEY-----
";

pub const KEY_B: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACBSSIBGbq9Ea3kZLmgDq7TRI3Nmed1KL5dyoZEqu576dAAAAJBpYfIpaWHy
KQAAAAtzc2gtZWQyNTUxOQAAACBSSIBGbq9Ea3kZLmgDq7TRI3Nmed1KL5dyoZEqu576dA
AAAEAcrtZoUmFTkuXOuxLBlU0DUhjZLoWfiIEbm1WBwMDR7VJIgEZur0RreRkuaAOrtNEj
c2Z53Uovl3KhkSq7nvp0AAAACXRlc3Qtb25seQECAwQ=
-----END OPENSSH PRIVATE KEY-----
";

/// A key protected by `ENCRYPTED_PASSPHRASE`.
pub const KEY_ENCRYPTED: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAACmFlczI1Ni1jdHIAAAAGYmNyeXB0AAAAGAAAABCK+HBloL
T+MQJqmKiLt+PvAAAAGAAAAAEAAAAzAAAAC3NzaC1lZDI1NTE5AAAAIEmxytWuUUpVk5JX
p5D1xZRhI7KpGr5/UXt6hhVC6prsAAAAkC7wQvu0+rNi25lgPP4QbBzjd8aV2nDupUnXQb
Jth/AiDnefb6vTL4WP67ndP3j4k2KrN8lILryxNg6E4EgiGtUi2HiVC8mcx0YB27D9mh/A
AZSZZDkhsU50JL13mTYOULU/HhbIOMVvy3ZH4iX/gNhhAe4MRu7xIxm2oMeGSRM2QVkZdk
iPZqP9H5zTDducOw==
-----END OPENSSH PRIVATE KEY-----
";
pub const ENCRYPTED_PASSPHRASE: &str = "test-passphrase";

pub fn key(pem: &str) -> PrivateKey {
    PrivateKey::from_openssh(pem).expect("test key")
}

/// A public key as a server sends it, without the comment a key file carries.
pub fn public_of(private: &PrivateKey) -> PublicKey {
    PublicKey::from(private.public_key().key_data().clone())
}

/// What the test server lets in.
#[derive(Default)]
pub struct Accepts {
    pub user: String,
    pub password: Option<String>,
    pub keys: Vec<PublicKey>,
}

/// What happened on the server, for tests to look at.
#[derive(Default)]
pub struct Stats {
    /// Connections that were opened.
    pub opened: AtomicUsize,
    /// Connections that have ended.
    pub ended: AtomicUsize,
    /// Terminal sizes asked for: first the one in the pty request, then each window change.
    pub sizes: Mutex<Vec<(u32, u32)>>,
    pub shells: AtomicUsize,
}

#[derive(Clone)]
struct TestServer(Arc<Accepts>, Arc<Stats>);

impl server::Server for TestServer {
    type Handler = TestHandler;
    fn new_client(&mut self, _peer: Option<std::net::SocketAddr>) -> TestHandler {
        TestHandler(self.0.clone(), self.1.clone())
    }
}

pub struct TestHandler(Arc<Accepts>, Arc<Stats>);

impl server::Handler for TestHandler {
    type Error = russh::Error;

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: russh::server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        cols: u32,
        rows: u32,
        _px: u32,
        _py: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.1.sizes.lock().unwrap().push((cols, rows));
        session.channel_success(channel)?;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        _channel: ChannelId,
        cols: u32,
        rows: u32,
        _px: u32,
        _py: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.1.sizes.lock().unwrap().push((cols, rows));
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.1.shells.fetch_add(1, Ordering::SeqCst);
        session.channel_success(channel)?;
        session.data(channel, b"welcome-banner".to_vec())?;
        Ok(())
    }

    /// Echoes what it is sent. "exit-now" ends the shell with exit code 7 when it gets "exit-now".
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if data == b"exit-now" {
            session.exit_status_request(channel, 7)?;
            session.eof(channel)?;
            session.close(channel)?;
        } else {
            session.data(channel, data.to_vec())?;
        }
        Ok(())
    }

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        let ok = user == self.0.user && self.0.password.as_deref() == Some(password);
        Ok(if ok { Auth::Accept } else { Auth::reject() })
    }

    async fn auth_publickey(
        &mut self,
        user: &str,
        key: VerifiedPublicKey<'_>,
    ) -> Result<Auth, Self::Error> {
        let ok = user == self.0.user && self.0.keys.iter().any(|k| k == key.public_key);
        Ok(if ok { Auth::Accept } else { Auth::reject() })
    }
}

/// A running test server. Dropping it, or calling `kill`, cuts every connection at once, the way
/// a crashed server would.
pub struct ServerHandle {
    pub port: u16,
    pub stats: Arc<Stats>,
    cut: tokio_util::sync::CancellationToken,
}

impl ServerHandle {
    /// Cuts every connection and stops listening.
    pub fn kill(&self) {
        self.cut.cancel();
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Starts a server that offers `host_key` and lets in only what `accepts` says. Returns the
/// port it listens on.
pub async fn start_server_with(host_key: PrivateKey, accepts: Accepts) -> u16 {
    let handle = spawn_server(host_key, accepts).await;
    let port = handle.port;
    // Tests that only need the port leave the server running until the test ends.
    std::mem::forget(handle);
    port
}

pub async fn spawn_server(host_key: PrivateKey, accepts: Accepts) -> ServerHandle {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let config = Arc::new(server::Config {
        keys: vec![host_key],
        auth_rejection_time: std::time::Duration::from_millis(1),
        auth_rejection_time_initial: Some(std::time::Duration::from_millis(1)),
        ..Default::default()
    });
    let stats = Arc::new(Stats::default());
    let cut = tokio_util::sync::CancellationToken::new();
    let mut server = TestServer(Arc::new(accepts), stats.clone());
    {
        let cut = cut.clone();
        let stats = stats.clone();
        tokio::spawn(async move {
            loop {
                let accepted = tokio::select! {
                    accepted = listener.accept() => accepted,
                    _ = cut.cancelled() => return,
                };
                let Ok((mut stream, peer)) = accepted else {
                    return;
                };
                stats.opened.fetch_add(1, Ordering::SeqCst);
                let handler = server.new_client(Some(peer));
                let config = config.clone();
                let stats = stats.clone();
                // The SSH server talks over an in-memory pipe, and this task joins the pipe to
                // the real socket. Cutting the server closes the socket at once, the way a
                // crashed machine would, which stopping the SSH tasks alone would not.
                let (ssh_side, mut pipe_side) = tokio::io::duplex(64 * 1024);
                let cut = cut.clone();
                tokio::spawn(async move {
                    tokio::select! {
                        _ = tokio::io::copy_bidirectional(&mut stream, &mut pipe_side) => {}
                        _ = cut.cancelled() => {}
                    }
                });
                tokio::spawn(async move {
                    if let Ok(session) = server::run_stream(config, ssh_side, handler).await {
                        let _ = session.await;
                    }
                    stats.ended.fetch_add(1, Ordering::SeqCst);
                });
            }
        });
    }
    ServerHandle { port, stats, cut }
}

/// A server for tests that only look at the host key.
pub async fn start_server(host_key: PrivateKey) -> u16 {
    start_server_with(host_key, Accepts::default()).await
}

/// A keychain in memory that can be told to fail.
#[derive(Default)]
pub struct MemoryStore {
    pub items: Mutex<HashMap<String, String>>,
    pub fail: std::sync::atomic::AtomicBool,
}

impl SecretStore for MemoryStore {
    fn set(&self, account: &str, secret: &Secret) -> Result<(), String> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("keychain is down".into());
        }
        self.items
            .lock()
            .unwrap()
            .insert(account.into(), secret.expose().into());
        Ok(())
    }
    fn get(&self, account: &str) -> Result<Option<Secret>, String> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("keychain is down".into());
        }
        Ok(self.items.lock().unwrap().get(account).map(Secret::new))
    }
    fn delete(&self, account: &str) -> Result<(), String> {
        self.items.lock().unwrap().remove(account);
        Ok(())
    }
}
