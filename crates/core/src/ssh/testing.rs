//! A small SSH server that runs inside the tests, so they need no container.
//!
//! The keys below are throwaway keys made only for these tests. They open nothing anywhere.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use russh::keys::{PrivateKey, PublicKey};
use russh::server::{self, Auth, Server as _, VerifiedPublicKey};
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

#[derive(Clone)]
struct TestServer(Arc<Accepts>);

impl server::Server for TestServer {
    type Handler = TestHandler;
    fn new_client(&mut self, _peer: Option<std::net::SocketAddr>) -> TestHandler {
        TestHandler(self.0.clone())
    }
}

pub struct TestHandler(Arc<Accepts>);

impl server::Handler for TestHandler {
    type Error = russh::Error;

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

/// Starts a server that offers `host_key` and lets in only what `accepts` says. Returns the
/// port it listens on.
pub async fn start_server_with(host_key: PrivateKey, accepts: Accepts) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let config = Arc::new(server::Config {
        keys: vec![host_key],
        auth_rejection_time: std::time::Duration::from_millis(1),
        auth_rejection_time_initial: Some(std::time::Duration::from_millis(1)),
        ..Default::default()
    });
    let mut server = TestServer(Arc::new(accepts));
    tokio::spawn(async move {
        loop {
            let Ok((stream, peer)) = listener.accept().await else {
                return;
            };
            let handler = server.new_client(Some(peer));
            let config = config.clone();
            tokio::spawn(async move {
                if let Ok(session) = server::run_stream(config, stream, handler).await {
                    let _ = session.await;
                }
            });
        }
    });
    port
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
