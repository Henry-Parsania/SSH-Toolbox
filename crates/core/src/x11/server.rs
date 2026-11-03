//! The X server on Windows. Windows has none for remote programs, so this finds VcXsrv, starts
//! it with a cookie only this app knows, and stops it again.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::display::{self, Endpoint, LocalDisplay};
use crate::error::{AppError, ErrorCode};

/// The display number the app's own server uses.
const DISPLAY: u32 = 0;
const START_TIMEOUT: Duration = Duration::from_secs(10);

/// Where a VcXsrv can be found: next to the app (a bundled copy) or where its installer puts it.
pub fn find_executable() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let mut places = Vec::new();
        if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
            places.push(dir.join("x-server").join("vcxsrv.exe"));
        }
        for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Some(root) = std::env::var_os(var) {
                places.push(Path::new(&root).join("VcXsrv").join("vcxsrv.exe"));
            }
        }
        places.into_iter().find(|p| p.is_file())
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// True if something already answers on the display's TCP port.
pub async fn listening(port: u16) -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_millis(500),
            tokio::net::TcpStream::connect(("127.0.0.1", port))
        )
        .await,
        Ok(Ok(_))
    )
}

/// The X server this app started.
#[derive(Default)]
pub struct Managed {
    child: Option<std::process::Child>,
    display: Option<LocalDisplay>,
}

impl Managed {
    /// The running server, if it is still running.
    pub fn running(&mut self) -> Option<LocalDisplay> {
        if let Some(child) = self.child.as_mut()
            && !matches!(child.try_wait(), Ok(None))
        {
            self.child = None;
            self.display = None;
        }
        self.display.clone()
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.display = None;
    }

    /// Starts VcXsrv in its multi-window mode. Only programs with our cookie are let in.
    pub async fn start(&mut self, exe: &Path, folder: &Path) -> Result<LocalDisplay, AppError> {
        if let Some(display) = self.running() {
            return Ok(display);
        }
        let failed = |what: &str| AppError::new(ErrorCode::Internal, what.to_string());
        let cookie = display::new_cookie();
        std::fs::create_dir_all(folder).map_err(|_| failed("Couldn't prepare the X server."))?;
        let authority = folder.join("x11.authority");
        display::write_authority(&authority, DISPLAY, &cookie)
            .map_err(|_| failed("Couldn't prepare the X server."))?;

        let mut command = std::process::Command::new(exe);
        command
            .arg(format!(":{DISPLAY}"))
            .args(["-multiwindow", "-clipboard", "-wgl", "-auth"])
            .arg(&authority);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let child = command
            .spawn()
            .map_err(|_| failed("The X server couldn't be started."))?;
        self.child = Some(child);

        let port = 6000 + DISPLAY as u16;
        let deadline = tokio::time::Instant::now() + START_TIMEOUT;
        loop {
            if listening(port).await {
                break;
            }
            let ended = self
                .child
                .as_mut()
                .is_none_or(|c| !matches!(c.try_wait(), Ok(None)));
            if ended || tokio::time::Instant::now() >= deadline {
                self.stop();
                return Err(failed("The X server didn't start."));
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        let display = LocalDisplay {
            endpoint: Endpoint::Tcp("127.0.0.1".into(), port),
            number: DISPLAY,
            screen: 0,
            cookie: Some(cookie),
        };
        self.display = Some(display.clone());
        Ok(display)
    }
}

impl Drop for Managed {
    fn drop(&mut self) {
        self.stop();
    }
}
