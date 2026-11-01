//! Files on a server over SFTP, on an SSH connection that is already open (F1, F2, F6). Opening
//! it uses a new channel on that connection, so there is no second login.

use std::sync::Arc;

use async_trait::async_trait;
use russh_sftp::client::SftpSession;
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::protocol::{FileAttributes, FileType, StatusCode};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;

use super::backend::{FileBackend, FileCapabilities, FileEntry, FileKind, Progress};
use super::local::copy_bytes;
use crate::error::{AppError, ErrorCode};
use crate::ssh::pool::SharedConnection;

pub struct SftpFiles {
    sftp: SftpSession,
    /// Keeps the connection open for as long as the file panel is.
    _connection: Arc<SharedConnection>,
}

fn lost() -> AppError {
    AppError::new(
        ErrorCode::ConnectionLost,
        "The connection to the server was lost.",
    )
}

/// Turns an SFTP error into a plain one. The server's own wording isn't shown as it is, because
/// it comes from the server.
fn from_sftp(error: &SftpError, path: &str) -> AppError {
    match error {
        SftpError::Status(status) => match status.status_code {
            StatusCode::NoSuchFile => {
                AppError::new(ErrorCode::NotFound, format!("{path} wasn't found."))
            }
            StatusCode::PermissionDenied => AppError::new(
                ErrorCode::PermissionDenied,
                format!("You don't have permission to use {path}."),
            ),
            StatusCode::NoConnection | StatusCode::ConnectionLost => lost(),
            StatusCode::OpUnsupported => {
                AppError::new(ErrorCode::Unsupported, "The server doesn't support that.")
            }
            _ => AppError::new(
                ErrorCode::Internal,
                format!("The server couldn't do that with {path}."),
            ),
        },
        // The library reports a closed channel in several ways, none of them a server answer.
        SftpError::Timeout | SftpError::IO(_) | SftpError::UnexpectedBehavior(_) => lost(),
        _ => AppError::new(
            ErrorCode::Internal,
            format!("The server couldn't do that with {path}."),
        ),
    }
}

fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

fn entry_of(path: String, name: String, attrs: &FileAttributes) -> FileEntry {
    let kind = match attrs.file_type() {
        FileType::Dir => FileKind::Dir,
        FileType::File => FileKind::File,
        FileType::Symlink => FileKind::Symlink,
        FileType::Other => FileKind::Other,
    };
    FileEntry {
        name,
        path,
        kind,
        size: (kind == FileKind::File).then(|| attrs.len()),
        modified: attrs.mtime.map(i64::from),
        permissions: attrs.permissions.map(|p| p & 0o7777),
    }
}

impl SftpFiles {
    /// Starts SFTP on a new channel of an open connection.
    pub async fn open(connection: Arc<SharedConnection>) -> Result<Self, AppError> {
        let channel = connection
            .handle
            .channel_open_session()
            .await
            .map_err(|_| lost())?;
        channel.request_subsystem(true, "sftp").await.map_err(|_| {
            AppError::new(
                ErrorCode::Unsupported,
                "This server doesn't offer SFTP, so its files can't be browsed.",
            )
        })?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| match e {
                // The server accepted the channel but didn't speak SFTP on it, or closed it.
                SftpError::Timeout => from_sftp(&e, "the server"),
                _ => AppError::new(
                    ErrorCode::Unsupported,
                    "This server doesn't offer SFTP, so its files can't be browsed.",
                ),
            })?;
        Ok(Self {
            sftp,
            _connection: connection,
        })
    }

    /// The folder a session starts in (usually the home folder).
    pub async fn home(&self) -> Result<String, AppError> {
        self.sftp
            .canonicalize(".")
            .await
            .map_err(|e| from_sftp(&e, "."))
    }
}

#[async_trait]
impl FileBackend for SftpFiles {
    fn capabilities(&self) -> FileCapabilities {
        FileCapabilities {
            chmod: true,
            root_delete: true,
        }
    }

    async fn remove_as_root(&self, path: &str, password: Option<&str>) -> Result<(), AppError> {
        super::sudo::remove_as_root(&self._connection.handle, path, password).await
    }

    async fn list(&self, path: &str) -> Result<Vec<FileEntry>, AppError> {
        let entries = self
            .sftp
            .read_dir(path)
            .await
            .map_err(|e| from_sftp(&e, path))?;
        Ok(entries
            .map(|e| entry_of(join(path, &e.file_name()), e.file_name(), &e.metadata()))
            .collect())
    }

    async fn stat(&self, path: &str) -> Result<FileEntry, AppError> {
        let attrs = self
            .sftp
            .symlink_metadata(path)
            .await
            .map_err(|e| from_sftp(&e, path))?;
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        Ok(entry_of(path.to_string(), name, &attrs))
    }

    async fn mkdir(&self, path: &str) -> Result<(), AppError> {
        match self.sftp.create_dir(path).await {
            Ok(()) => Ok(()),
            Err(error) => {
                // Servers answer "failure" for a folder that already exists, so look.
                if self.sftp.try_exists(path).await.unwrap_or(false) {
                    return Err(AppError::new(
                        ErrorCode::AlreadyExists,
                        format!("{path} already exists."),
                    ));
                }
                Err(from_sftp(&error, path))
            }
        }
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), AppError> {
        // SFTP refuses to rename onto something that exists. The queue removes the old file first.
        self.sftp
            .rename(from, to)
            .await
            .map_err(|e| from_sftp(&e, from))
    }

    async fn remove_file(&self, path: &str) -> Result<(), AppError> {
        self.sftp
            .remove_file(path)
            .await
            .map_err(|e| from_sftp(&e, path))
    }

    async fn remove_dir(&self, path: &str) -> Result<(), AppError> {
        self.sftp
            .remove_dir(path)
            .await
            .map_err(|e| from_sftp(&e, path))
    }

    async fn chmod(&self, path: &str, mode: u32) -> Result<(), AppError> {
        let attrs = FileAttributes {
            permissions: Some(mode & 0o7777),
            ..FileAttributes::empty()
        };
        self.sftp
            .set_metadata(path, attrs)
            .await
            .map_err(|e| from_sftp(&e, path))
    }

    async fn read_to(
        &self,
        path: &str,
        dest: &mut (dyn AsyncWrite + Send + Unpin),
        progress: Progress<'_>,
        cancel: CancellationToken,
    ) -> Result<(), AppError> {
        let mut file = self
            .sftp
            .open(path)
            .await
            .map_err(|e| from_sftp(&e, path))?;
        copy_bytes(&mut file, dest, progress, &cancel, path).await
    }

    async fn write_from(
        &self,
        path: &str,
        src: &mut (dyn AsyncRead + Send + Unpin),
        progress: Progress<'_>,
        cancel: CancellationToken,
    ) -> Result<(), AppError> {
        let mut file = self
            .sftp
            .create(path)
            .await
            .map_err(|e| from_sftp(&e, path))?;
        copy_bytes(src, &mut file, progress, &cancel, path).await
    }
}
