//! Files on this computer. Also used for a WSL distribution, through its
//! `\\wsl.localhost\<distro>\` path (W4).

use std::path::Path;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use super::backend::{COPY_BUFFER, FileBackend, FileCapabilities, FileEntry, FileKind, Progress};
use crate::error::{AppError, ErrorCode};

#[derive(Default)]
pub struct LocalFiles;

fn from_io(error: &std::io::Error, path: &str) -> AppError {
    use std::io::ErrorKind::*;
    match error.kind() {
        NotFound => AppError::new(ErrorCode::NotFound, format!("{path} wasn't found.")),
        PermissionDenied => AppError::new(
            ErrorCode::PermissionDenied,
            format!("You don't have permission to use {path}."),
        ),
        AlreadyExists => AppError::new(ErrorCode::AlreadyExists, format!("{path} already exists.")),
        _ => AppError::new(
            ErrorCode::Internal,
            format!("Couldn't use {path}: {}", plain(error)),
        ),
    }
}

/// The operating system's message without its code, which means nothing to most people.
fn plain(error: &std::io::Error) -> String {
    let text = error.to_string();
    match text.rsplit_once(" (os error") {
        Some((message, _)) => message.to_string(),
        None => text,
    }
}

fn seconds(time: std::io::Result<std::time::SystemTime>) -> Option<i64> {
    let time = time.ok()?;
    match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).ok(),
        Err(e) => i64::try_from(e.duration().as_secs()).ok().map(|s| -s),
    }
}

fn entry(path: &Path, name: String, meta: &std::fs::Metadata) -> FileEntry {
    let kind = if meta.is_symlink() {
        FileKind::Symlink
    } else if meta.is_dir() {
        FileKind::Dir
    } else if meta.is_file() {
        FileKind::File
    } else {
        FileKind::Other
    };
    #[cfg(unix)]
    let permissions = {
        use std::os::unix::fs::PermissionsExt;
        Some(meta.permissions().mode() & 0o7777)
    };
    #[cfg(not(unix))]
    let permissions = None;
    FileEntry {
        name,
        path: path.display().to_string(),
        kind,
        size: (kind == FileKind::File).then_some(meta.len()),
        modified: seconds(meta.modified()),
        permissions,
    }
}

fn cancelled() -> AppError {
    AppError::new(ErrorCode::Cancelled, "The transfer was cancelled.")
}

#[async_trait]
impl FileBackend for LocalFiles {
    fn capabilities(&self) -> FileCapabilities {
        FileCapabilities {
            chmod: cfg!(unix),
            root_delete: false,
        }
    }

    async fn list(&self, path: &str) -> Result<Vec<FileEntry>, AppError> {
        let mut reader = tokio::fs::read_dir(path)
            .await
            .map_err(|e| from_io(&e, path))?;
        let mut entries = Vec::new();
        while let Some(item) = reader.next_entry().await.map_err(|e| from_io(&e, path))? {
            let name = item.file_name().to_string_lossy().into_owned();
            // `symlink_metadata` so a link is reported as a link, not as what it points to.
            if let Ok(meta) = tokio::fs::symlink_metadata(item.path()).await {
                entries.push(entry(&item.path(), name, &meta));
            }
        }
        Ok(entries)
    }

    async fn stat(&self, path: &str) -> Result<FileEntry, AppError> {
        let meta = tokio::fs::symlink_metadata(path)
            .await
            .map_err(|e| from_io(&e, path))?;
        let name = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
        Ok(entry(Path::new(path), name, &meta))
    }

    async fn mkdir(&self, path: &str) -> Result<(), AppError> {
        tokio::fs::create_dir(path)
            .await
            .map_err(|e| from_io(&e, path))
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), AppError> {
        // Moving onto something that exists would silently replace it on most systems, so
        // that is refused here. Replacing is the caller's decision, made by removing first.
        if tokio::fs::symlink_metadata(to).await.is_ok() {
            return Err(from_io(
                &std::io::Error::from(std::io::ErrorKind::AlreadyExists),
                to,
            ));
        }
        tokio::fs::rename(from, to)
            .await
            .map_err(|e| from_io(&e, from))
    }

