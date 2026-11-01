//! A file backend that lives in memory, for tests of the transfer queue. It can be told to read
//! slowly or to fail part way, and it behaves like a real one about replacing files:
//! renaming onto something that exists is refused.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use super::backend::{FileBackend, FileCapabilities, FileEntry, FileKind, Progress};
use crate::error::{AppError, ErrorCode};

#[derive(Default)]
pub struct MemoryFiles {
    files: Mutex<BTreeMap<String, Vec<u8>>>,
    dirs: Mutex<BTreeSet<String>>,
    /// Entries that are links, by path.
    links: Mutex<BTreeSet<String>>,
    /// Reading a file stops with an error after this many bytes.
    pub fail_read_after: Mutex<Option<usize>>,
    /// Each chunk read waits this long (milliseconds), so a transfer can be cancelled mid-way.
    pub read_delay_ms: AtomicUsize,
    /// Names made up for a listing that aren't real files (to try hostile names).
    pub fake_names: Mutex<Vec<(String, String)>>,
    /// Paths that were removed, in order.
    pub removed: Mutex<Vec<String>>,
}

fn name_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn parent_of(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((parent, _)) => parent.to_string(),
        None => String::new(),
    }
}

impl MemoryFiles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put(&self, path: &str, bytes: &[u8]) {
        let mut parent = parent_of(path);
        while !parent.is_empty() {
            self.dirs.lock().unwrap().insert(parent.clone());
            parent = parent_of(&parent);
        }
        self.files
            .lock()
            .unwrap()
            .insert(path.into(), bytes.to_vec());
    }

    pub fn put_dir(&self, path: &str) {
        self.dirs.lock().unwrap().insert(path.into());
    }

    pub fn put_link(&self, path: &str) {
        self.links.lock().unwrap().insert(path.into());
        self.files
            .lock()
            .unwrap()
            .insert(path.into(), b"x".to_vec());
    }

    pub fn get(&self, path: &str) -> Option<Vec<u8>> {
        self.files.lock().unwrap().get(path).cloned()
    }

    pub fn names(&self) -> Vec<String> {
        self.files.lock().unwrap().keys().cloned().collect()
    }

    pub fn dir_names(&self) -> Vec<String> {
        self.dirs.lock().unwrap().iter().cloned().collect()
    }

    fn entry(&self, path: &str) -> Option<FileEntry> {
        if self.links.lock().unwrap().contains(path) {
            return Some(FileEntry {
                name: name_of(path),
                path: path.into(),
                kind: FileKind::Symlink,
                size: None,
                modified: None,
                permissions: None,
            });
        }
        if let Some(bytes) = self.files.lock().unwrap().get(path) {
            return Some(FileEntry {
                name: name_of(path),
                path: path.into(),
                kind: FileKind::File,
                size: Some(bytes.len() as u64),
                modified: Some(0),
                permissions: None,
            });
        }
        self.dirs.lock().unwrap().contains(path).then(|| FileEntry {
            name: name_of(path),
            path: path.into(),
            kind: FileKind::Dir,
            size: None,
            modified: Some(0),
            permissions: None,
        })
    }
}

fn not_found(path: &str) -> AppError {
    AppError::new(ErrorCode::NotFound, format!("{path} wasn't found."))
}

#[async_trait]
impl FileBackend for MemoryFiles {
    fn capabilities(&self) -> FileCapabilities {
        FileCapabilities {
            chmod: false,
            root_delete: false,
        }
    }

    async fn list(&self, path: &str) -> Result<Vec<FileEntry>, AppError> {
        if !self.dirs.lock().unwrap().contains(path) {
            return Err(not_found(path));
        }
        // Collect the paths first, so no lock is held while each entry is looked up.
        let paths: Vec<String> = {
            let files = self.files.lock().unwrap();
            let dirs = self.dirs.lock().unwrap();
            files
                .keys()
                .chain(dirs.iter())
                .filter(|p| parent_of(p) == path && p.as_str() != path)
                .cloned()
                .collect()
        };
        let mut entries: Vec<FileEntry> = paths.iter().filter_map(|p| self.entry(p)).collect();
        for (dir, name) in self.fake_names.lock().unwrap().iter() {
            if dir == path {
                entries.push(FileEntry {
                    name: name.clone(),
                    path: format!("{path}/{name}"),
                    kind: FileKind::File,
                    size: Some(1),
                    modified: None,
                    permissions: None,
                });
            }
        }
        Ok(entries)
    }

