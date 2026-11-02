//! A tiny stand-in for the Docker engine's HTTP interface, for tests: enough to list, change and
//! inspect two containers, stream a log and run a shell. It records every request it gets.

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// The requests the engine has received, as "METHOD /path?query".
pub type Requests = Arc<Mutex<Vec<String>>>;

pub const CONTAINERS: &str = r#"[
  {"Id":"abc123","Names":["/web"],"Image":"nginx","State":"running","Status":"Up 2 hours","Created":200,
   "Ports":[{"IP":"0.0.0.0","PrivatePort":80,"PublicPort":8080,"Type":"tcp"}],
   "Labels":{"com.docker.compose.project":"shop","com.docker.compose.service":"web"}},
  {"Id":"def456","Names":["/old\u001b[31m"],"Image":"redis","State":"exited","Status":"Exited (0) a day ago","Created":100,
   "Ports":[],"Labels":{}}
]"#;

pub const INSPECT: &str = r#"{"Id":"abc123","Name":"/web","Image":"sha256:1","State":{"Status":"running"},
  "Config":{"Tty":false,"Env":["MODE=test","TOKEN=abc"]},
  "Mounts":[{"Type":"bind","Source":"/srv/site","Destination":"/usr/share/nginx/html","RW":false}],
  "NetworkSettings":{"Ports":{"80/tcp":[{"HostIp":"0.0.0.0","HostPort":"8080"}],"443/tcp":null},
    "Networks":{"shop_default":{"IPAddress":"172.18.0.2"}}}}"#;

const CRLF: &str = "\r\n";

/// Starts the engine. Returns the port it listens on and a record of what it was asked.
pub async fn fake_docker_engine() -> (u16, Requests) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Requests::default();
    let record = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                return;
            };
            let record = record.clone();
            tokio::spawn(serve(socket, record));
        }
    });
    (port, seen)
}

fn reply(status: &str, kind: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}{CRLF}Api-Version: 1.46{CRLF}Content-Type: {kind}{CRLF}Content-Length: {}{CRLF}{CRLF}{body}",
        body.len()
    )
}

/// The log: a frame of standard output and one of standard error, then the stream stays open
/// like a followed log until the client goes away.
async fn stream_log(socket: &mut TcpStream) {
    let mut frames = Vec::new();
    for (stream, text) in [(1u8, "hello\n"), (2u8, "oops\n")] {
        frames.extend_from_slice(&[stream, 0, 0, 0]);
        frames.extend_from_slice(&(text.len() as u32).to_be_bytes());
        frames.extend_from_slice(text.as_bytes());
    }
    let head = format!(
        "HTTP/1.1 200 OK{CRLF}Content-Type: application/vnd.docker.multiplexed-stream{CRLF}Transfer-Encoding: chunked{CRLF}{CRLF}{:x}{CRLF}",
        frames.len()
    );
    let mut out = head.into_bytes();
    out.extend_from_slice(&frames);
    out.extend_from_slice(CRLF.as_bytes());
    if socket.write_all(&out).await.is_err() {
        return;
    }
    let mut chunk = [0u8; 1024];
    while matches!(socket.read(&mut chunk).await, Ok(n) if n > 0) {}
}

/// An upgraded connection: a prompt, then everything typed comes back with "echo:" in front.
async fn run_shell(socket: &mut TcpStream) {
    let head = format!(
        "HTTP/1.1 101 UPGRADED{CRLF}Content-Type: application/vnd.docker.raw-stream{CRLF}Connection: Upgrade{CRLF}Upgrade: tcp{CRLF}{CRLF}$ "
    );
    if socket.write_all(head.as_bytes()).await.is_err() {
        return;
    }
    let mut chunk = [0u8; 4096];
    loop {
        match socket.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                let mut back = b"echo:".to_vec();
                back.extend_from_slice(&chunk[..n]);
                if socket.write_all(&back).await.is_err() {
                    return;
                }
            }
        }
    }
}

