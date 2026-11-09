//! What the agent's tools do on the server, with the safety rules applied before and after:
//! commands are classified first, and everything that comes back has its secrets removed and its
//! length limited.

use serde::Serialize;
use tokio_util::sync::CancellationToken;

use super::policy::{Risk, classify_with};
use super::redact::redact;
use crate::error::{AppError, ErrorCode};
use crate::files::FileBackend;
use crate::ssh::connection::Connection;
use crate::ssh::install::run_exec;

/// The most of a command's output that is passed on: the start and the end are kept.
const OUTPUT_HEAD: usize = 12 * 1024;
const OUTPUT_TAIL: usize = 4 * 1024;
/// Largest file the agent may read or write in one go.
pub const MAX_FILE: u64 = 256 * 1024;
/// How much of a file is passed on.
const FILE_SHOWN: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExecResult {
    /// The command's exit code, if it finished.
    pub code: Option<u32>,
    /// What it printed (output and errors together), with secrets removed.
    pub output: String,
}

/// Keeps the start and the end of long text.
pub fn shorten(text: &str) -> String {
    if text.len() <= OUTPUT_HEAD + OUTPUT_TAIL {
        return text.to_string();
    }
    let mut head = OUTPUT_HEAD;
    while !text.is_char_boundary(head) {
        head -= 1;
    }
    let mut tail = text.len() - OUTPUT_TAIL;
    while !text.is_char_boundary(tail) {
        tail += 1;
    }
    format!(
        "{}\n… ({} bytes left out) …\n{}",
        &text[..head],
        tail - head,
        &text[tail..]
    )
}

/// Whether the rules allow running this command. `approved` is the user's say-so, which can allow
/// a change but never a blocked command.
pub fn allow(command: &str, approved: bool) -> Result<Risk, AppError> {
    allow_with(command, approved, false)
}

/// Like [`allow`]. When the user has allowed dangerous commands, those need `approved` too.
pub fn allow_with(command: &str, approved: bool, allow_dangerous: bool) -> Result<Risk, AppError> {
    match classify_with(command, allow_dangerous) {
        Risk::Dangerous if !approved => Err(AppError::new(
            ErrorCode::PermissionDenied,
            "That command is dangerous, so the user has to approve it first.",
        )),
        Risk::Blocked => Err(AppError::new(
            ErrorCode::PermissionDenied,
            "That command is too dangerous to run from here, even with approval.",
        )),
        Risk::Changes if !approved => Err(AppError::new(
            ErrorCode::PermissionDenied,
            "That command changes things, so the user has to approve it first.",
        )),
        risk => Ok(risk),
    }
}

/// Runs a command on the server (30 seconds at most) and returns what it printed.
pub async fn run_command(
    connection: &Connection,
    command: &str,
    approved: bool,
    allow_dangerous: bool,
) -> Result<ExecResult, AppError> {
    allow_with(command, approved, allow_dangerous)?;
    let out = run_exec(connection, command, None).await?;
    let mut text = out.stdout;
    if !out.stderr.trim().is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&out.stderr);
    }
    Ok(ExecResult {
        code: out.code,
        output: shorten(&redact(&text)),
    })
}

/// Paths that hold secrets. Reading or writing them needs the user's say-so.
pub fn sensitive_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    [
        "id_rsa",
        "id_ed25519",
        "id_ecdsa",
        "/etc/shadow",
        "/.env",
        ".pem",
        ".kdbx",
        ".netrc",
        ".pgpass",
        "/.ssh/",
        "credentials",
    ]
    .iter()
    .any(|s| p.contains(s))
}

fn path_error(message: &str) -> AppError {
    AppError::new(ErrorCode::InvalidInput, message)
}

/// Reads a text file for the model. Binary files and big files are refused.
pub async fn read_text_file(
    backend: &dyn FileBackend,
    path: &str,
    approved: bool,
) -> Result<String, AppError> {
    if sensitive_path(path) && !approved {
        return Err(AppError::new(
            ErrorCode::PermissionDenied,
            "That file may hold secrets, so the user has to approve reading it first.",
        ));
    }
    let entry = backend.stat(path).await?;
    if entry.size.unwrap_or(0) > MAX_FILE {
        return Err(path_error(
            "That file is too big to read whole. Use head, tail or grep instead.",
        ));
    }
    let mut data: Vec<u8> = Vec::new();
    backend
        .read_to(path, &mut data, &|_| {}, CancellationToken::new())
        .await?;
    if data.contains(&0) {
        return Err(path_error("That looks like a binary file, not text."));
    }
    let text = String::from_utf8_lossy(&data).into_owned();
    let mut text = redact(&text);
    if text.len() > FILE_SHOWN {
        let mut cut = FILE_SHOWN;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n… (the rest of the file is left out)");
    }
    Ok(text)
}

