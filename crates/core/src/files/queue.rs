//! The transfer queue (INTERFACES.md section 6, F4).
//!
//! A transfer writes to `<name>.sshtoolbox-part` and renames it when it is complete, so a
//! failed or cancelled transfer never damages a file that was already there. If the target
//! exists the user is asked first. The queue works on any two `FileBackend`s.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::backend::{FileBackend, FileEntry, FileKind};
use super::names::check_remote_name;
use crate::error::{AppError, ErrorCode};

pub const PART_SUFFIX: &str = ".sshtoolbox-part";
const PIPE_SIZE: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverwriteChoice {
    Overwrite,
    Skip,
    /// Overwrite this one and every later one until the queue is empty.
    OverwriteAll,
    /// Skip this one and every later one until the queue is empty.
    SkipAll,
}

/// Asks the user what to do about a file that already exists.
#[async_trait]
pub trait OverwriteResolver: Send + Sync {
    async fn resolve(&self, job: &JobSnapshot, existing: &FileEntry) -> OverwriteChoice;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum JobState {
    Queued,
    /// Waiting for the user to say what to do about an existing file.
    Asking,
    Running,
    Done,
    Skipped,
    Cancelled,
    Failed {
        error: AppError,
    },
}

impl JobState {
    fn finished(&self) -> bool {
        !matches!(self, Self::Queued | Self::Asking | Self::Running)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Copy,
    /// Makes a folder at the destination (part of copying a folder).
    Folder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JobSnapshot {
    pub id: u64,
    pub kind: JobKind,
    pub name: String,
    pub from: String,
    pub to: String,
    pub state: JobState,
    pub done: u64,
    pub total: Option<u64>,
}

struct Job {
    snapshot: JobSnapshot,
    source: Arc<dyn FileBackend>,
    target: Arc<dyn FileBackend>,
    cancel: CancellationToken,
}

struct Inner {
    jobs: Mutex<Vec<Job>>,
    wake: Notify,
    resolver: Arc<dyn OverwriteResolver>,
    on_change: Box<dyn Fn() + Send + Sync>,
    /// Set by "all" answers, and cleared when the queue empties.
    policy: Mutex<Option<OverwriteChoice>>,
    next_id: AtomicU64,
}

#[derive(Clone)]
pub struct TransferQueue {
    inner: Arc<Inner>,
}

/// What to copy.
pub struct CopyRequest {
    pub source: Arc<dyn FileBackend>,
    pub from: String,
    pub target: Arc<dyn FileBackend>,
    pub to: String,
    pub size: Option<u64>,
}

fn last_part(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

impl TransferQueue {
    /// Starts the queue. `on_change` is called whenever a job is added or changes, so the
    /// interface can refresh its list.
    pub fn new(
        resolver: Arc<dyn OverwriteResolver>,
        on_change: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let inner = Arc::new(Inner {
            jobs: Mutex::new(Vec::new()),
            wake: Notify::new(),
            resolver,
            on_change: Box::new(on_change),
            policy: Mutex::new(None),
            next_id: AtomicU64::new(1),
        });
        tokio::spawn(worker(inner.clone()));
        Self { inner }
    }

    fn push(&self, kind: JobKind, request: CopyRequest) -> u64 {
        let id = self.inner.next_id.fetch_add(1, Ordering::SeqCst);
        let job = Job {
            snapshot: JobSnapshot {
                id,
                kind,
                name: last_part(&request.to),
                from: request.from,
                to: request.to,
                state: JobState::Queued,
                done: 0,
                total: request.size,
            },
            source: request.source,
            target: request.target,
            cancel: CancellationToken::new(),
        };
        self.inner.jobs.lock().unwrap().push(job);
        (self.inner.on_change)();
        self.inner.wake.notify_one();
        id
    }

    pub fn add_copy(&self, request: CopyRequest) -> u64 {
        self.push(JobKind::Copy, request)
    }

    pub fn add_folder(&self, request: CopyRequest) -> u64 {
        self.push(JobKind::Folder, request)
    }

    pub fn snapshot(&self) -> Vec<JobSnapshot> {
        self.inner
            .jobs
            .lock()
            .unwrap()
            .iter()
            .map(|j| j.snapshot.clone())
            .collect()
    }

    /// Cancels a job that is waiting or running. A finished job is left as it is.
    pub fn cancel(&self, id: u64) {
        for job in self.inner.jobs.lock().unwrap().iter() {
            if job.snapshot.id == id && !job.snapshot.state.finished() {
                job.cancel.cancel();
            }
        }
        self.inner.wake.notify_one();
    }

    pub fn cancel_all(&self) {
        for job in self.inner.jobs.lock().unwrap().iter() {
            if !job.snapshot.state.finished() {
                job.cancel.cancel();
            }
        }
        self.inner.wake.notify_one();
    }

    /// Removes finished jobs from the list.
    pub fn clear_finished(&self) {
        self.inner
            .jobs
            .lock()
            .unwrap()
            .retain(|j| !j.snapshot.state.finished());
        (self.inner.on_change)();
    }
}

fn update(inner: &Inner, id: u64, change: impl FnOnce(&mut JobSnapshot)) {
    if let Some(job) = inner
        .jobs
        .lock()
        .unwrap()
        .iter_mut()
        .find(|j| j.snapshot.id == id)
    {
        change(&mut job.snapshot);
    }
    (inner.on_change)();
}

async fn worker(inner: Arc<Inner>) {
    loop {
        // The next job that hasn't started. A job cancelled while it waits is closed here.
        let next = {
            let mut jobs = inner.jobs.lock().unwrap();
            let mut found = None;
            for job in jobs.iter_mut() {
                if job.snapshot.state == JobState::Queued {
                    if job.cancel.is_cancelled() {
                        job.snapshot.state = JobState::Cancelled;
                        continue;
                    }
                    found = Some((
                        job.snapshot.clone(),
                        job.source.clone(),
                        job.target.clone(),
                        job.cancel.clone(),
                    ));
                    break;
                }
            }
            found
        };
        let Some((snapshot, source, target, cancel)) = next else {
            // Nothing left: the "all" answers end here.
            *inner.policy.lock().unwrap() = None;
            (inner.on_change)();
            inner.wake.notified().await;
            continue;
        };

        let id = snapshot.id;
        let state = run_job(&inner, snapshot, source, target, cancel).await;
        update(&inner, id, |s| s.state = state);
    }
}

async fn run_job(
    inner: &Arc<Inner>,
    snapshot: JobSnapshot,
    source: Arc<dyn FileBackend>,
    target: Arc<dyn FileBackend>,
    cancel: CancellationToken,
) -> JobState {
    let id = snapshot.id;
    let result = match snapshot.kind {
        JobKind::Folder => make_folder(&target, &snapshot.to).await,
        JobKind::Copy => copy_file(inner, &snapshot, &source, &target, &cancel).await,
    };
    let _ = id;
    match result {
        Ok(outcome) => outcome,
        Err(e) if e.code == ErrorCode::Cancelled => JobState::Cancelled,
        Err(error) => JobState::Failed { error },
    }
}

async fn make_folder(target: &Arc<dyn FileBackend>, path: &str) -> Result<JobState, AppError> {
    match target.mkdir(path).await {
        Ok(()) => Ok(JobState::Done),
        // A folder that is already there is what was wanted.
        Err(e) if e.code == ErrorCode::AlreadyExists => Ok(JobState::Done),
        Err(e) => Err(e),
    }
}

async fn copy_file(
    inner: &Arc<Inner>,
    snapshot: &JobSnapshot,
    source: &Arc<dyn FileBackend>,
    target: &Arc<dyn FileBackend>,
    cancel: &CancellationToken,
) -> Result<JobState, AppError> {
    let id = snapshot.id;

    // Is something already there?
    match target.stat(&snapshot.to).await {
        Ok(existing) if existing.kind == FileKind::Dir => {
            return Err(AppError::new(
                ErrorCode::AlreadyExists,
                format!("A folder named {} is already there.", snapshot.name),
            ));
        }
        Ok(existing) => {
            let remembered = *inner.policy.lock().unwrap();
            let choice = match remembered {
                Some(choice) => choice,
                None => {
                    update(inner, id, |s| s.state = JobState::Asking);
                    let asked = tokio::select! {
                        choice = inner.resolver.resolve(snapshot, &existing) => choice,
                        _ = cancel.cancelled() => return Err(cancelled()),
                    };
                    if matches!(
                        asked,
                        OverwriteChoice::OverwriteAll | OverwriteChoice::SkipAll
                    ) {
                        *inner.policy.lock().unwrap() = Some(asked);
                    }
                    asked
                }
            };
            if matches!(choice, OverwriteChoice::Skip | OverwriteChoice::SkipAll) {
                return Ok(JobState::Skipped);
            }
        }
        Err(e) if e.code == ErrorCode::NotFound => {}
        Err(e) => return Err(e),
    }

    update(inner, id, |s| s.state = JobState::Running);
    let part = format!("{}{PART_SUFFIX}", snapshot.to);

    // Read and write run side by side, joined by a pipe.
    let (mut writer, mut reader) = tokio::io::duplex(PIPE_SIZE);
    let stop = cancel.child_token();
    let progress = {
        let inner = inner.clone();
        move |done: u64| update(&inner, id, |s| s.done = done)
    };
    let read = {
        let (source, from, stop) = (source.clone(), snapshot.from.clone(), stop.clone());
        async move {
            let result = source
                .read_to(&from, &mut writer, &|_| {}, stop.clone())
                .await;
            if result.is_err() {
                stop.cancel(); // Don't leave the writing side waiting.
            }
            result
        }
    };
    let write = {
        let (target, part, stop) = (target.clone(), part.clone(), stop.clone());
        async move {
            let result = target
                .write_from(&part, &mut reader, &progress, stop.clone())
                .await;
            if result.is_err() {
                stop.cancel();
            }
            result
        }
    };
    let (read, write) = tokio::join!(read, write);

    // The first real error is the one to report. A cancel from the user is reported as such.
    let outcome = if cancel.is_cancelled() {
        Err(cancelled())
    } else {
        read.and(write)
    };
    if let Err(error) = outcome {
        let _ = target.remove_file(&part).await; // The half-written file goes; the old one stays.
        return Err(error);
    }

    // Complete. Only now does the finished file take the place of the old one.
    if target.stat(&snapshot.to).await.is_ok()
        && let Err(error) = target.remove_file(&snapshot.to).await
    {
        let _ = target.remove_file(&part).await;
        return Err(error);
    }
    if let Err(error) = target.rename(&part, &snapshot.to).await {
        let _ = target.remove_file(&part).await;
        return Err(error);
    }
    Ok(JobState::Done)
}

fn cancelled() -> AppError {
    AppError::new(ErrorCode::Cancelled, "The transfer was cancelled.")
}

// ---- Folders -------------------------------------------------------------------------------

/// One step of copying a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeOp {
    Folder {
        to: String,
    },
    File {
        from: String,
        to: String,
        size: Option<u64>,
    },
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct TreePlan {
    /// Folders first, each before what is inside it.
    pub ops: Vec<TreeOp>,
    /// Entries left out, each with the reason.
    pub skipped: Vec<String>,
}

fn join(base: &str, name: &str) -> String {
    if base.ends_with('/') || base.ends_with('\\') {
        format!("{base}{name}")
    } else if base.contains('\\') && !base.contains('/') {
        format!("{base}\\{name}")
    } else {
        format!("{base}/{name}")
    }
}

/// Works out what copying a whole folder involves. Names from the source are not trusted (SEC11):
/// one that could write outside the destination is left out and reported, and the rest carry on.
/// Links are not followed.
pub async fn plan_tree(
    source: &Arc<dyn FileBackend>,
    from: &str,
    to: &str,
) -> Result<TreePlan, AppError> {
    let mut plan = TreePlan::default();
    plan.ops.push(TreeOp::Folder { to: to.to_string() });
    let mut pending = vec![(from.to_string(), to.to_string())];
    while let Some((dir_from, dir_to)) = pending.pop() {
        let mut entries = source.list(&dir_from).await?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        for entry in entries {
            if let Err(why) = check_remote_name(&entry.name) {
                plan.skipped
                    .push(format!("{}: {why}", entry.name.escape_debug()));
                continue;
            }
            let child_from = join(&dir_from, &entry.name);
            let child_to = join(&dir_to, &entry.name);
            match entry.kind {
                FileKind::Dir => {
                    plan.ops.push(TreeOp::Folder {
                        to: child_to.clone(),
                    });
                    pending.push((child_from, child_to));
                }
                FileKind::File => plan.ops.push(TreeOp::File {
                    from: child_from,
                    to: child_to,
                    size: entry.size,
                }),
                FileKind::Symlink | FileKind::Other => plan.skipped.push(format!(
                    "{}: links and special files are not copied",
                    entry.name
                )),
            }
        }
    }
    Ok(plan)
}

/// Adds a whole folder to the queue, as folder-making and file jobs in a safe order.
pub fn queue_tree(
    queue: &TransferQueue,
    source: &Arc<dyn FileBackend>,
    target: &Arc<dyn FileBackend>,
    plan: &TreePlan,
    from_root: &str,
) -> Vec<u64> {
    plan.ops
        .iter()
        .map(|op| match op {
            TreeOp::Folder { to } => queue.add_folder(CopyRequest {
                source: source.clone(),
                from: from_root.to_string(),
                target: target.clone(),
                to: to.clone(),
                size: None,
            }),
            TreeOp::File { from, to, size } => queue.add_copy(CopyRequest {
                source: source.clone(),
                from: from.clone(),
                target: target.clone(),
                to: to.clone(),
                size: *size,
            }),
        })
        .collect()
}

/// What deleting a folder involves, worked out before anything is removed so the user can be
/// told how much is about to go.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DeletePlan {
    /// Files first, then folders from the bottom up, ending with the folder itself.
    pub order: Vec<(String, FileKind)>,
    pub files: usize,
    pub folders: usize,
}

pub async fn plan_delete(
    backend: &Arc<dyn FileBackend>,
    path: &str,
) -> Result<DeletePlan, AppError> {
    let mut plan = DeletePlan::default();
    let mut stack = vec![path.to_string()];
    let mut folders = Vec::new();
    while let Some(dir) = stack.pop() {
        folders.push(dir.clone());
        for entry in backend.list(&dir).await? {
            if check_remote_name(&entry.name).is_err() {
                continue; // Can't be addressed safely, so it is left alone.
            }
            let child = join(&dir, &entry.name);
            if entry.kind == FileKind::Dir {
                stack.push(child);
            } else {
                plan.order.push((child, entry.kind));
                plan.files += 1;
            }
        }
    }
    // Deepest folders first, so each is empty when its turn comes.
    folders.reverse();
    plan.folders = folders.len();
    plan.order
        .extend(folders.into_iter().map(|f| (f, FileKind::Dir)));
    Ok(plan)
}

/// Carries out a plan from `plan_delete`. Stops at the first error.
pub async fn run_delete(
    backend: &Arc<dyn FileBackend>,
    plan: &DeletePlan,
    cancel: &CancellationToken,
) -> Result<(), AppError> {
    for (path, kind) in &plan.order {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        match kind {
            FileKind::Dir => backend.remove_dir(path).await?,
            _ => backend.remove_file(path).await?,
        }
    }
    Ok(())
}
