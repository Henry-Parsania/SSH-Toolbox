//! Opening a file in the program the computer uses for it.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::{AppError, ErrorCode};

fn failed() -> AppError {
    AppError::new(ErrorCode::Internal, "Couldn't open the file.")
}

fn spawn(mut command: Command) -> Result<(), AppError> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| failed())
}

/// Opens the file with the program the system has chosen for its type.
pub fn open_default(path: &Path) -> Result<(), AppError> {
    #[cfg(windows)]
    {
        let mut command = Command::new("explorer.exe");
        command.arg(path);
        spawn(command)
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        command.arg(path);
        spawn(command)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        spawn(command)
    }
}

/// Asks which program to open the file with (Windows). Elsewhere it opens it as usual.
pub fn open_with_dialog(path: &Path) -> Result<(), AppError> {
    #[cfg(windows)]
    {
        let mut command = Command::new("rundll32.exe");
        command.arg("shell32.dll,OpenAs_RunDLL").arg(path);
        spawn(command)
    }
    #[cfg(not(windows))]
    {
        open_default(path)
    }
}
