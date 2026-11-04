//! Deleting as the server's administrator ("root"), for files the login user can't remove.
//!
//! It runs `sudo rm -r` on the server. The password, if one is needed, goes to sudo's standard
//! input, never into the command line, and is not kept, logged or put in an error. What the
//! server prints is not shown either: only the kind of problem is, in fixed words.

use crate::error::{AppError, ErrorCode};
use crate::ssh::connection::Connection;
use crate::ssh::install::run_exec;

/// Wraps text in single quotes for a Unix shell.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// The folders that must never be removed this way, however it is asked.
fn is_too_broad(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    parts.is_empty() || parts.iter().any(|p| *p == "." || *p == "..")
}

/// The command to run, or why it can't be. `with_password` picks between reading the password
/// from standard input and refusing to ask for one.
fn command(path: &str, with_password: bool) -> Result<String, AppError> {
    if !path.starts_with('/') || path.contains('\0') || path.contains('\n') {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "That isn't a path on the server.",
        ));
    }
    if is_too_broad(path) {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "That folder is too important to delete this way.",
        ));
    }
    // -k forgets any earlier sudo login so the password given here is the one that counts.
    let flags = if with_password { "-S -k -p ''" } else { "-n" };
    Ok(format!("sudo {flags} -- rm -r -- {}", quote(path)))
}

/// Says what went wrong from sudo's own words, without showing them.
fn explain(stderr: &str) -> AppError {
    let text = stderr.to_lowercase();
    if text.contains("password is required") || text.contains("a terminal is required") {
        AppError::new(
            ErrorCode::PermissionDenied,
            "This server wants a password for sudo. Enter it and try again.",
        )
    } else if text.contains("incorrect password") || text.contains("try again") {
        AppError::new(
            ErrorCode::PermissionDenied,
            "That sudo password wasn't right.",
        )
    } else if text.contains("not in the sudoers") || text.contains("not allowed") {
        AppError::new(
            ErrorCode::PermissionDenied,
            "This user isn't allowed to use sudo on the server.",
        )
    } else if text.contains("sudo") && text.contains("not found") {
        AppError::new(
            ErrorCode::Unsupported,
            "sudo isn't installed on this server.",
        )
    } else {
        AppError::new(ErrorCode::Internal, "The server couldn't delete that.")
    }
}

/// Deletes a file, or a folder and everything in it, as root. `password` is the sudo password, or
/// none if sudo doesn't need one for this user.
pub async fn remove_as_root(
    connection: &Connection,
    path: &str,
    password: Option<&str>,
) -> Result<(), AppError> {
    let password = password.filter(|p| !p.is_empty());
    let command = command(path, password.is_some())?;
    let line = password.map(|p| format!("{p}\n"));
    let out = run_exec(connection, &command, line.as_deref().map(str::as_bytes)).await?;
    match out.code {
        Some(0) => Ok(()),
        _ => Err(explain(&out.stderr)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_path_is_quoted_so_it_cannot_run_anything() {
        let c = command("/home/me/it's; rm -rf ~", false).unwrap();
        assert_eq!(c, "sudo -n -- rm -r -- '/home/me/it'\\''s; rm -rf ~'");
    }

    #[test]
    fn a_password_is_never_part_of_the_command() {
        let c = command("/srv/old", true).unwrap();
        assert!(c.starts_with("sudo -S -k -p ''"));
        assert!(!c.contains("hunter2"));
    }

    #[test]
    fn the_top_folder_and_tricky_paths_are_refused() {
        for bad in [
            "/", "//", "/.", "/home/..", "relative", "/a\nb", "/a\0b", "",
        ] {
            assert!(command(bad, false).is_err(), "{bad:?}");
        }
        assert!(command("/home/me/old", false).is_ok());
    }

    #[test]
    fn server_wording_is_turned_into_fixed_messages() {
        assert!(
            explain("sudo: a password is required")
                .message
                .contains("wants a password")
        );
        assert!(
            explain("Sorry, try again.")
                .message
                .contains("wasn't right")
        );
        assert!(
            explain("me is not in the sudoers file")
                .message
                .contains("isn't allowed")
        );
        let other = explain("rm: cannot remove 'secret-name': Device busy");
        assert!(!other.message.contains("secret-name"));
    }
}