/// Replaces (or creates) a text file. Needs the user's approval.
pub async fn write_text_file(
    backend: &dyn FileBackend,
    path: &str,
    content: &str,
    approved: bool,
) -> Result<(), AppError> {
    if !approved {
        return Err(AppError::new(
            ErrorCode::PermissionDenied,
            "Writing a file needs the user's approval first.",
        ));
    }
    if !path.starts_with('/') || path.contains('\0') || path.ends_with('/') {
        return Err(path_error(
            "Use a full path to a file, such as /etc/app/config.yml.",
        ));
    }
    if content.len() as u64 > MAX_FILE {
        return Err(path_error("That is too much text to write in one go."));
    }
    let mut source = content.as_bytes();
    backend
        .write_from(path, &mut source, &|_| {}, CancellationToken::new())
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::memory::MemoryFiles;

    #[test]
    fn the_rules_come_before_the_user_s_approval() {
        assert!(allow("ls -la", false).is_ok());
        assert!(allow("systemctl restart nginx", false).is_err());
        assert!(allow("systemctl restart nginx", true).is_ok());
    }

    #[test]
    fn dangerous_commands_are_blocked_unless_allowed_and_then_still_need_approval() {
        let cmd = "rm -rf / --no-preserve-root";
        // By default nothing can run it, approved or not.
        assert!(allow_with(cmd, true, false).is_err());
        assert!(allow(cmd, true).is_err());
        // When the user allows them, it needs approval, one command at a time.
        assert!(allow_with(cmd, false, true).is_err());
        assert_eq!(allow_with(cmd, true, true).unwrap(), Risk::Dangerous);
        // The switch changes nothing for ordinary commands.
        assert!(allow_with("ls -la", false, true).is_ok());
        assert!(allow_with("systemctl restart nginx", false, true).is_err());
        // A blocked command stays blocked even when approved.
        assert!(allow("rm -rf /", true).is_err());
    }

    #[test]
    fn long_output_keeps_both_ends() {
        let long = format!("start{}end", "x".repeat(50_000));
        let short = shorten(&long);
        assert!(short.starts_with("start") && short.ends_with("end"));
        assert!(short.contains("bytes left out"));
        assert!(short.len() < 17_000);
        assert_eq!(shorten("small"), "small");
    }

    #[tokio::test]
    async fn files_are_read_with_secrets_removed_and_binary_refused() {
        let files = MemoryFiles::new();
        files.put("/etc/app.conf", b"port=80\nDB_PASSWORD=hunter2\n");
        files.put("/bin/tool", &[1, 2, 0, 3]);
        files.put("/home/me/.ssh/id_ed25519", b"x");
        let text = read_text_file(&files, "/etc/app.conf", false)
            .await
            .unwrap();
        assert!(
            text.contains("port=80") && !text.contains("hunter2"),
            "{text}"
        );
        assert!(read_text_file(&files, "/bin/tool", false).await.is_err());
        assert!(
            read_text_file(&files, "/home/me/.ssh/id_ed25519", false)
                .await
                .is_err()
        );
        assert!(
            read_text_file(&files, "/home/me/.ssh/id_ed25519", true)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn writing_needs_approval_and_a_full_path() {
        let files = MemoryFiles::new();
        assert!(write_text_file(&files, "/a.txt", "x", false).await.is_err());
        assert!(write_text_file(&files, "a.txt", "x", true).await.is_err());
        assert!(write_text_file(&files, "/dir/", "x", true).await.is_err());
        files.put_dir("/etc");
        write_text_file(&files, "/etc/new.conf", "hello", true)
            .await
            .unwrap();
        assert_eq!(
            read_text_file(&files, "/etc/new.conf", false)
                .await
                .unwrap(),
            "hello"
        );
    }
}
