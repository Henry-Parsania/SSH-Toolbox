//! The scan itself: a plain TCP connect to each port of each host. A connection that is accepted
//! means the port is open; one that is refused means a computer is there but the port is closed;
//! no answer at all means nothing is known. Nothing is sent to the ports, so a scan only does what
//! any program that tries to connect would do.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::{ports, target};
use crate::error::{AppError, ErrorCode};

/// Connections attempted at the same time.
const CONCURRENCY: usize = 256;
/// Most connections one scan may try in all.
pub const MAX_PROBES: usize = 300_000;
const MIN_TIMEOUT_MS: u64 = 150;
const MAX_TIMEOUT_MS: u64 = 5_000;
/// How long a port that was found open is listened to for a greeting.
const BANNER_WAIT: Duration = Duration::from_millis(500);

fn default_timeout() -> u64 {
    800
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScanRequest {
    /// Addresses, networks, ranges or names; see `target`.
    pub targets: String,
    /// Ports; see `ports::parse`.
    pub ports: String,
    /// How long to wait for each connection, in milliseconds.
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    /// Listen briefly on each open port for the greeting some services send (SSH, FTP, SMTP...).
    #[serde(default)]
    pub banners: bool,
    /// Report only hosts that answered (a network scan), instead of every host asked about.
    #[serde(default)]
    pub only_alive: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OpenPort {
    pub port: u16,
    pub service: Option<String>,
    pub banner: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HostResult {
    pub ip: String,
    /// Something answered: a port was open, or a connection was actively refused.
    pub alive: bool,
    pub open: Vec<OpenPort>,
    /// How many ports answered "closed".
    pub closed: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScanEvent {
    Started { hosts: usize, ports: usize },
    Host(HostResult),
    Progress { done: usize, total: usize },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Summary {
    pub hosts: usize,
    pub up: usize,
    pub open_ports: usize,
    pub cancelled: bool,
}

enum Probe {
    Open(Option<String>),
    Closed,
    Silent,
}

/// Cleans a greeting for display: the first line, printable characters only.
fn tidy(bytes: &[u8]) -> Option<String> {
    let text: String = String::from_utf8_lossy(bytes)
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control() && *c != '\u{fffd}')
        .take(100)
        .collect();
    (text.trim().chars().count() >= 3).then(|| text.trim().to_string())
}

async fn probe(
    addr: SocketAddr,
    timeout: Duration,
    banner: bool,
    cancel: &CancellationToken,
) -> Probe {
    let connect = tokio::time::timeout(timeout, TcpStream::connect(addr));
    let connected = tokio::select! {
        _ = cancel.cancelled() => return Probe::Silent,
        c = connect => c,
    };
    match connected {
        Ok(Ok(mut stream)) => {
            let mut greeting = None;
            if banner {
                let mut buffer = [0u8; 200];
                let wait = BANNER_WAIT.min(timeout.max(Duration::from_millis(200)));
                if let Ok(Ok(n)) = tokio::time::timeout(wait, stream.read(&mut buffer)).await {
                    greeting = tidy(&buffer[..n]);
                }
            }
            Probe::Open(greeting)
        }
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => Probe::Closed,
        _ => Probe::Silent,
    }
}

/// Runs a scan. `emit` is told about each host as it finishes and about the progress.
pub async fn run(
    request: &ScanRequest,
    cancel: CancellationToken,
    emit: impl Fn(ScanEvent) + Send + Sync + 'static,
) -> Result<Summary, AppError> {
    let hosts = target::resolve(&request.targets).await?;
    let ports = ports::parse(&request.ports)?;
    if hosts.len() * ports.len() > MAX_PROBES {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "That is {} connections to try; the most is {MAX_PROBES}. Use fewer hosts or fewer ports.",
                hosts.len() * ports.len()
            ),
        ));
    }
    let timeout = Duration::from_millis(request.timeout_ms.clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS));
    let total = hosts.len() * ports.len();
    emit(ScanEvent::Started {
        hosts: hosts.len(),
        ports: ports.len(),
    });

    let emit = Arc::new(emit);
    let permits = Arc::new(Semaphore::new(CONCURRENCY));
    let done = Arc::new(AtomicUsize::new(0));
    let last_progress = Arc::new(std::sync::Mutex::new(Instant::now()));
    let mut set: JoinSet<HostResult> = JoinSet::new();

    for ip in hosts.iter().copied() {
        let (ports, permits, done, cancel, emit, last_progress) = (
            ports.clone(),
            permits.clone(),
            done.clone(),
            cancel.clone(),
            emit.clone(),
            last_progress.clone(),
        );
        let banners = request.banners;
        set.spawn(async move {
            let mut probes: JoinSet<(u16, Probe)> = JoinSet::new();
            for port in ports {
                let (permits, cancel) = (permits.clone(), cancel.clone());
                probes.spawn(async move {
                    let Ok(_permit) = permits.acquire_owned().await else {
                        return (port, Probe::Silent);
                    };
                    (
                        port,
                        probe(SocketAddr::new(ip, port), timeout, banners, &cancel).await,
                    )
                });
            }
            let mut open = Vec::new();
            let mut closed = 0;
            while let Some(Ok((port, result))) = probes.join_next().await {
                match result {
                    Probe::Open(banner) => open.push(OpenPort {
                        port,
                        service: ports::service_name(port).map(str::to_string),
                        banner,
                    }),
                    Probe::Closed => closed += 1,
                    Probe::Silent => {}
                }
                let finished = done.fetch_add(1, Ordering::Relaxed) + 1;
                let mut last = last_progress.lock().unwrap();
                if last.elapsed() >= Duration::from_millis(150) {
                    *last = Instant::now();
                    emit(ScanEvent::Progress {
                        done: finished,
                        total,
                    });
                }
            }
            open.sort_by_key(|p| p.port);
            HostResult {
                ip: ip.to_string(),
                alive: !open.is_empty() || closed > 0,
                open,
                closed,
            }
        });
    }

    let mut summary = Summary {
        hosts: hosts.len(),
        up: 0,
        open_ports: 0,
        cancelled: false,
    };
    while let Some(Ok(result)) = set.join_next().await {
        summary.up += usize::from(result.alive);
        summary.open_ports += result.open.len();
        if result.alive || !request.only_alive {
            emit(ScanEvent::Host(result));
        }
    }
    summary.cancelled = cancel.is_cancelled();
    emit(ScanEvent::Progress { done: total, total });
    Ok(summary)
}