    async fn remove_file(&self, path: &str) -> Result<(), AppError> {
        tokio::fs::remove_file(path)
            .await
            .map_err(|e| from_io(&e, path))
    }

    async fn remove_dir(&self, path: &str) -> Result<(), AppError> {
        tokio::fs::remove_dir(path)
            .await
            .map_err(|e| from_io(&e, path))
    }

    async fn chmod(&self, path: &str, mode: u32) -> Result<(), AppError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & 0o7777))
                .await
                .map_err(|e| from_io(&e, path))
        }
        #[cfg(not(unix))]
        {
            let _ = (path, mode);
            Err(AppError::new(
                ErrorCode::Unsupported,
                "This system doesn't have Unix permissions.",
            ))
        }
    }

    async fn read_to(
        &self,
        path: &str,
        dest: &mut (dyn AsyncWrite + Send + Unpin),
        progress: Progress<'_>,
        cancel: CancellationToken,
    ) -> Result<(), AppError> {
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|e| from_io(&e, path))?;
        copy_bytes(&mut file, dest, progress, &cancel, path).await
    }

    async fn write_from(
        &self,
        path: &str,
        src: &mut (dyn AsyncRead + Send + Unpin),
        progress: Progress<'_>,
        cancel: CancellationToken,
    ) -> Result<(), AppError> {
        let mut file = tokio::fs::File::create(path)
            .await
            .map_err(|e| from_io(&e, path))?;
        copy_bytes(src, &mut file, progress, &cancel, path).await?;
        file.flush().await.map_err(|e| from_io(&e, path))
    }
}

