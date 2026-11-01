//! The transfer queue, against in-memory backends that can be made to fail or to go slowly.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;

use super::memory::MemoryFiles;
use super::queue::{
    CopyRequest, JobSnapshot, JobState, OverwriteChoice, OverwriteResolver, PART_SUFFIX,
    TransferQueue, TreeOp, plan_delete, plan_tree, queue_tree, run_delete,
};
use super::{FileBackend, FileEntry, FileKind, LocalFiles};
use crate::error::ErrorCode;

/// Answers questions from a list, and counts how often it was asked.
struct Scripted {
    answers: std::sync::Mutex<Vec<OverwriteChoice>>,
    asked: AtomicUsize,
}

impl Scripted {
    fn new(answers: Vec<OverwriteChoice>) -> Arc<Self> {
        Arc::new(Self {
            answers: std::sync::Mutex::new(answers),
            asked: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl OverwriteResolver for Scripted {
    async fn resolve(&self, _job: &JobSnapshot, _existing: &FileEntry) -> OverwriteChoice {
        self.asked.fetch_add(1, Ordering::SeqCst);
        let mut answers = self.answers.lock().unwrap();
        if answers.is_empty() {
            OverwriteChoice::Skip
        } else {
            answers.remove(0)
        }
    }
}

fn queue(resolver: Arc<Scripted>) -> TransferQueue {
    TransferQueue::new(resolver, || {})
}

fn copy(source: &Arc<MemoryFiles>, from: &str, target: &Arc<MemoryFiles>, to: &str) -> CopyRequest {
    let size = source.get(from).map(|b| b.len() as u64);
    let s: Arc<dyn FileBackend> = source.clone();
    let t: Arc<dyn FileBackend> = target.clone();
    CopyRequest {
        source: s,
        from: from.into(),
        target: t,
        to: to.into(),
        size,
    }
}

async fn finished(queue: &TransferQueue) -> Vec<JobSnapshot> {
    for _ in 0..600 {
        let snapshot = queue.snapshot();
        if !snapshot.is_empty()
            && snapshot.iter().all(|j| {
                !matches!(
                    j.state,
                    JobState::Queued | JobState::Asking | JobState::Running
                )
            })
        {
            return snapshot;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("jobs didn't finish: {:?}", queue.snapshot());
}

fn data(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_file_is_copied_in_full_with_progress_and_no_part_file_left() {
    let (a, b) = (Arc::new(MemoryFiles::new()), Arc::new(MemoryFiles::new()));
    a.put("/src/f.bin", &data(50_000));
    b.put_dir("/dst");
    let q = queue(Scripted::new(vec![]));
    q.add_copy(copy(&a, "/src/f.bin", &b, "/dst/f.bin"));

    let jobs = finished(&q).await;
    assert_eq!(jobs[0].state, JobState::Done);
    assert_eq!(jobs[0].done, 50_000);
    assert_eq!(jobs[0].total, Some(50_000));
    assert_eq!(b.get("/dst/f.bin").unwrap(), data(50_000));
    assert!(
        b.names().iter().all(|n| !n.ends_with(PART_SUFFIX)),
        "{:?}",
        b.names()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_existing_file_is_kept_when_the_answer_is_skip_and_replaced_for_overwrite() {
    let (a, b) = (Arc::new(MemoryFiles::new()), Arc::new(MemoryFiles::new()));
    a.put("/src/f", b"NEW");
    b.put("/dst/f", b"OLD");

    let skipping = Scripted::new(vec![OverwriteChoice::Skip]);
    let q = queue(skipping.clone());
    q.add_copy(copy(&a, "/src/f", &b, "/dst/f"));
    assert_eq!(finished(&q).await[0].state, JobState::Skipped);
    assert_eq!(b.get("/dst/f").unwrap(), b"OLD");
    assert_eq!(skipping.asked.load(Ordering::SeqCst), 1);

    let q = queue(Scripted::new(vec![OverwriteChoice::Overwrite]));
    q.add_copy(copy(&a, "/src/f", &b, "/dst/f"));
    assert_eq!(finished(&q).await[0].state, JobState::Done);
    assert_eq!(b.get("/dst/f").unwrap(), b"NEW");
}

#[tokio::test(flavor = "multi_thread")]
async fn overwrite_all_and_skip_all_ask_once_and_then_apply_to_the_rest() {
    for (choice, expect_new) in [
        (OverwriteChoice::OverwriteAll, true),
        (OverwriteChoice::SkipAll, false),
    ] {
        let (a, b) = (Arc::new(MemoryFiles::new()), Arc::new(MemoryFiles::new()));
        for name in ["1", "2", "3"] {
            a.put(&format!("/src/{name}"), b"NEW");
            b.put(&format!("/dst/{name}"), b"OLD");
        }
        let resolver = Scripted::new(vec![choice]);
        let q = queue(resolver.clone());
        for name in ["1", "2", "3"] {
            q.add_copy(copy(
                &a,
                &format!("/src/{name}"),
                &b,
                &format!("/dst/{name}"),
            ));
        }
        finished(&q).await;
        assert_eq!(resolver.asked.load(Ordering::SeqCst), 1, "{choice:?}");
        for name in ["1", "2", "3"] {
            let expected: &[u8] = if expect_new { b"NEW" } else { b"OLD" };
            assert_eq!(b.get(&format!("/dst/{name}")).unwrap(), expected);
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_a_transfer_leaves_the_old_file_untouched_and_removes_the_part_file() {
    let (a, b) = (Arc::new(MemoryFiles::new()), Arc::new(MemoryFiles::new()));
    a.put("/src/big", &data(200_000));
    a.read_delay_ms.store(5, Ordering::SeqCst);
    b.put("/dst/big", b"PRECIOUS OLD CONTENT");
    let q = queue(Scripted::new(vec![OverwriteChoice::Overwrite]));
    let id = q.add_copy(copy(&a, "/src/big", &b, "/dst/big"));

    // Wait until it is really moving, then cancel.
    for _ in 0..400 {
        if q.snapshot()[0].done > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(q.snapshot()[0].done > 0, "the transfer never started");
    q.cancel(id);

    let jobs = finished(&q).await;
    assert_eq!(jobs[0].state, JobState::Cancelled);
    assert_eq!(b.get("/dst/big").unwrap(), b"PRECIOUS OLD CONTENT");
    assert!(
        b.names().iter().all(|n| !n.ends_with(PART_SUFFIX)),
        "{:?}",
        b.names()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failure_part_way_leaves_the_old_file_untouched_and_is_reported() {
    let (a, b) = (Arc::new(MemoryFiles::new()), Arc::new(MemoryFiles::new()));
    a.put("/src/f", &data(100_000));
    *a.fail_read_after.lock().unwrap() = Some(30_000);
    b.put("/dst/f", b"OLD");
    let q = queue(Scripted::new(vec![OverwriteChoice::Overwrite]));
    q.add_copy(copy(&a, "/src/f", &b, "/dst/f"));

    let jobs = finished(&q).await;
    match &jobs[0].state {
        JobState::Failed { error } => assert_eq!(error.code, ErrorCode::ConnectionLost),
        other => panic!("{other:?}"),
    }
    assert_eq!(b.get("/dst/f").unwrap(), b"OLD");
    assert!(
        b.names().iter().all(|n| !n.ends_with(PART_SUFFIX)),
        "{:?}",
        b.names()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_cancelled_while_waiting_never_runs_and_the_next_one_does() {
    let (a, b) = (Arc::new(MemoryFiles::new()), Arc::new(MemoryFiles::new()));
    a.put("/src/slow", &data(100_000));
    a.read_delay_ms.store(3, Ordering::SeqCst);
    a.put("/src/quick", b"quick");
    let q = queue(Scripted::new(vec![]));
    let first = q.add_copy(copy(&a, "/src/slow", &b, "/dst/slow"));
    let second = q.add_copy(copy(&a, "/src/quick", &b, "/dst/quick"));
    let third = q.add_copy(copy(&a, "/src/quick", &b, "/dst/quick2"));
    q.cancel(second);
    // Let the first finish quickly by cancelling it too.
    tokio::time::sleep(Duration::from_millis(50)).await;
    q.cancel(first);

    let jobs = finished(&q).await;
    let state = |id: u64| jobs.iter().find(|j| j.id == id).unwrap().state.clone();
    assert_eq!(state(first), JobState::Cancelled);
    assert_eq!(state(second), JobState::Cancelled);
    assert_eq!(state(third), JobState::Done);
    assert!(b.get("/dst/quick").is_none());
    assert_eq!(b.get("/dst/quick2").unwrap(), b"quick");
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_while_the_question_is_open_ends_the_job() {
    struct Never;
    #[async_trait]
    impl OverwriteResolver for Never {
        async fn resolve(&self, _job: &JobSnapshot, _existing: &FileEntry) -> OverwriteChoice {
            std::future::pending().await
        }
    }
    let (a, b) = (Arc::new(MemoryFiles::new()), Arc::new(MemoryFiles::new()));
    a.put("/src/f", b"NEW");
    b.put("/dst/f", b"OLD");
    let q = TransferQueue::new(Arc::new(Never), || {});
    let id = q.add_copy(copy(&a, "/src/f", &b, "/dst/f"));
    for _ in 0..200 {
        if q.snapshot()[0].state == JobState::Asking {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(q.snapshot()[0].state, JobState::Asking);
    q.cancel(id);
    assert_eq!(finished(&q).await[0].state, JobState::Cancelled);
    assert_eq!(b.get("/dst/f").unwrap(), b"OLD");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_with_that_name_is_not_replaced_by_a_file() {
    let (a, b) = (Arc::new(MemoryFiles::new()), Arc::new(MemoryFiles::new()));
    a.put("/src/x", b"file");
    b.put_dir("/dst/x");
    let q = queue(Scripted::new(vec![]));
    q.add_copy(copy(&a, "/src/x", &b, "/dst/x"));
    match &finished(&q).await[0].state {
        JobState::Failed { error } => assert_eq!(error.code, ErrorCode::AlreadyExists),
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn clearing_removes_only_finished_jobs() {
    let (a, b) = (Arc::new(MemoryFiles::new()), Arc::new(MemoryFiles::new()));
    a.put("/src/f", b"x");
    let q = queue(Scripted::new(vec![]));
    q.add_copy(copy(&a, "/src/f", &b, "/dst/f"));
    finished(&q).await;
    q.clear_finished();
    assert!(q.snapshot().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn copying_between_memory_and_a_real_folder_works_both_ways() {
    let dir = tempfile::tempdir().unwrap();
    let local = Arc::new(LocalFiles);
    let mem = Arc::new(MemoryFiles::new());
    mem.put("/remote/a.txt", &data(70_000));
    let q = queue(Scripted::new(vec![]));
    let to = dir.path().join("a.txt").display().to_string();

    let (s, t): (Arc<dyn FileBackend>, Arc<dyn FileBackend>) = (mem.clone(), local.clone());
    q.add_copy(CopyRequest {
        source: s,
        from: "/remote/a.txt".into(),
        target: t,
        to: to.clone(),
        size: Some(70_000),
    });
    assert_eq!(finished(&q).await[0].state, JobState::Done);
    assert_eq!(
        std::fs::read(dir.path().join("a.txt")).unwrap(),
        data(70_000)
    );
    assert!(!dir.path().join(format!("a.txt{PART_SUFFIX}")).exists());

    // And back up, replacing an existing remote file after the answer.
    mem.put("/up/a.txt", b"old");
    let q = queue(Scripted::new(vec![OverwriteChoice::Overwrite]));
    let (s, t): (Arc<dyn FileBackend>, Arc<dyn FileBackend>) = (local.clone(), mem.clone());
    q.add_copy(CopyRequest {
        source: s,
        from: to,
        target: t,
        to: "/up/a.txt".into(),
        size: None,
    });
    assert_eq!(finished(&q).await[0].state, JobState::Done);
    assert_eq!(mem.get("/up/a.txt").unwrap(), data(70_000));
}

// ---- Folders ----

#[tokio::test(flavor = "multi_thread")]
async fn copying_a_folder_makes_folders_before_what_is_in_them_and_skips_links() {
    let (a, b) = (Arc::new(MemoryFiles::new()), Arc::new(MemoryFiles::new()));
    a.put("/proj/readme.md", b"read me");
    a.put("/proj/src/main.rs", b"fn main() {}");
    a.put("/proj/src/deep/lib.rs", b"//");
    a.put_dir("/proj/empty");
    a.put_link("/proj/shortcut");
    let source: Arc<dyn FileBackend> = a.clone();
    let target: Arc<dyn FileBackend> = b.clone();

    let plan = plan_tree(&source, "/proj", "/out/proj").await.unwrap();
    assert_eq!(
        plan.ops[0],
        TreeOp::Folder {
            to: "/out/proj".into()
        }
    );
    // Every folder is made before anything inside it.
    for (i, op) in plan.ops.iter().enumerate() {
        let path = match op {
            TreeOp::Folder { to } | TreeOp::File { to, .. } => to,
        };
        let parent = path.rsplit_once('/').unwrap().0;
        if parent != "/out" {
            let made_before = plan.ops[..i]
                .iter()
                .any(|p| matches!(p, TreeOp::Folder { to } if to == parent));
            assert!(made_before, "{path} comes before its folder");
        }
    }
    assert!(
        plan.skipped.iter().any(|s| s.contains("shortcut")),
        "{:?}",
        plan.skipped
    );

    b.put_dir("/out");
    let q = queue(Scripted::new(vec![]));
    queue_tree(&q, &source, &target, &plan, "/proj");
    let jobs = finished(&q).await;
    assert!(jobs.iter().all(|j| j.state == JobState::Done), "{jobs:?}");
    assert_eq!(b.get("/out/proj/src/deep/lib.rs").unwrap(), b"//");
    assert_eq!(b.get("/out/proj/readme.md").unwrap(), b"read me");
    assert!(b.dir_names().contains(&"/out/proj/empty".to_string()));
    assert!(b.get("/out/proj/shortcut").is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn hostile_names_from_the_source_are_left_out_and_reported() {
    let a = Arc::new(MemoryFiles::new());
    a.put("/proj/fine.txt", b"ok");
    for bad in ["..", "a/b", "..\\..\\evil", "C:", "", "x\0y"] {
        a.fake_names
            .lock()
            .unwrap()
            .push(("/proj".into(), bad.into()));
    }
    let source: Arc<dyn FileBackend> = a.clone();
    let plan = plan_tree(&source, "/proj", "/out").await.unwrap();

    let files: Vec<&TreeOp> = plan
        .ops
        .iter()
        .filter(|o| matches!(o, TreeOp::File { .. }))
        .collect();
    assert_eq!(
        files.len(),
        1,
        "only the harmless file is planned: {files:?}"
    );
    assert_eq!(plan.skipped.len(), 6, "{:?}", plan.skipped);
    for op in &plan.ops {
        let path = match op {
            TreeOp::Folder { to } | TreeOp::File { to, .. } => to,
        };
        assert!(!path.contains(".."), "{path}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_a_folder_removes_the_contents_from_the_bottom_up_after_counting_them() {
    let a = Arc::new(MemoryFiles::new());
    a.put("/d/one", b"1");
    a.put("/d/sub/two", b"2");
    a.put("/d/sub/deeper/three", b"3");
    let backend: Arc<dyn FileBackend> = a.clone();

    let plan = plan_delete(&backend, "/d").await.unwrap();
    assert_eq!((plan.files, plan.folders), (3, 3));
    run_delete(&backend, &plan, &tokio_util::sync::CancellationToken::new())
        .await
        .unwrap();
    assert!(a.names().is_empty());
    assert!(!a.dir_names().contains(&"/d".to_string()));

    // Each folder was empty when it was removed, and the folder itself went last.
    let removed = a.removed.lock().unwrap().clone();
    assert_eq!(removed.last().unwrap(), "/d");
    let position = |p: &str| removed.iter().position(|r| r == p).unwrap();
    assert!(position("/d/sub/deeper") < position("/d/sub"));
    assert!(position("/d/sub") < position("/d"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_delete_stops_where_it_is() {
    let a = Arc::new(MemoryFiles::new());
    a.put("/d/one", b"1");
    let backend: Arc<dyn FileBackend> = a.clone();
    let plan = plan_delete(&backend, "/d").await.unwrap();
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();
    let err = run_delete(&backend, &plan, &cancel).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Cancelled);
    assert_eq!(a.get("/d/one").unwrap(), b"1");
    let _ = FileKind::File;
}