/// The networks this computer is on, as `192.168.1.0/24`, for suggesting what to scan. Only
/// private IPv4 networks are listed.
pub fn local_networks() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for data in sysinfo::Networks::new_with_refreshed_list().values() {
        for net in data.ip_networks() {
            let IpAddr::V4(ip) = net.addr else { continue };
            if !ip.is_private() || ip.is_loopback() || ip.is_link_local() {
                continue;
            }
            // A bigger network than a /24 is rarely meant to be scanned whole; offer the /24 around us.
            let prefix = u32::from(net.prefix).clamp(24, 32);
            let mask = u32::MAX << (32 - prefix);
            let network = std::net::Ipv4Addr::from(u32::from(ip) & mask);
            let text = format!("{network}/{prefix}");
            if !out.contains(&text) {
                out.push(text);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    fn request(targets: &str, ports: &str) -> ScanRequest {
        ScanRequest {
            targets: targets.into(),
            ports: ports.into(),
            timeout_ms: 500,
            banners: false,
            only_alive: false,
        }
    }

    async fn collect(
        request: ScanRequest,
        cancel: CancellationToken,
    ) -> (Summary, Vec<HostResult>) {
        let hosts = Arc::new(Mutex::new(Vec::new()));
        let sink = hosts.clone();
        let summary = run(&request, cancel, move |e| {
            if let ScanEvent::Host(h) = e {
                sink.lock().unwrap().push(h);
            }
        })
        .await
        .unwrap();
        let hosts = hosts.lock().unwrap().clone();
        (summary, hosts)
    }

    #[tokio::test]
    async fn an_open_port_is_found_and_a_closed_one_shows_the_host_is_there() {
        let open = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let open_port = open.local_addr().unwrap().port();
        // A port that was just free and has nothing on it.
        let closed_port = {
            let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
            l.local_addr().unwrap().port()
        };
        let (summary, hosts) = collect(
            request("127.0.0.1", &format!("{open_port},{closed_port}")),
            CancellationToken::new(),
        )
        .await;
        assert_eq!((summary.hosts, summary.up, summary.open_ports), (1, 1, 1));
        assert_eq!(
            hosts[0].open.iter().map(|p| p.port).collect::<Vec<_>>(),
            [open_port]
        );
        // Linux answers a closed port at once. Windows takes about two seconds to give up on one,
        // longer than the wait used here, so there it simply counts as no answer.
        if !cfg!(windows) {
            assert_eq!(hosts[0].closed, 1);
        }
        assert!(hosts[0].alive);
    }

    #[tokio::test]
    async fn greetings_are_read_when_asked_and_cleaned() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = listener.accept().await {
                let _ = s.write_all(b"SSH-2.0-OpenSSH_9.6\x1b[31m\r\nrest").await;
            }
        });
        let mut req = request("127.0.0.1", &port.to_string());
        req.banners = true;
        let (_, hosts) = collect(req, CancellationToken::new()).await;
        assert_eq!(
            hosts[0].open[0].banner.as_deref(),
            Some("SSH-2.0-OpenSSH_9.6[31m")
        );
        // Without the option nothing is read.
        let (_, hosts) = collect(
            request("127.0.0.1", &port.to_string()),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(hosts[0].open[0].banner, None);
    }

    #[tokio::test]
    async fn a_network_scan_reports_only_hosts_that_answer() {
        let open = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = open.local_addr().unwrap().port();
        let mut req = request("127.0.0.1", &port.to_string());
        req.only_alive = true;
        let (summary, hosts) = collect(req, CancellationToken::new()).await;
        assert_eq!((summary.up, hosts.len()), (1, 1));
    }

    #[tokio::test]
    async fn a_stopped_scan_ends_and_says_so() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let (summary, _) = collect(request("127.0.0.1", "1-2000"), cancel).await;
        assert!(summary.cancelled);
    }

    #[tokio::test]
    async fn too_big_a_scan_is_refused_before_it_starts() {
        let err = run(
            &request("10.0.0.0/20", "1-200"),
            CancellationToken::new(),
            |_| {},
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput);
        assert!(
            run(
                &request("nonsense!", "80"),
                CancellationToken::new(),
                |_| {}
            )
            .await
            .is_err()
        );
        assert!(
            run(
                &request("127.0.0.1", "99999"),
                CancellationToken::new(),
                |_| {}
            )
            .await
            .is_err()
        );
    }

    #[test]
    fn greetings_lose_control_characters_and_junk() {
        assert_eq!(
            tidy(b"220 mail ready\r\nmore").as_deref(),
            Some("220 mail ready")
        );
        assert_eq!(tidy(&[0, 1, 2, 0xff, 0xfe]), None);
        assert_eq!(tidy(b"ab"), None);
    }

    #[test]
    fn local_networks_are_private_and_look_like_networks() {
        for n in local_networks() {
            let (addr, prefix) = n.split_once('/').unwrap();
            assert!(addr.parse::<std::net::Ipv4Addr>().unwrap().is_private());
            assert!((24..=32).contains(&prefix.parse::<u32>().unwrap()));
        }
    }
}
