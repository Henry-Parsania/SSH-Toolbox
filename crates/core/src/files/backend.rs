use async_trait::async_trait;
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;

use crate::error::{AppError, ErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    File,
    Dir,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: FileKind,
    pub size: Option<u64>,
    /// Seconds since 1970, if known.
    pub modified: Option<i64>,
    /// Unix permission bits, if the system has them.
    pub permissions: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FileCapabilities {
    pub chmod: bool,
    /// Whether `remove_as_root` works (servers reached over SSH).
    pub root_delete: bool,
}

/// Reports how many bytes have been moved so far.
pub type Progress<'a> = &'a (dyn Fn(u64) + Send + Sync);

/// How much is moved at a time.
pub const COPY_BUFFER: usize = 64 * 1024;

#[async_trait]
pub trait FileBackend: Send + Sync {
    fn capabilities(&self) -> FileCapabilities;
    async fn list(&self, path: &str) -> Result<Vec<FileEntry>, AppError>;
    async fn stat(&self, path: &str) -> Result<FileEntry, AppError>;
    async fn mkdir(&self, path: &str) -> Result<(), AppError>;
    async fn rename(&self, from: &str, to: &str) -> Result<(), AppError>;
    async fn remove_file(&self, path: &str) -> Result<(), AppError>;
    /// Empty folders only.
    async fn remove_dir(&self, path: &str) -> Result<(), AppError>;
    /// `Unsupported` when `capabilities().chmod` is false.
    async fn chmod(&self, path: &str, mode: u32) -> Result<(), AppError>;
    /// Deletes a file or a folder with everything in it, as the server's administrator, using
    /// `sudo`. `password` is the sudo password, or none if none is needed. `Unsupported` when
    /// `capabilities().root_delete` is false.
    async fn remove_as_root(&self, _path: &str, _password: Option<&str>) -> Result<(), AppError> {
        Err(AppError::new(
            ErrorCode::Unsupported,
            "Deleting as root isn't available here.",
        ))
    }
    /// Streams the file into `dest`. Stops with `Cancelled` when `cancel` fires.
    async fn read_to(
        &self,
        path: &str,
        dest: &mut (dyn AsyncWrite + Send + Unpin),
        progress: Progress<'_>,
        cancel: CancellationToken,
    ) -> Result<(), AppError>;
    /// Streams `src` into the file, replacing it if present.
    async fn write_from(
        &self,
        path: &str,
        src: &mut (dyn AsyncRead + Send + Unpin),
        progress: Progress<'_>,
        cancel: CancellationToken,
    ) -> Result<(), AppError>;
}
