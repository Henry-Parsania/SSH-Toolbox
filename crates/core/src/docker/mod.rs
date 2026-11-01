//! Docker, on this computer and on a server reached over SSH (INTERFACES.md section 8).

pub mod ssh;

#[cfg(test)]
mod ssh_tests;

use bollard::Docker;

use crate::error::{AppError, ErrorCode};

/// Connects to Docker on this computer: the Unix socket on Linux, the named pipe on Windows, or
/// whatever `DOCKER_HOST` names. The engine is pinged so a missing one is reported here, in plain
/// words, instead of by the first screen that needs it.
pub async fn connect_local() -> Result<Docker, AppError> {
    let not_running = || {
        let message = if cfg!(windows) {
            "Docker isn't running. Start Docker Desktop and try again."
        } else {
            "Docker isn't running, or this user can't reach it. Start the Docker service, and check that your user is in the docker group."
        };
        AppError::new(ErrorCode::ConnectionFailed, message)
    };
    let docker = Docker::connect_with_defaults().map_err(|_| not_running())?;
    docker.ping().await.map_err(|_| not_running())?;
    // An older engine rejects the newest API version, so the two agree on one first.
    docker.negotiate_version().await.map_err(|_| not_running())
}

/// Turns the text a command printed on a server into a plain-words reason. Only the kind of
/// problem is used: the text itself comes from the server and is never shown.
pub fn explain_failure(code: Option<u32>, stderr: &str) -> AppError {
    let text = stderr.to_lowercase();
    let (code_kind, message) = if text.contains("permission denied") {
        (
            ErrorCode::PermissionDenied,
            "That user isn't allowed to use Docker on the server. Add the user to the docker group there (sudo usermod -aG docker <user>), then log in again. This app never uses sudo.",
        )
    } else if code == Some(127) || text.contains("not found") || text.contains("no such file") {
        (
            ErrorCode::NotFound,
            "Docker doesn't seem to be installed on that server.",
        )
    } else if text.contains("cannot connect") || text.contains("daemon running") {
        (
            ErrorCode::ConnectionFailed,
            "Docker is installed on the server, but its engine isn't running.",
        )
    } else {
        (
            ErrorCode::ConnectionFailed,
            "Couldn't reach Docker on that server.",
        )
    };
    AppError::new(code_kind, message)
}
