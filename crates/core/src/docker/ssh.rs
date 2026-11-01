//! Talking to Docker on a server over an SSH connection that is already open.
//!
//! Two ways are tried in order, and the first that answers is used (INTERFACES.md section 8):
//! 1. `docker system dial-stdio` on an exec channel, which is what the Docker command line does
//!    over SSH. It needs the `docker` command on the server.
//! 2. A channel straight to `/var/run/docker.sock`, which needs nothing installed but needs the
//!    server to allow it.
//!
//! Every request opens its own channel, so a slow log stream never blocks a quick list. Nothing
//! is installed on the server and `sudo` is never used.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use bollard::Docker;
use hyper::Uri;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::{Connected, Connection};
use hyper_util::rt::{TokioExecutor, TokioIo};
use russh::ChannelStream;
use russh::client::Msg;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use super::explain_failure;
use crate::error::{AppError, ErrorCode};
use crate::ssh::install::run_exec;
use crate::ssh::pool::SharedConnection;

const DIAL_STDIO: &str = "docker system dial-stdio";
const SOCKET: &str = "/var/run/docker.sock";
/// How long a server gets to answer the first request of a method before the next is tried.
const PING_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT_SECONDS: u64 = 120;

/// How the app reaches Docker on the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    DialStdio,
    Socket,
}

impl Method {
    const ORDER: [Method; 2] = [Method::DialStdio, Method::Socket];
}

/// A channel, shaped so the HTTP client accepts it as a connection.
struct Stream(ChannelStream<Msg>);

impl Connection for Stream {
    fn connected(&self) -> Connected {
        Connected::new()
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

/// Opens a fresh channel to Docker for each connection the HTTP client asks for.
#[derive(Clone)]
struct Connector {
    shared: Arc<SharedConnection>,
    method: Method,
}

impl tower_service::Service<Uri> for Connector {
    type Response = TokioIo<Stream>;
    type Error = std::io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // The address in the URI is a placeholder: the channel decides where the request goes.
    fn call(&mut self, _uri: Uri) -> Self::Future {
        let (shared, method) = (self.shared.clone(), self.method);
        Box::pin(async move {
            let broken = |_| std::io::Error::other("The connection to the server closed.");
            let stream = match method {
                Method::DialStdio => {
                    let channel = shared.handle.channel_open_session().await.map_err(broken)?;
                    channel.exec(false, DIAL_STDIO).await.map_err(broken)?;
                    channel.into_stream()
                }
                Method::Socket => shared
                    .handle
                    .channel_open_direct_streamlocal(SOCKET)
                    .await
                    .map_err(broken)?
                    .into_stream(),
            };
            Ok(TokioIo::new(Stream(stream)))
        })
    }
}

/// A Docker client that sends its requests over `shared` by the given method.
fn client(shared: Arc<SharedConnection>, method: Method) -> Result<Docker, AppError> {
    let mut builder = Client::builder(TokioExecutor::new());
    // A connection is one channel running one `docker` command; reusing it is not worth the
    // risk of a half-closed channel.
    builder.pool_max_idle_per_host(0);
    let http = Arc::new(builder.build(Connector { shared, method }));
    Docker::connect_with_custom_transport(
        move |request: bollard::BollardRequest| {
            let http = http.clone();
            async move {
                http.request(request)
                    .await
                    .map_err(bollard::errors::Error::from)
            }
        },
        Some("http://docker"),
        REQUEST_TIMEOUT_SECONDS,
        bollard::API_DEFAULT_VERSION,
    )
    .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't set up the Docker client."))
}

/// Connects to Docker on the server behind `shared`, trying each method in turn. When none works
/// the server is asked why, and the answer is explained in plain words.
pub async fn connect_ssh(shared: Arc<SharedConnection>) -> Result<(Docker, Method), AppError> {
    for method in Method::ORDER {
        let docker = client(shared.clone(), method)?;
        let answered = tokio::time::timeout(PING_TIMEOUT, docker.ping()).await;
        if matches!(answered, Ok(Ok(_))) {
            let docker = docker.negotiate_version().await.map_err(|_| {
                AppError::new(
                    ErrorCode::ConnectionFailed,
                    "Docker on that server didn't agree on a version.",
                )
            })?;
            return Ok((docker, method));
        }
    }
    Err(why_not(&shared).await)
}

/// Runs the same command the first method uses, with nothing to say to it, and reads the reason
/// from how it ends.
async fn why_not(shared: &SharedConnection) -> AppError {
    match run_exec(&shared.handle, DIAL_STDIO, None).await {
        Ok(out) if out.code == Some(0) => AppError::new(
            ErrorCode::ConnectionFailed,
            "Couldn't reach Docker on that server.",
        ),
        Ok(out) => explain_failure(out.code, &out.stderr),
        Err(e) => e,
    }
}