    async fn stat(&self, path: &str) -> Result<FileEntry, AppError> {
        self.entry(path).ok_or_else(|| not_found(path))
    }

    async fn mkdir(&self, path: &str) -> Result<(), AppError> {
        if self.entry(path).is_some() {
            return Err(AppError::new(ErrorCode::AlreadyExists, "exists"));
        }
        self.dirs.lock().unwrap().insert(path.into());
        Ok(())
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), AppError> {
        if self.entry(to).is_some() {
            return Err(AppError::new(ErrorCode::AlreadyExists, "exists"));
        }
        let mut files = self.files.lock().unwrap();
        let bytes = files.remove(from).ok_or_else(|| not_found(from))?;
        files.insert(to.into(), bytes);
        Ok(())
    }

    async fn remove_file(&self, path: &str) -> Result<(), AppError> {
        self.removed.lock().unwrap().push(path.into());
        self.files
            .lock()
            .unwrap()
            .remove(path)
            .map(|_| ())
            .ok_or_else(|| not_found(path))
    }

    async fn remove_dir(&self, path: &str) -> Result<(), AppError> {
        let busy = {
            let files = self.files.lock().unwrap();
            let dirs = self.dirs.lock().unwrap();
            files
                .keys()
                .chain(dirs.iter())
                .any(|p| parent_of(p) == path)
        };
        if busy {
            return Err(AppError::new(ErrorCode::InvalidInput, "not empty"));
        }
        self.removed.lock().unwrap().push(path.into());
        self.dirs.lock().unwrap().remove(path);
        Ok(())
    }

    async fn chmod(&self, _path: &str, _mode: u32) -> Result<(), AppError> {
        Err(AppError::new(ErrorCode::Unsupported, "no permissions here"))
    }

    async fn read_to(
        &self,
        path: &str,
        dest: &mut (dyn AsyncWrite + Send + Unpin),
        progress: Progress<'_>,
        cancel: CancellationToken,
    ) -> Result<(), AppError> {
        let bytes = self.get(path).ok_or_else(|| not_found(path))?;
        let limit = *self.fail_read_after.lock().unwrap();
        let delay = self.read_delay_ms.load(Ordering::SeqCst);
        let mut sent = 0usize;
        for chunk in bytes.chunks(1024) {
            if cancel.is_cancelled() {
                return Err(AppError::new(ErrorCode::Cancelled, "cancelled"));
            }
            if limit.is_some_and(|l| sent >= l) {
                return Err(AppError::new(
                    ErrorCode::ConnectionLost,
                    "the source failed",
                ));
            }
            if delay > 0 {
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_millis(delay as u64)) => {}
                    _ = cancel.cancelled() => {
                        return Err(AppError::new(ErrorCode::Cancelled, "cancelled"));
                    }
                }
            }
            dest.write_all(chunk)
                .await
                .map_err(|_| AppError::new(ErrorCode::Internal, "the destination stopped"))?;
            sent += chunk.len();
            progress(sent as u64);
        }
        dest.shutdown().await.ok();
        Ok(())
    }

    async fn write_from(
        &self,
        path: &str,
        src: &mut (dyn AsyncRead + Send + Unpin),
        progress: Progress<'_>,
        cancel: CancellationToken,
    ) -> Result<(), AppError> {
        let mut data = Vec::new();
        let mut buffer = [0u8; 1024];
        loop {
            let read = tokio::select! {
                read = src.read(&mut buffer) => read.map_err(|_| AppError::new(ErrorCode::Internal, "read failed"))?,
                _ = cancel.cancelled() => {
                    return Err(AppError::new(ErrorCode::Cancelled, "cancelled"));
                }
            };
            if read == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..read]);
            progress(data.len() as u64);
            // The part file shows up as it grows, like a real one.
            self.files.lock().unwrap().insert(path.into(), data.clone());
        }
        self.put(path, &data);
        Ok(())
    }
}
