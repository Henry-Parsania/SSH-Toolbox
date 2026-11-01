//! Which saved servers can be reached right now: a quick TCP connection to each one's port.
//! (A real ping needs special rights on most systems, and a server that answers pings may still
//! have its SSH port closed, so this tests what matters.) Nothing is sent over the connection.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio::sync::Semaphore;

/// How long to wait for one server before calling it unreachable.
pub const TIMEOUT: Duration = Duration::from_millis(2500);
/// At most this many are tried at once, so a long list doesn't open hundreds of sockets.
const AT_ONCE: usize = 24;
/// The most servers checked in one request.
pub const MAX_TARGETS: usize = 500;

#[derive(Debug, Clone, Deserialize)]
pub struct Target {
    /// The saved session's ID, handed back with the result.
    pub id: String,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Reach {
    pub id: String,
    pub reachable: bool,
    /// How long the connection took, when it worked.
    pub millis: Option<u32>,
}

async fn check_one(target: Target, timeout: Duration) -> Reach {
    let started = Instant::now();
    let connected = tokio::time::timeout(
        timeout,
        TcpStream::connect((target.host.as_str(), target.port)),
    )
    .await;
    match connected {
        Ok(Ok(_)) => Reach {
            id: target.id,
            reachable: true,
            millis: Some(u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX)),
        },
        _ => Reach {
            id: target.id,
            reachable: false,
            millis: None,
        },
    }
}

/// Checks all of them at the same time (a few at once), and returns the answers in the order
/// the servers were given.
pub async fn check_all(targets: Vec<Target>, timeout: Duration) -> Vec<Reach> {
    let permits = Arc::new(Semaphore::new(AT_ONCE));
    let tasks: Vec<_> = targets
        .into_iter()
        .take(MAX_TARGETS)
        .map(|target| {
            let permits = permits.clone();
            tokio::spawn(async move {
                let _permit = permits.acquire().await;
                check_one(target, timeout).await
            })
        })
        .collect();
    let mut results = Vec::with_capacity(tasks.len());
    for task in tasks {
        if let Ok(result) = task.await {
            results.push(result);
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    fn target(id: &str, port: u16) -> Target {
        Target {
            id: id.into(),
            host: "127.0.0.1".into(),
            port,
        }
    }

    #[tokio::test]
    async fn a_listening_port_is_reachable_and_a_closed_one_is_not() {
        let open = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let open_port = open.local_addr().unwrap().port();
        let closed_port = {
            let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
            l.local_addr().unwrap().port()
        };
        let results = check_all(
            vec![target("up", open_port), target("down", closed_port)],
            Duration::from_secs(2),
        )
        .await;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "up");
        assert!(results[0].reachable);
        assert!(results[0].millis.is_some());
        assert_eq!(results[1].id, "down");
        assert!(!results[1].reachable);
        assert_eq!(results[1].millis, None);
    }

    #[tokio::test]
    async fn an_unknown_host_name_is_unreachable_not_an_error() {
        let results = check_all(
            vec![Target {
                id: "x".into(),
                host: "no-such-host.invalid".into(),
                port: 22,
            }],
            Duration::from_secs(2),
        )
        .await;
        assert!(!results[0].reachable);
    }

    #[tokio::test]
    async fn a_long_list_is_cut_off_instead_of_swamping_the_computer() {
        let many = (0..MAX_TARGETS + 20)
            .map(|i| target(&i.to_string(), 1))
            .collect();
        let results = check_all(many, Duration::from_millis(200)).await;
        assert_eq!(results.len(), MAX_TARGETS);
    }
}