async fn serve(mut socket: TcpStream, record: Requests) {
    let mut pending = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let end = loop {
            if let Some(at) = pending.windows(4).position(|w| w == b"\r\n\r\n") {
                break at + 4;
            }
            match socket.read(&mut chunk).await {
                Ok(0) | Err(_) => return,
                Ok(n) => pending.extend_from_slice(&chunk[..n]),
            }
        };
        let head = String::from_utf8_lossy(&pending[..end]).into_owned();
        pending.drain(..end);
        let mut words = head.split_whitespace();
        let method = words.next().unwrap_or("").to_string();
        let target = words.next().unwrap_or("").to_string();
        let path = target.split('?').next().unwrap_or("").to_string();
        record.lock().unwrap().push(format!("{method} {target}"));
        // Take the request's body off the connection so it isn't mistaken for the next request.
        let length = head
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .and_then(|v| v.trim().parse::<usize>().ok())
            })
            .unwrap_or(0);
        while pending.len() < length {
            match socket.read(&mut chunk).await {
                Ok(0) | Err(_) => return,
                Ok(n) => pending.extend_from_slice(&chunk[..n]),
            }
        }
        let body: Vec<u8> = pending.drain(..length).collect();
        if path.ends_with("/containers/create") {
            record
                .lock()
                .unwrap()
                .push(format!("BODY {}", String::from_utf8_lossy(&body)));
        }

        if path.ends_with("/abc123/logs") {
            return stream_log(&mut socket).await;
        }
        if path.ends_with("/exec/exec1/start") {
            return run_shell(&mut socket).await;
        }

        let answer = if path.ends_with("/_ping") {
            reply("200 OK", "text/plain", "OK")
        } else if path.ends_with("/version") {
            reply(
                "200 OK",
                "application/json",
                r#"{"Version":"27.0.0","ApiVersion":"1.46","MinAPIVersion":"1.24"}"#,
            )
        } else if path.ends_with("/containers/json") {
            reply("200 OK", "application/json", CONTAINERS)
        } else if path.ends_with("/abc123/json") {
            reply("200 OK", "application/json", INSPECT)
        } else if method == "POST" && path.ends_with("/abc123/exec") {
            reply("201 Created", "application/json", r#"{"Id":"exec1"}"#)
        } else if path.ends_with("/exec/exec1/resize") {
            reply("200 OK", "text/plain", "")
        } else if path.ends_with("/exec/exec1/json") {
            reply(
                "200 OK",
                "application/json",
                r#"{"ExitCode":3,"Running":false}"#,
            )
        } else if path.ends_with("/images/json") {
            reply(
                "200 OK",
                "application/json",
                r#"[{"Id":"sha256:aabbccddeeff00112233","ParentId":"","RepoTags":["nginx:latest"],"RepoDigests":[],"Created":1,"Size":142000000,"SharedSize":-1,"Labels":{},"Containers":-1},{"Id":"sha256:0123456789ab","ParentId":"","RepoTags":[],"RepoDigests":[],"Created":1,"Size":5000000,"SharedSize":-1,"Labels":{},"Containers":-1}]"#,
            )
        } else if path.ends_with("/volumes") {
            reply(
                "200 OK",
                "application/json",
                r#"{"Volumes":[{"Name":"data","Driver":"local","Mountpoint":"/var/lib/docker/volumes/data/_data","Labels":{},"Scope":"local","Options":{}}],"Warnings":null}"#,
            )
        } else if path.ends_with("/networks") {
            reply(
                "200 OK",
                "application/json",
                r#"[{"Id":"aa11","Name":"bridge","Driver":"bridge","Scope":"local"},{"Id":"bb22","Name":"shop_default","Driver":"bridge","Scope":"local"}]"#,
            )
        } else if method == "POST" && path.ends_with("/containers/create") {
            reply("201 Created", "application/json", r#"{"Id":"abc123","Warnings":[]}"#)
        } else if method == "POST" && path.ends_with("/containers/prune") {
            reply("200 OK", "application/json", r#"{"ContainersDeleted":["aa","bb"],"SpaceReclaimed":2048}"#)
        } else if method == "POST" && path.ends_with("/images/prune") {
            reply("200 OK", "application/json", r#"{"ImagesDeleted":[{"Deleted":"sha256:x"}],"SpaceReclaimed":5000000}"#)
        } else if method == "POST" && path.ends_with("/volumes/prune") {
            reply("200 OK", "application/json", r#"{"VolumesDeleted":["old"],"SpaceReclaimed":10}"#)
        } else if method == "POST" && path.ends_with("/networks/prune") {
            reply("200 OK", "application/json", r#"{"NetworksDeleted":["n1","n2","n3"]}"#)
        } else if method == "POST" && path.ends_with("/images/create") {
            reply(
                "200 OK",
                "application/json",
                concat!(
                    "{\"status\":\"Pulling fs layer\",\"id\":\"l1\"}\r\n",
                    "{\"status\":\"Downloading\",\"id\":\"l1\",\"progressDetail\":{\"current\":50,\"total\":100}}\r\n",
                    "{\"status\":\"Pull complete\",\"id\":\"l1\"}\r\n",
                    "{\"status\":\"Done\"}\r\n",
                ),
            )
        } else if method == "GET" && path.contains("/images/") && path.ends_with("/json") {
            reply(
                "200 OK",
                "application/json",
                r#"{"Id":"sha256:aabbccddeeff00112233","RepoTags":["nginx:latest"],"Created":"2026-01-01T00:00:00Z","Size":142000000,"Os":"linux","Architecture":"amd64","Config":{"Cmd":["nginx","-g","daemon off;"],"ExposedPorts":{"80/tcp":{}},"Labels":{"maintainer":"me"}}}"#,
            )
        } else if method == "GET" && path.contains("/volumes/") {
            reply(
                "200 OK",
                "application/json",
                r#"{"Name":"data","Driver":"local","Mountpoint":"/var/lib/docker/volumes/data/_data","CreatedAt":"2026-01-01T00:00:00Z","Scope":"local","Labels":{},"Options":{}}"#,
            )
        } else if method == "GET" && path.contains("/networks/") {
            reply(
                "200 OK",
                "application/json",
                r#"{"Id":"bb22","Name":"shop_default","Driver":"bridge","Scope":"local","Internal":false,"IPAM":{"Config":[{"Subnet":"172.18.0.0/16","Gateway":"172.18.0.1"}]},"Containers":{"c1":{"Name":"web","IPv4Address":"172.18.0.2/16"}}}"#,
            )
        } else if method == "DELETE" && (path.contains("/volumes/") || path.contains("/networks/"))
        {
            reply("204 No Content", "text/plain", "")
        } else if method == "DELETE" && path.contains("/images/") {
            reply("200 OK", "application/json", "[]")
        } else if path.contains("/fade") {
            reply(
                "404 Not Found",
                "application/json",
                r#"{"message":"No such container"}"#,
            )
        } else if (method == "POST" || method == "DELETE") && path.contains("/containers/") {
            reply("204 No Content", "text/plain", "")
        } else {
            reply(
                "404 Not Found",
                "application/json",
                r#"{"message":"page not found"}"#,
            )
        };
        if socket.write_all(answer.as_bytes()).await.is_err() {
            return;
        }
    }
}
