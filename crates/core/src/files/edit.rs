//! Editing a file from a server on this computer: it is downloaded to a private temporary
//! folder, opened in the user's own program, and watched. When it is saved, the page is told, and
//! sends it back when the user agrees (or has said it always may).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use super::backend::{FileBackend, FileKind};
use crate::error::{AppError, ErrorCode};

/// Larger files are not opened this way.
pub const MAX_EDIT_BYTES: u64 = 50 * 1024 * 1024;

/// How often the local copy is looked at.
const POLL: Duration = Duration::from_millis(600);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EditEvent {
    /// The local copy was saved and is not being changed any more.
    Changed,
    /// The local copy was deleted or can't be read, so nothing is watched any more.
    Gone,
}

#[derive(Debug, Clone, Serialize)]
pub struct EditInfo {
    pub id: String,
    pub name: String,
    pub local_path: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UploadResult {
    Uploaded,
    /// The file on the server changed after it was downloaded. Nothing was sent.
    RemoteChanged,
}

struct Entry {
    local: PathBuf,
    remote: String,
    backend: Arc<dyn FileBackend>,
    /// When the server's copy was last changed, as far as we know.
    remote_modified: Mutex<Option<i64>>,
    cancel: CancellationToken,
}

/// The files being edited, with the folder their local copies are in.
pub struct EditSessions {
    root: PathBuf,
    entries: Mutex<HashMap<String, Arc<Entry>>>,
}

/// Programs and scripts that opening could run. These are not opened with the default action.
pub fn is_risky_to_open(name: &str) -> bool {
    const RISKY: &[&str] = &[
        "exe",
        "com",
        "bat",
        "cmd",
        "scr",
        "msi",
        "msp",
        "pif",
        "ps1",
        "psm1",
        "vbs",
        "vbe",
        "js",
        "jse",
        "wsf",
        "wsh",
        "hta",
        "cpl",
        "msc",
        "lnk",
        "reg",
        "dll",
        "jar",
        "url",
        "scf",
        "inf",
        "application",
        "gadget",
        "desktop",
        "appimage",
    ];
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| RISKY.contains(&ext.to_ascii_lowercase().as_str()))
}

/// A file name that is safe to create in the temporary folder: just the last part of the path,
/// with anything odd replaced.
fn safe_name(remote: &str) -> String {
    let last = remote.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = last
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim_matches(|c| c == '.' || c == ' ').to_string();
    if clean.is_empty() {
        "file".into()
    } else {
        clean
    }
}

fn internal(what: &str) -> AppError {
    AppError::new(ErrorCode::Internal, what.to_string())
}

fn stamp(path: &Path) -> Option<(u64, SystemTime)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

