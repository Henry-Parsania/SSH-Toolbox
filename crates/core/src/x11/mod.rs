//! X11 forwarding: programs on a server show their windows on this computer.
//!
//! The server is told to forward X11 with a made-up cookie. Each program that opens a window
//! connects back through the SSH connection; we check its cookie, swap in the real one for the
//! local X server, and join the two.

pub mod display;
pub mod server;
pub mod setup;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use crate::error::{AppError, ErrorCode};
use display::{Endpoint, LocalDisplay};

/// How long a program has to send its first message once the tunnel is open.
const SETUP_TIMEOUT: Duration = Duration::from_secs(10);

/// What one SSH connection forwards: the made-up cookie it gave the server, and the local server
/// the windows go to.
pub struct Forward {
    pub fake_cookie: Vec<u8>,
    pub local: LocalDisplay,
}

/// The forwarding of one connection, shared by the panes on it.
pub type Slot = Arc<Mutex<Option<Arc<Forward>>>>;

/// Where the local X server comes from.
pub struct X11Local {
    /// A server to use instead of looking for one (tests).
    fixed: Option<LocalDisplay>,
    /// Only used where the app starts the X server itself (not on Unix).
    #[cfg_attr(unix, allow(dead_code))]
    folder: PathBuf,
    managed: tokio::sync::Mutex<server::Managed>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum X11State {
    /// A server is ready for windows.
    Running,
    /// One can be started, and will be when a session needs it.
    Stopped,
    /// There is none and none can be started.
    Missing,
}

#[derive(Debug, Clone, Serialize)]
pub struct X11Status {
    pub state: X11State,
    /// A sentence for the screen.
    pub detail: String,
}

impl X11Local {
    pub fn new(folder: PathBuf) -> Self {
        Self {
            fixed: None,
            folder,
            managed: Default::default(),
        }
    }

    #[cfg(test)]
    pub fn fixed(display: LocalDisplay) -> Self {
        Self {
            fixed: Some(display),
            folder: PathBuf::new(),
            managed: Default::default(),
        }
    }

    /// The X server on this computer, starting the app's own if that is how it works here.
    pub async fn find(&self) -> Result<LocalDisplay, AppError> {
        if let Some(fixed) = &self.fixed {
            return Ok(fixed.clone());
        }
        self.find_system().await
    }

    #[cfg(unix)]
    async fn find_system(&self) -> Result<LocalDisplay, AppError> {
        let text = std::env::var("DISPLAY").unwrap_or_default();
        let Some(mut found) = display::parse_display(&text) else {
            return Err(AppError::new(
                ErrorCode::NotFound,
                "There is no X display on this computer (DISPLAY isn't set).",
            ));
        };
        found.cookie =
            display::default_authority().and_then(|file| display::read_cookie(&file, found.number));
        Ok(found)
    }

    #[cfg(not(unix))]
    async fn find_system(&self) -> Result<LocalDisplay, AppError> {
        let mut managed = self.managed.lock().await;
        if let Some(running) = managed.running() {
            return Ok(running);
        }
        // Something else already serves display 0, such as the user's own X server. It has its
        // own rules about who may connect, so no cookie is sent.
        if server::listening(6000).await {
            let mut found = display::parse_display("127.0.0.1:0").unwrap();
            found.cookie = None;
            return Ok(found);
        }
        let Some(exe) = server::find_executable() else {
            return Err(AppError::new(
                ErrorCode::NotFound,
                "No X server found. Install VcXsrv, then connect again.",
            ));
        };
        managed.start(&exe, &self.folder).await
    }

    pub async fn status(&self) -> X11Status {
        if self.fixed.is_some() {
            return X11Status {
                state: X11State::Running,
                detail: "An X server is ready.".into(),
            };
        }
        #[cfg(unix)]
        {
            let ok =
                display::parse_display(&std::env::var("DISPLAY").unwrap_or_default()).is_some();
            X11Status {
                state: if ok {
                    X11State::Running
                } else {
                    X11State::Missing
                },
                detail: if ok {
                    "This desktop's X display is used.".into()
                } else {
                    "No X display (DISPLAY isn't set).".into()
                },
            }
        }
        #[cfg(not(unix))]
        {
            if self.managed.lock().await.running().is_some() {
                return X11Status {
                    state: X11State::Running,
                    detail: "The app's X server is running.".into(),
                };
            }
            if server::listening(6000).await {
                return X11Status {
                    state: X11State::Running,
                    detail: "Another X server is running on this computer.".into(),
                };
            }
            if server::find_executable().is_some() {
                X11Status {
                    state: X11State::Stopped,
                    detail: "VcXsrv is installed. It starts when a session needs it.".into(),
                }
            } else {
                X11Status {
                    state: X11State::Missing,
                    detail: "No X server found. Install VcXsrv.".into(),
                }
            }
        }
    }

    /// Starts the app's X server now. Does nothing where the desktop already has one.
    pub async fn start(&self) -> Result<X11Status, AppError> {
        self.find().await?;
        Ok(self.status().await)
    }

    /// Stops the X server the app started, closing its windows.
    pub async fn stop(&self) -> X11Status {
        self.managed.lock().await.stop();
        self.status().await
    }

    /// Stops the server without waiting, for when the app quits.
    pub fn stop_now(&self) {
        if let Ok(mut managed) = self.managed.try_lock() {
            managed.stop();
        }
    }

    /// The forwarding for a connection: the one already set up, or a new one with a fresh cookie.
    pub async fn prepare(&self, slot: &Slot) -> Result<Arc<Forward>, AppError> {
        let local = self.find().await?;
        let mut held = slot.lock().unwrap();
        if let Some(existing) = held.as_ref()
            && existing.local == local
        {
            return Ok(existing.clone());
        }
        let forward = Arc::new(Forward {
            fake_cookie: display::new_cookie(),
            local,
        });
        *held = Some(forward.clone());
        Ok(forward)
    }
}

/// Carries one program's connection to the local X server.
pub async fn serve<S>(mut remote: S, forward: Arc<Forward>)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let Ok(Ok(packet)) = tokio::time::timeout(SETUP_TIMEOUT, setup::read_setup(&mut remote)).await
    else {
        return;
    };
    let Some(first) = setup::rewrite(
        &packet,
        &forward.fake_cookie,
        forward.local.cookie.as_deref(),
    ) else {
        return; // Without our cookie the program is not let in.
    };
    match &forward.local.endpoint {
        Endpoint::Tcp(host, port) => {
            if let Ok(local) = tokio::net::TcpStream::connect((host.as_str(), *port)).await {
                relay(remote, local, &first).await;
            }
        }
        #[cfg(unix)]
        Endpoint::Unix(path) => {
            if let Ok(local) = tokio::net::UnixStream::connect(path).await {
                relay(remote, local, &first).await;
            }
        }
    }
}

async fn relay<R, L>(mut remote: R, mut local: L, first: &[u8])
where
    R: AsyncRead + AsyncWrite + Unpin,
    L: AsyncRead + AsyncWrite + Unpin,
{
    if local.write_all(first).await.is_err() {
        return;
    }
    let _ = tokio::io::copy_bidirectional(&mut remote, &mut local).await;
    let _ = remote.shutdown().await;
}
