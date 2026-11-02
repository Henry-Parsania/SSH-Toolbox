use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tauri::State;
use tauri::ipc::Channel;
use toolbox_core::files::PendingAnswers;
use toolbox_core::files::{
    CopyRequest, FileBackend, FileEntry, FileKind, FtpFiles, FtpTarget, JobSnapshot, LocalFiles,
    OverwriteChoice, SftpFiles, TransferQueue, check_remote_name, plan_delete, plan_tree,
    queue_tree, run_delete,
};
use toolbox_core::prompt::Prompter;
use toolbox_core::sessions::{SavedSession, SessionStore};
use toolbox_core::ssh::backend::SshServices;
use toolbox_core::terminal::TerminalStatus;
use toolbox_core::{AppError, ErrorCode};

use super::connect::shared_connection;
use super::keys::{KeySetups, QuestionSink};

/// Open file backends (this computer, and servers), by ID, and the transfer queue.
#[derive(Default)]
pub struct Files {
    backends: Mutex<HashMap<String, Arc<dyn FileBackend>>>,
    answers: Arc<PendingAnswers>,
    subscriber: Arc<Mutex<Option<Channel<Vec<JobSnapshot>>>>>,
    queue: OnceLock<TransferQueue>,
}

impl Files {
    pub fn backend(&self, id: &str) -> Result<Arc<dyn FileBackend>, AppError> {
        self.backends
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| {
                AppError::new(ErrorCode::NotFound, "That file panel isn't open any more.")
            })
    }

    /// The queue starts on first use, when there is an async runtime to run it on.
    fn queue(&self) -> &TransferQueue {
        self.queue.get_or_init(|| {
            let subscriber = self.subscriber.clone();
            let queue_cell: Arc<OnceLock<TransferQueue>> = Arc::new(OnceLock::new());
            let for_callback = queue_cell.clone();
            let queue = TransferQueue::new(self.answers.clone(), move || {
                if let (Some(channel), Some(queue)) =
                    (subscriber.lock().unwrap().as_ref(), for_callback.get())
                {
                    let _ = channel.send(queue.snapshot());
                }
            });
            let _ = queue_cell.set(queue.clone());
            queue
        })
    }
}

#[derive(Serialize)]
pub struct OpenedFiles {
    pub id: String,
    /// Where to start looking.
    pub home: String,
    pub chmod: bool,
    /// Whether "delete as root" is offered.
    pub root_delete: bool,
    /// `/` for servers; `\` or `/` for this computer.
    pub separator: String,
}

fn register(
    files: &Files,
    backend: Arc<dyn FileBackend>,
    home: String,
    separator: &str,
) -> OpenedFiles {
    let id = uuid::Uuid::new_v4().to_string();
    let caps = backend.capabilities();
    files.backends.lock().unwrap().insert(id.clone(), backend);
    OpenedFiles {
        id,
        home,
        chmod: caps.chmod,
        root_delete: caps.root_delete,
        separator: separator.to_string(),
    }
}

/// The files of this computer.
#[tauri::command]
pub async fn files_open_local(files: State<'_, Files>) -> Result<OpenedFiles, AppError> {
    let home = toolbox_core::config::home_dir()
        .map(|d| d.display().to_string())
        .ok_or_else(|| AppError::new(ErrorCode::NotFound, "Couldn't find your home folder."))?;
    let separator = if cfg!(windows) { "\\" } else { "/" };
    Ok(register(&files, Arc::new(LocalFiles), home, separator))
}

/// The files of a server, over SFTP on the session's connection (connecting if needed).
#[tauri::command]
pub async fn files_open_sftp(
    files: State<'_, Files>,
    setups: State<'_, KeySetups>,
    services: State<'_, Arc<SshServices>>,
    operation_id: String,
    saved_session_id: String,
    on_status: Channel<TerminalStatus>,
) -> Result<OpenedFiles, AppError> {
    let shared = shared_connection(
        &setups,
        &services,
        &operation_id,
        &saved_session_id,
        on_status,
    )
    .await?;
    let sftp = SftpFiles::open(shared).await?;
    let home = sftp.home().await.unwrap_or_else(|_| "/".into());
    Ok(register(&files, Arc::new(sftp), home, "/"))
}