impl EditSessions {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            entries: Mutex::default(),
        }
    }

    /// Removes copies left behind by an earlier run that didn't end normally.
    pub fn clean_leftovers(&self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }

    fn entry(&self, id: &str) -> Result<Arc<Entry>, AppError> {
        self.entries
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::NotFound,
                    "That file isn't being edited any more.",
                )
            })
    }

    /// Downloads `remote` and starts watching the copy. `notify` is called from a background task
    /// each time the copy is saved.
    pub async fn open(
        &self,
        backend: Arc<dyn FileBackend>,
        remote: &str,
        notify: impl Fn(EditEvent) + Send + Sync + 'static,
    ) -> Result<EditInfo, AppError> {
        let info = backend.stat(remote).await?;
        if info.kind != FileKind::File {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "Only files can be opened this way.",
            ));
        }
        if info.size.is_some_and(|s| s > MAX_EDIT_BYTES) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "That file is too large to open this way (over 50 MB). Transfer it instead.",
            ));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let name = safe_name(remote);
        let folder = self.root.join(&id);
        std::fs::create_dir_all(&folder)
            .map_err(|_| internal("Couldn't make a place for the file."))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let private = std::fs::Permissions::from_mode(0o700);
            let _ = std::fs::set_permissions(&self.root, private.clone());
            let _ = std::fs::set_permissions(&folder, private);
        }
        let local = folder.join(&name);

        let downloaded = async {
            let mut out = tokio::fs::File::create(&local)
                .await
                .map_err(|_| internal("Couldn't save the file here."))?;
            backend
                .read_to(remote, &mut out, &|_| {}, CancellationToken::new())
                .await?;
            out.flush()
                .await
                .map_err(|_| internal("Couldn't save the file here."))?;
            Ok::<(), AppError>(())
        }
        .await;
        if let Err(error) = downloaded {
            let _ = std::fs::remove_dir_all(&folder);
            return Err(error);
        }

        let entry = Arc::new(Entry {
            local: local.clone(),
            remote: remote.to_string(),
            backend,
            remote_modified: Mutex::new(info.modified),
            cancel: CancellationToken::new(),
        });
        self.entries
            .lock()
            .unwrap()
            .insert(id.clone(), entry.clone());
        tokio::spawn(watch(entry, notify));
        Ok(EditInfo {
            id,
            name,
            local_path: local.display().to_string(),
        })
    }

    /// Sends the local copy back. If the server's copy changed since it was downloaded, nothing
    /// is sent unless `force` is set.
    pub async fn upload(&self, id: &str, force: bool) -> Result<UploadResult, AppError> {
        let entry = self.entry(id)?;
        let known = *entry.remote_modified.lock().unwrap();
        if !force && let Some(known) = known {
            let now = entry.backend.stat(&entry.remote).await?;
            if now.modified.is_some_and(|m| m != known) {
                return Ok(UploadResult::RemoteChanged);
            }
        }
        let mut file = tokio::fs::File::open(&entry.local)
            .await
            .map_err(|_| AppError::new(ErrorCode::NotFound, "The local copy is gone."))?;
        entry
            .backend
            .write_from(&entry.remote, &mut file, &|_| {}, CancellationToken::new())
            .await?;
        // Remember the server's new time, so the next save isn't taken for someone else's change.
        if let Ok(after) = entry.backend.stat(&entry.remote).await {
            *entry.remote_modified.lock().unwrap() = after.modified;
        }
        Ok(UploadResult::Uploaded)
    }

    /// The local copy's path, to open it again.
    pub fn local_path(&self, id: &str) -> Result<PathBuf, AppError> {
        Ok(self.entry(id)?.local.clone())
    }

    /// Stops watching and deletes the local copy.
    pub fn stop(&self, id: &str) {
        if let Some(entry) = self.entries.lock().unwrap().remove(id) {
            entry.cancel.cancel();
            if let Some(folder) = entry.local.parent() {
                let _ = std::fs::remove_dir_all(folder);
            }
        }
    }

    /// Stops everything and deletes every local copy, for when the app quits.
    pub fn stop_all(&self) {
        let ids: Vec<String> = self.entries.lock().unwrap().keys().cloned().collect();
        for id in ids {
            self.stop(&id);
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Looks at the local copy and reports each save once the file has stopped changing.
async fn watch(entry: Arc<Entry>, notify: impl Fn(EditEvent) + Send + Sync + 'static) {
    let mut reported = stamp(&entry.local);
    let mut seen = reported;
    loop {
        tokio::select! {
            _ = entry.cancel.cancelled() => return,
            _ = tokio::time::sleep(POLL) => {}
        }
        let Some(now) = stamp(&entry.local) else {
            // Some programs replace the file by deleting it and writing it again: give them a moment.
            tokio::time::sleep(POLL * 2).await;
            if stamp(&entry.local).is_none() {
                notify(EditEvent::Gone);
                return;
            }
            continue;
        };
        if Some(now) != seen {
            seen = Some(now); // Still changing. Look again before saying anything.
        } else if Some(now) != reported {
            reported = Some(now);
            notify(EditEvent::Changed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::LocalFiles;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn programs_and_scripts_are_not_opened_directly() {
        for name in [
            "a.exe",
            "B.BAT",
            "x.ps1",
            "run.js",
            "l.lnk",
            "x.desktop",
            "a.AppImage",
        ] {
            assert!(is_risky_to_open(name), "{name}");
        }
        for name in [
            "compose.yaml",
            "notes.txt",
            "a.sh",
            "Makefile",
            "x.json",
            "exe",
        ] {
            assert!(!is_risky_to_open(name), "{name}");
        }
    }

    #[test]
    fn the_local_name_is_just_the_file_name() {
        assert_eq!(safe_name("/etc/app/compose.yaml"), "compose.yaml");
        assert_eq!(safe_name("/a/../../b:c*.txt"), "b_c_.txt");
        assert_eq!(safe_name("/a/.."), "file");
        assert_eq!(safe_name("C:\\x\\y.txt"), "y.txt");
    }

    struct World {
        _dir: tempfile::TempDir,
        server_file: PathBuf,
        edits: EditSessions,
        backend: Arc<dyn FileBackend>,
        changed: Arc<AtomicUsize>,
    }

    fn world(content: &str) -> World {
        let dir = tempfile::tempdir().unwrap();
        let server_file = dir.path().join("server").join("compose.yaml");
        std::fs::create_dir_all(server_file.parent().unwrap()).unwrap();
        std::fs::write(&server_file, content).unwrap();
        World {
            edits: EditSessions::new(dir.path().join("edits")),
            server_file,
            _dir: dir,
            backend: Arc::new(LocalFiles),
            changed: Arc::default(),
        }
    }

    async fn open(w: &World) -> EditInfo {
        let changed = w.changed.clone();
        w.edits
            .open(
                w.backend.clone(),
                &w.server_file.display().to_string(),
                move |e| {
                    if e == EditEvent::Changed {
                        changed.fetch_add(1, Ordering::SeqCst);
                    }
                },
            )
            .await
            .unwrap()
    }

    async fn until(what: &str, mut ok: impl FnMut() -> bool) {
        for _ in 0..200 {
            if ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("timed out waiting for: {what}");
    }

    #[tokio::test]
    async fn a_saved_change_is_reported_once_and_can_be_sent_back() {
        let w = world("services: {}\n");
        let info = open(&w).await;
        assert_eq!(
            std::fs::read_to_string(&info.local_path).unwrap(),
            "services: {}\n"
        );
        assert_eq!(info.name, "compose.yaml");

        // Nothing is reported until the file is saved.
        tokio::time::sleep(Duration::from_millis(900)).await;
        assert_eq!(w.changed.load(Ordering::SeqCst), 0);

        std::fs::write(&info.local_path, "services:\n  web: {}\n").unwrap();
        until("the save is reported", || {
            w.changed.load(Ordering::SeqCst) == 1
        })
        .await;
        tokio::time::sleep(Duration::from_millis(1000)).await;
        assert_eq!(w.changed.load(Ordering::SeqCst), 1, "reported once");

        assert_eq!(
            w.edits.upload(&info.id, false).await.unwrap(),
            UploadResult::Uploaded
        );
        assert_eq!(
            std::fs::read_to_string(&w.server_file).unwrap(),
            "services:\n  web: {}\n"
        );

        // A second save is reported again.
        std::fs::write(&info.local_path, "services:\n  db: {}\n").unwrap();
        until("the second save", || w.changed.load(Ordering::SeqCst) == 2).await;
        assert_eq!(
            w.edits.upload(&info.id, false).await.unwrap(),
            UploadResult::Uploaded
        );
        assert_eq!(
            std::fs::read_to_string(&w.server_file).unwrap(),
            "services:\n  db: {}\n"
        );
        w.edits.stop_all();
    }

    #[tokio::test]
    async fn a_file_changed_on_the_server_is_not_overwritten_without_asking() {
        let w = world("one\n");
        let info = open(&w).await;
        std::fs::write(&info.local_path, "mine\n").unwrap();

        // Someone else changes the server's copy, with a different time.
        tokio::time::sleep(Duration::from_millis(1200)).await;
        std::fs::write(&w.server_file, "theirs\n").unwrap();

        assert_eq!(
            w.edits.upload(&info.id, false).await.unwrap(),
            UploadResult::RemoteChanged
        );
        assert_eq!(std::fs::read_to_string(&w.server_file).unwrap(), "theirs\n");
        assert_eq!(
            w.edits.upload(&info.id, true).await.unwrap(),
            UploadResult::Uploaded
        );
        assert_eq!(std::fs::read_to_string(&w.server_file).unwrap(), "mine\n");
        w.edits.stop_all();
    }

    #[tokio::test]
    async fn stopping_deletes_the_local_copy() {
        let w = world("x\n");
        let info = open(&w).await;
        assert!(Path::new(&info.local_path).exists());
        w.edits.stop(&info.id);
        assert!(!Path::new(&info.local_path).exists());
        assert!(w.edits.upload(&info.id, false).await.is_err());
    }

    #[tokio::test]
    async fn folders_and_missing_files_are_refused() {
        let w = world("x\n");
        let dir = w.server_file.parent().unwrap().display().to_string();
        let refused = w.edits.open(w.backend.clone(), &dir, |_| {}).await;
        assert_eq!(refused.unwrap_err().code, ErrorCode::InvalidInput);
        let missing = w
            .edits
            .open(w.backend.clone(), "/no/such/file", |_| {})
            .await;
        assert!(missing.is_err());
    }

    #[tokio::test]
    async fn leftovers_from_an_earlier_run_are_cleaned() {
        let w = world("x\n");
        let info = open(&w).await;
        let edits2 = EditSessions::new(w.edits.root.clone());
        edits2.clean_leftovers();
        assert!(!Path::new(&info.local_path).exists());
        w.edits.stop_all();
    }
}
