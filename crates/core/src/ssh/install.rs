//! Putting a public key on a server and checking it works (INTERFACES.md section 15, steps 2-6).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use russh::ChannelMsg;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use super::auth::{Login, LoginMethod, login};
use super::backend::SshServices;
use super::connection::{Connection, connect};
use crate::error::{AppError, ErrorCode};
use crate::prompt::Prompter;
use crate::sessions::{SavedSession, SessionStore};
use crate::terminal::OutputSink;

const EXEC_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_OUTPUT: usize = 64 * 1024;

/// What the server runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerOs {
    Unix,
    Windows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallOutcome {
    Installed,
    AlreadyPresent,
}

#[derive(Debug)]
pub struct ExecOutput {
    pub code: Option<u32>,
    pub stdout: String,
    pub stderr: String,
}

fn lost() -> AppError {
    AppError::new(
        ErrorCode::ConnectionLost,
        "The connection closed while setting up the key.",
    )
}

/// Runs a command on its own channel. `stdin` goes to the command's standard input, so secrets
/// and keys never appear in a shell command line.
pub async fn run_exec(
    connection: &Connection,
    command: &str,
    stdin: Option<&[u8]>,
) -> Result<ExecOutput, AppError> {
    let work = async {
        let mut channel = connection
            .channel_open_session()
            .await
            .map_err(|_| lost())?;
        channel.exec(true, command).await.map_err(|_| lost())?;
        if let Some(bytes) = stdin {
            channel.data(bytes).await.map_err(|_| lost())?;
        }
        channel.eof().await.map_err(|_| lost())?;

        let mut out = ExecOutput {
            code: None,
            stdout: String::new(),
            stderr: String::new(),
        };
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } if stdout.len() < MAX_OUTPUT => {
                    stdout.extend_from_slice(&data)
                }
                ChannelMsg::ExtendedData { data, .. } if stderr.len() < MAX_OUTPUT => {
                    stderr.extend_from_slice(&data)
                }
                ChannelMsg::Failure => {
                    return Err(AppError::new(
                        ErrorCode::Unsupported,
                        "The server refused to run a command.",
                    ));
                }
                ChannelMsg::ExitStatus { exit_status } => out.code = Some(exit_status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        out.stdout = String::from_utf8_lossy(&stdout).into_owned();
        out.stderr = String::from_utf8_lossy(&stderr).into_owned();
        Ok(out)
    };
    tokio::time::timeout(EXEC_TIMEOUT, work)
        .await
        .map_err(|_| {
            AppError::new(
                ErrorCode::ConnectionFailed,
                "The server took too long to answer.",
            )
        })?
}

/// `uname -s` first. If that doesn't work the server may be Windows, which is checked with
/// PowerShell. If neither works, say so.
pub async fn detect_os(connection: &Connection) -> Result<ServerOs, AppError> {
    if let Ok(out) = run_exec(connection, "uname -s", None).await
        && out.code == Some(0)
        && !out.stdout.trim().is_empty()
    {
        return Ok(ServerOs::Unix);
    }
    let check = "[Environment]::OSVersion.Platform";
    if let Ok(out) = run_exec(connection, &powershell_command(check), None).await
        && out.stdout.trim() == "Win32NT"
    {
        return Ok(ServerOs::Windows);
    }
    Err(AppError::new(
        ErrorCode::Unsupported,
        "Couldn't tell what kind of computer the server is. The key can only be installed on Linux, macOS and Windows servers.",
    ))
}

/// Only the key and the app's own comment may reach a script: `ssh-ed25519 <base64>` plus
/// `sshtoolbox`. Anything else is refused before it gets near a shell.
pub fn validate_key_line(line: &str) -> Result<String, AppError> {
    let refuse = || {
        AppError::new(
            ErrorCode::InvalidInput,
            "That isn't a public key the app can install.",
        )
    };
    let line = line.trim_end_matches(['\r', '\n']);
    let mut parts = line.split(' ');
    let (Some(kind), Some(blob), comment, None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(refuse());
    };
    let blob_ok = !blob.is_empty()
        && blob.len() < 2048
        && blob
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+/=".contains(c));
    if kind != "ssh-ed25519" || !blob_ok || comment.is_some_and(|c| c != "sshtoolbox") {
        return Err(refuse());
    }
    Ok(format!("{kind} {blob} sshtoolbox"))
}

/// Linux and macOS (K4). The key arrives on standard input.
pub const UNIX_SCRIPT: &str = r#"umask 077; key=$(cat); blob=$(printf '%s' "$key" | cut -d' ' -f1,2)
mkdir -p ~/.ssh && chmod 700 ~/.ssh && touch ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys || exit 1
grep -qF -- "$blob" ~/.ssh/authorized_keys && { echo already-present; exit 0; }
if [ -s ~/.ssh/authorized_keys ] && [ -n "$(tail -c1 ~/.ssh/authorized_keys)" ]; then echo >> ~/.ssh/authorized_keys; fi
printf '%s\n' "$key" >> ~/.ssh/authorized_keys && echo installed"#;

