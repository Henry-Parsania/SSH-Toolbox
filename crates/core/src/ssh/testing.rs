//! A small SSH server that runs inside the tests, so they need no container.
//!
//! The keys below are throwaway keys made only for these tests. They open nothing anywhere.

use std::sync::Arc;

use russh::keys::PrivateKey;
use russh::server::{self, Server as _};
use tokio::net::TcpListener;

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

pub fn key(pem: &str) -> PrivateKey {
    PrivateKey::from_openssh(pem).expect("test key")
}

#[derive(Clone)]
struct TestServer;

impl server::Server for TestServer {
    type Handler = TestHandler;
    fn new_client(&mut self, _peer: Option<std::net::SocketAddr>) -> TestHandler {
        TestHandler
    }
}

pub struct TestHandler;

impl server::Handler for TestHandler {
    type Error = russh::Error;
}

/// Starts a server that offers `host_key`, and returns the port it listens on.
pub async fn start_server(host_key: PrivateKey) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let config = Arc::new(server::Config {
        keys: vec![host_key],
        ..Default::default()
    });
    tokio::spawn(async move {
        let mut server = TestServer;
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