/// Moves bytes from `src` to `dst`, reporting progress and stopping when cancelled.
pub(super) async fn copy_bytes(
    src: &mut (dyn AsyncRead + Send + Unpin),
    dst: &mut (dyn AsyncWrite + Send + Unpin),
    progress: Progress<'_>,
    cancel: &CancellationToken,
    path: &str,
) -> Result<(), AppError> {
    let mut buffer = vec![0u8; COPY_BUFFER];
    let mut total = 0u64;
    loop {
        let read = tokio::select! {
            read = src.read(&mut buffer) => read.map_err(|e| from_io(&e, path))?,
            _ = cancel.cancelled() => return Err(cancelled()),
        };
        if read == 0 {
            dst.shutdown().await.map_err(|e| from_io(&e, path))?;
            return Ok(());
        }
        tokio::select! {
            written = dst.write_all(&buffer[..read]) => written.map_err(|e| from_io(&e, path))?,
            _ = cancel.cancelled() => return Err(cancelled()),
        }
        total += read as u64;
        progress(total);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn p(dir: &tempfile::TempDir, name: &str) -> String {
        dir.path().join(name).display().to_string()
    }

    #[tokio::test]
    async fn lists_files_folders_and_sizes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let mut entries = LocalFiles
            .list(&dir.path().display().to_string())
            .await
            .unwrap();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "a.txt");
        assert_eq!(entries[0].kind, FileKind::File);
        assert_eq!(entries[0].size, Some(5));
        assert!(entries[0].modified.is_some());
        assert_eq!(entries[1].kind, FileKind::Dir);
        assert_eq!(entries[1].size, None);
        assert_eq!(entries[0].path, p(&dir, "a.txt"));
    }

    #[tokio::test]
    async fn a_missing_folder_is_not_found_in_plain_words() {
        let dir = tempfile::tempdir().unwrap();
        let err = LocalFiles.list(&p(&dir, "nope")).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
        assert!(!err.message.contains("os error"), "{}", err.message);
    }

    #[tokio::test]
    async fn stat_makes_folders_renames_and_removes() {
        let dir = tempfile::tempdir().unwrap();
        LocalFiles.mkdir(&p(&dir, "d")).await.unwrap();
        assert_eq!(
            LocalFiles.mkdir(&p(&dir, "d")).await.unwrap_err().code,
            ErrorCode::AlreadyExists
        );
        std::fs::write(dir.path().join("d").join("f"), "x").unwrap();

        // A folder with something in it can't be removed as a folder.
        assert!(LocalFiles.remove_dir(&p(&dir, "d")).await.is_err());
        LocalFiles.remove_file(&p(&dir, "d/f")).await.unwrap();
        LocalFiles.remove_dir(&p(&dir, "d")).await.unwrap();
        assert_eq!(
            LocalFiles.stat(&p(&dir, "d")).await.unwrap_err().code,
            ErrorCode::NotFound
        );

        std::fs::write(dir.path().join("one"), "1").unwrap();
        LocalFiles
            .rename(&p(&dir, "one"), &p(&dir, "two"))
            .await
            .unwrap();
        assert_eq!(
            LocalFiles.stat(&p(&dir, "two")).await.unwrap().size,
            Some(1)
        );
    }

    #[tokio::test]
    async fn rename_never_replaces_something_that_is_there() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), "A").unwrap();
        std::fs::write(dir.path().join("b"), "B").unwrap();
        let err = LocalFiles
            .rename(&p(&dir, "a"), &p(&dir, "b"))
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::AlreadyExists);
        assert_eq!(std::fs::read_to_string(dir.path().join("b")).unwrap(), "B");
    }

    #[tokio::test]
    async fn a_file_is_copied_in_full_with_progress_up_to_its_size() {
        let dir = tempfile::tempdir().unwrap();
        let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(dir.path().join("src"), &data).unwrap();

        let seen = AtomicU64::new(0);
        let progress = |n: u64| seen.store(n, Ordering::SeqCst);
        let (mut writer, mut reader) = tokio::io::duplex(8 * 1024);
        let source = p(&dir, "src");
        let reading = tokio::spawn(async move {
            LocalFiles
                .read_to(&source, &mut writer, &|_| {}, CancellationToken::new())
                .await
        });
        let target = p(&dir, "dst");
        LocalFiles
            .write_from(&target, &mut reader, &progress, CancellationToken::new())
            .await
            .unwrap();
        reading.await.unwrap().unwrap();
        assert_eq!(std::fs::read(dir.path().join("dst")).unwrap(), data);
        assert_eq!(seen.load(Ordering::SeqCst), data.len() as u64);
    }

    #[tokio::test]
    async fn writing_replaces_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f"), "a much longer old text").unwrap();
        let mut src: &[u8] = b"new";
        LocalFiles
            .write_from(&p(&dir, "f"), &mut src, &|_| {}, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("f")).unwrap(),
            "new"
        );
    }

    #[tokio::test]
    async fn cancelling_stops_a_copy() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("big"), vec![7u8; 1_000_000]).unwrap();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let (mut writer, _reader) = tokio::io::duplex(1024);
        let err = LocalFiles
            .read_to(&p(&dir, "big"), &mut writer, &|_| {}, cancel)
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::Cancelled);
    }

    #[tokio::test]
    async fn reading_a_missing_file_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let (mut writer, _reader) = tokio::io::duplex(1024);
        let err = LocalFiles
            .read_to(
                &p(&dir, "nope"),
                &mut writer,
                &|_| {},
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn permissions_can_be_read_and_changed_and_links_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("f"), "x").unwrap();
        assert!(LocalFiles.capabilities().chmod);
        LocalFiles.chmod(&p(&dir, "f"), 0o640).await.unwrap();
        assert_eq!(
            LocalFiles.stat(&p(&dir, "f")).await.unwrap().permissions,
            Some(0o640)
        );
        std::os::unix::fs::symlink(dir.path().join("f"), dir.path().join("link")).unwrap();
        assert_eq!(
            LocalFiles.stat(&p(&dir, "link")).await.unwrap().kind,
            FileKind::Symlink
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_has_no_chmod_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!LocalFiles.capabilities().chmod);
        let err = LocalFiles.chmod(&p(&dir, "x"), 0o600).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::Unsupported);
    }
}