/// Windows (K5). The validated key goes straight into the script.
pub fn windows_script(validated_key: &str) -> String {
    format!(
        r#"$key = '{validated_key}'
$blob = ($key -split ' ')[0..1] -join ' '
$isAdmin = [bool]((whoami /groups) -match 'S-1-5-32-544')
if ($isAdmin) {{
  $file = Join-Path $env:ProgramData 'ssh\administrators_authorized_keys'
}} else {{
  $dir = Join-Path $env:USERPROFILE '.ssh'
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  $file = Join-Path $dir 'authorized_keys'
}}
if ((Test-Path $file) -and (Select-String -Path $file -SimpleMatch -Quiet -Pattern $blob)) {{ 'already-present'; exit 0 }}
Add-Content -Force -Path $file -Encoding ascii -Value $key
if ($isAdmin) {{
  icacls.exe $file /inheritance:r /grant '*S-1-5-32-544:F' /grant '*S-1-5-18:F' | Out-Null
}} else {{
  $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
  icacls.exe $file /inheritance:r /grant "*${{sid}}:F" /grant '*S-1-5-18:F' /grant '*S-1-5-32-544:F' | Out-Null
}}
if ($LASTEXITCODE -ne 0) {{ 'acl-failed'; exit 1 }}
'installed'
"#
    )
}

/// `powershell -EncodedCommand` takes the script as base64 of UTF-16LE text, which works
/// whether the server's default shell is cmd or PowerShell.
pub fn powershell_command(script: &str) -> String {
    let utf16: Vec<u8> = script
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(utf16);
    format!("powershell -NoProfile -NonInteractive -EncodedCommand {encoded}")
}

fn explain_failure(out: &ExecOutput, os: ServerOs) -> AppError {
    let detail = out.stderr.trim();
    let message = match os {
        ServerOs::Windows if out.stdout.contains("acl-failed") => {
            "Couldn't set the permissions on the key file. For an administrator account, this \
             needs an elevated session on the server."
                .to_string()
        }
        ServerOs::Windows => {
            "Couldn't write the key on the server. If this is an administrator account, writing \
             administrators_authorized_keys needs an elevated session."
                .to_string()
        }
        ServerOs::Unix if detail.is_empty() => {
            "Couldn't add the key to ~/.ssh/authorized_keys on the server.".to_string()
        }
        ServerOs::Unix => format!("Couldn't add the key to ~/.ssh/authorized_keys: {detail}"),
    };
    AppError::new(ErrorCode::PermissionDenied, message)
}

/// Adds the key to the server's list of allowed keys, unless it is already there.
pub async fn install_key(
    connection: &Connection,
    os: ServerOs,
    public_line: &str,
) -> Result<InstallOutcome, AppError> {
    let key = validate_key_line(public_line)?;
    let out = match os {
        ServerOs::Unix => run_exec(connection, UNIX_SCRIPT, Some(key.as_bytes())).await?,
        ServerOs::Windows => {
            run_exec(connection, &powershell_command(&windows_script(&key)), None).await?
        }
    };
    if out.code != Some(0) {
        return Err(explain_failure(&out, os));
    }
    match out.stdout.trim() {
        "installed" => Ok(InstallOutcome::Installed),
        "already-present" => Ok(InstallOutcome::AlreadyPresent),
        _ => Err(explain_failure(&out, os)),
    }
}

/// How the whole setup went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeySetupResult {
    pub outcome: InstallOutcome,
    /// The key login was tried on a fresh connection, and worked (K6).
    pub verified: bool,
}

/// Runs the whole flow for a saved SSH session: log in with the password, find out the server's
/// system, install the key, then open a separate connection that uses only the key. Success is
/// reported only if that last login works.
pub async fn setup_key_login(
    services: &SshServices,
    saved_session_id: &str,
    private_key_path: PathBuf,
    public_line: &str,
    sink: Arc<dyn OutputSink>,
    cancel: CancellationToken,
) -> Result<KeySetupResult, AppError> {
    validate_key_line(public_line)?;
    let path = services.sessions_file.clone();
    let loaded = tokio::task::spawn_blocking(move || SessionStore::new(path).load())
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't read the saved sessions."))?;
    if let Some(problem) = loaded.problem {
        return Err(AppError::new(ErrorCode::InvalidInput, problem));
    }
    let Some(SavedSession::Ssh {
        host, port, user, ..
    }) = loaded
        .file
        .sessions
        .into_iter()
        .find(|s| s.id() == saved_session_id)
    else {
        return Err(AppError::new(
            ErrorCode::NotFound,
            "That saved session doesn't exist any more.",
        ));
    };

    let prompter = Prompter::new(services.broker.clone(), sink, cancel.clone());
    let work = async {
        // Step 2: a normal connection and password login, with every host key check.
        let mut first =
            connect(&host, port, services.known_hosts.clone(), prompter.clone()).await?;
        login(
            &mut first,
            &Login {
                session_id: saved_session_id.to_string(),
                host: host.clone(),
                user: user.clone(),
                method: LoginMethod::Password,
            },
            &services.keychain,
            &prompter,
        )
        .await?;

        // Steps 3 and 4.
        let os = detect_os(&first).await?;
        let outcome = install_key(&first, os, public_line).await?;

        // Step 5: a separate connection that can only succeed with the key.
        let mut second =
            connect(&host, port, services.known_hosts.clone(), prompter.clone()).await?;
        let verified = login(
            &mut second,
            &Login {
                session_id: saved_session_id.to_string(),
                host: host.clone(),
                user: user.clone(),
                method: LoginMethod::Key {
                    path: private_key_path.display().to_string(),
                },
            },
            &services.keychain,
            &prompter,
        )
        .await
        .is_ok();
        if !verified {
            return Err(AppError::new(
                ErrorCode::AuthFailed,
                "The key was installed, but logging in with it didn't work. The server may not allow key logins. Your password still works.",
            ));
        }
        Ok(KeySetupResult { outcome, verified })
    };
    tokio::select! {
        result = work => result,
        _ = cancel.cancelled() => Err(AppError::new(ErrorCode::Cancelled, "Cancelled.")),
    }
}