/// The files of an FTP or FTPS session. Plain FTP is warned about, and a certificate that doesn't
/// check out is asked about, through `on_status`. A certificate the user accepts is saved with
/// the session, so that it is only asked about again if it changes (SEC6, SEC8).
#[tauri::command]
pub async fn files_open_ftp(
    files: State<'_, Files>,
    setups: State<'_, KeySetups>,
    services: State<'_, Arc<SshServices>>,
    operation_id: String,
    saved_session_id: String,
    on_status: Channel<TerminalStatus>,
) -> Result<OpenedFiles, AppError> {
    let cancel = tokio_util::sync::CancellationToken::new();
    setups
        .running
        .lock()
        .unwrap()
        .insert(operation_id.clone(), cancel.clone());
    let result = async {
        let path = services.sessions_file.clone();
        let loaded = tokio::task::spawn_blocking({
            let path = path.clone();
            move || SessionStore::new(path).load()
        })
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't read the saved sessions."))?;
        if let Some(problem) = loaded.problem {
            return Err(AppError::new(ErrorCode::InvalidInput, problem));
        }
        let Some(SavedSession::Ftp {
            host,
            port,
            user,
            security,
            trusted_cert_sha256,
            ..
        }) = loaded
            .file
            .sessions
            .iter()
            .find(|s| s.id() == saved_session_id)
            .cloned()
        else {
            return Err(AppError::new(
                ErrorCode::NotFound,
                "That isn't a saved FTP session.",
            ));
        };
        let target = FtpTarget {
            session_id: saved_session_id.clone(),
            host,
            port,
            user,
            security,
            trusted_cert_sha256,
        };
        let prompter = Prompter::new(
            services.broker.clone(),
            Arc::new(QuestionSink { status: on_status }),
            cancel.clone(),
        );
        let connecting = FtpFiles::connect(&target, &services.keychain, &prompter);
        let opened = tokio::select! {
            opened = connecting => opened?,
            _ = cancel.cancelled() => return Err(AppError::new(ErrorCode::Cancelled, "Cancelled.")),
        };
        if let Some(fingerprint) = opened.accepted_cert {
            // Remembering it is a convenience: if the file can't be written, the user is
            // simply asked again next time.
            let id = saved_session_id.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let store = SessionStore::new(path);
                let mut loaded = store.load();
                if loaded.problem.is_some() {
                    return;
                }
                for session in &mut loaded.file.sessions {
                    if let SavedSession::Ftp {
                        id: sid,
                        trusted_cert_sha256,
                        ..
                    } = session
                        && *sid == id
                    {
                        *trusted_cert_sha256 = fingerprint.clone();
                    }
                }
                let _ = store.save(&loaded.file);
            })
            .await;
        }
        let home = opened.files.home().await.unwrap_or_else(|_| "/".into());
        Ok(register(&files, Arc::new(opened.files), home, "/"))
    }
    .await;
    setups.running.lock().unwrap().remove(&operation_id);
    result
}

/// The files inside a WSL distribution, through its \\wsl.localhost\<distro>\ folder (W4).
#[tauri::command]
pub async fn files_open_wsl(
    files: State<'_, Files>,
    distro: String,
) -> Result<OpenedFiles, AppError> {
    // The name becomes part of a path, so it must be a plain distribution name.
    toolbox_core::terminal::wsl_spec(&distro)?;
    if distro.contains(['\\', '/', ':']) {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "That isn't a valid WSL distribution name.",
        ));
    }
    if !cfg!(windows) {
        return Err(AppError::new(
            ErrorCode::Unsupported,
            "WSL files are only available on Windows.",
        ));
    }
    let root = format!("\\\\wsl.localhost\\{distro}\\");
    Ok(register(&files, Arc::new(LocalFiles), root, "\\"))
}

#[tauri::command]
pub async fn files_close(files: State<'_, Files>, id: String) -> Result<(), AppError> {
    files.backends.lock().unwrap().remove(&id);
    Ok(())
}

#[tauri::command]
pub async fn files_list(
    files: State<'_, Files>,
    id: String,
    path: String,
) -> Result<Vec<FileEntry>, AppError> {
    files.backend(&id)?.list(&path).await
}

#[tauri::command]
pub async fn files_mkdir(
    files: State<'_, Files>,
    id: String,
    path: String,
) -> Result<(), AppError> {
    files.backend(&id)?.mkdir(&path).await
}

#[tauri::command]
pub async fn files_rename(
    files: State<'_, Files>,
    id: String,
    from: String,
    to: String,
) -> Result<(), AppError> {
    files.backend(&id)?.rename(&from, &to).await
}

#[tauri::command]
pub async fn files_chmod(
    files: State<'_, Files>,
    id: String,
    path: String,
    mode: u32,
) -> Result<(), AppError> {
    files.backend(&id)?.chmod(&path, mode).await
}

#[derive(Serialize)]
pub struct DeleteSummary {
    pub files: usize,
    pub folders: usize,
}

/// What deleting would remove, so the confirmation can say how much.
#[tauri::command]
pub async fn files_delete_plan(
    files: State<'_, Files>,
    id: String,
    path: String,
) -> Result<DeleteSummary, AppError> {
    let backend = files.backend(&id)?;
    let entry = backend.stat(&path).await?;
    if entry.kind == FileKind::Dir {
        let plan = plan_delete(&backend, &path).await?;
        Ok(DeleteSummary {
            files: plan.files,
            folders: plan.folders,
        })
    } else {
        Ok(DeleteSummary {
            files: 1,
            folders: 0,
        })
    }
}

/// Deletes a file, or a folder with everything in it. The interface asks first.
#[tauri::command]
pub async fn files_delete(
    files: State<'_, Files>,
    id: String,
    path: String,
) -> Result<(), AppError> {
    let backend = files.backend(&id)?;
    let entry = backend.stat(&path).await?;
    if entry.kind == FileKind::Dir {
        let plan = plan_delete(&backend, &path).await?;
        run_delete(&backend, &plan, &tokio_util::sync::CancellationToken::new()).await
    } else {
        backend.remove_file(&path).await
    }
}

/// Deletes as the server's administrator with `sudo`. `password` is the sudo password, or empty
/// if none is needed. It is passed on and forgotten: never stored, logged or returned.
#[tauri::command]
pub async fn files_delete_as_root(
    files: State<'_, Files>,
    id: String,
    path: String,
    password: Option<String>,
) -> Result<(), AppError> {
    let backend = files.backend(&id)?;
    backend.remove_as_root(&path, password.as_deref()).await
}

#[derive(Serialize)]
pub struct Added {
    pub queued: usize,
    /// Entries left out of a folder copy, each with the reason.
    pub skipped: Vec<String>,
}

fn join_for(dir: &str, name: &str) -> String {
    if dir.ends_with('/') || dir.ends_with('\\') {
        format!("{dir}{name}")
    } else if dir.contains('\\') && !dir.contains('/') {
        format!("{dir}\\{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// Puts a file or folder in the queue: copy `from_path` on one side into the folder `to_dir` on
/// the other.
#[tauri::command]
pub async fn transfer_add(
    files: State<'_, Files>,
    from_id: String,
    from_path: String,
    to_id: String,
    to_dir: String,
) -> Result<Added, AppError> {
    let source = files.backend(&from_id)?;
    let target = files.backend(&to_id)?;
    let entry = source.stat(&from_path).await?;
    // The name comes from a listing, which a server wrote.
    check_remote_name(&entry.name).map_err(|why| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!("That name can't be used: {why}."),
        )
    })?;
    let to = join_for(&to_dir, &entry.name);
    let queue = files.queue();

    if entry.kind == FileKind::Dir {
        let plan = plan_tree(&source, &from_path, &to).await?;
        let ids = queue_tree(queue, &source, &target, &plan, &from_path);
        Ok(Added {
            queued: ids.len(),
            skipped: plan.skipped,
        })
    } else {
        queue.add_copy(CopyRequest {
            source,
            from: from_path,
            target,
            to,
            size: entry.size,
        });
        Ok(Added {
            queued: 1,
            skipped: vec![],
        })
    }
}

/// The interface wants to be told whenever the queue changes. It gets the whole list each time.
#[tauri::command]
pub async fn transfers_subscribe(
    files: State<'_, Files>,
    on_change: Channel<Vec<JobSnapshot>>,
) -> Result<(), AppError> {
    let queue = files.queue();
    let _ = on_change.send(queue.snapshot());
    *files.subscriber.lock().unwrap() = Some(on_change);
    Ok(())
}

#[tauri::command]
pub async fn transfer_cancel(files: State<'_, Files>, id: u64) -> Result<(), AppError> {
    files.queue().cancel(id);
    Ok(())
}

#[tauri::command]
pub async fn transfer_cancel_all(files: State<'_, Files>) -> Result<(), AppError> {
    files.queue().cancel_all();
    Ok(())
}

#[tauri::command]
pub async fn transfer_clear(files: State<'_, Files>) -> Result<(), AppError> {
    files.queue().clear_finished();
    Ok(())
}

#[tauri::command]
pub async fn transfer_answer(
    files: State<'_, Files>,
    job_id: u64,
    choice: OverwriteChoice,
) -> Result<(), AppError> {
    files.answers.respond(job_id, choice)
}
