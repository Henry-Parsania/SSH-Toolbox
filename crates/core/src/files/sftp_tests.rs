//! SFTP against a server running inside the test, serving a temporary folder.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::memory::MemoryFiles;
use super::queue::{
    CopyRequest, JobSnapshot, JobState, OverwriteChoice, OverwriteResolver, PART_SUFFIX,
    TransferQueue,
};
use super::{FileBackend, FileEntry, FileKind, LocalFiles, SftpFiles};
use crate::error::ErrorCode;
use crate::keychain::{Keychain, SecretStore};
use crate::prompt::{PromptBroker, Prompter};
use crate::ssh::auth::{Login, LoginMethod, login};
use crate::ssh::connection::connect;
use crate::ssh::hostkeys::learn;
use crate::ssh::pool::SharedConnection;
use crate::ssh::testing::{
    Accepts, KEY_A, MemoryStore, ServerHandle, key, public_of, spawn_server,
};
use crate::terminal::testing::RecordingSink;

struct World {
    sftp: Arc<SftpFiles>,
    server: ServerHandle,
    root: tempfile::TempDir,
    _other: tempfile::TempDir,
}

async fn world() -> World {
    let root = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let host_key = key(KEY_A);
    let server = spawn_server(
        host_key.clone(),
        Accepts {
            user: "tester".into(),
            password: Some("pw".into()),
            sftp_root: Some(root.path().to_path_buf()),
            ..Default::default()
        },
    )
    .await;
    let known_hosts = other.path().join("known_hosts");
    learn(
        "127.0.0.1",
        server.port,
        &public_of(&host_key),
        &known_hosts,
    )
    .unwrap();

    let secrets = Arc::new(MemoryStore::default());
    secrets
        .items
        .lock()
        .unwrap()
        .insert("session:s1:password".into(), "pw".into());
    let dyn_secrets: Arc<dyn SecretStore> = secrets;
    let keychain = Keychain::with_store(Some(dyn_secrets), Duration::from_millis(500));
    let prompter = Prompter::new(
        PromptBroker::new(),
        Arc::new(RecordingSink::default()),
        CancellationToken::new(),
    );
    let mut connection = connect("127.0.0.1", server.port, known_hosts, prompter.clone())
        .await
        .unwrap();
    login(
        &mut connection,
        &Login {
            session_id: "s1".into(),
            host: "127.0.0.1".into(),
            user: "tester".into(),
            method: LoginMethod::Password,
        },
        &keychain,
        &prompter,
    )
    .await
    .unwrap();
    let sftp = SftpFiles::open(SharedConnection::new(connection))
        .await
        .unwrap();
    World {
        sftp: Arc::new(sftp),
        server,
        root,
        _other: other,
    }
}

fn data(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

struct Answers(OverwriteChoice);

#[async_trait::async_trait]
impl OverwriteResolver for Answers {
    async fn resolve(&self, _job: &JobSnapshot, _existing: &FileEntry) -> OverwriteChoice {
        self.0
    }
}

async fn finished(queue: &TransferQueue) -> Vec<JobSnapshot> {
    for _ in 0..800 {
        let jobs = queue.snapshot();
        if !jobs.is_empty()
            && jobs.iter().all(|j| {
                !matches!(
                    j.state,
                    JobState::Queued | JobState::Asking | JobState::Running
                )
            })
        {
            return jobs;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("jobs didn't finish: {:?}", queue.snapshot());
}

#[tokio::test(flavor = "multi_thread")]
async fn it_uses_the_open_connection_and_lists_folders_and_files() {
    let w = world().await;
    std::fs::write(w.root.path().join("a.txt"), "hello").unwrap();
    std::fs::create_dir(w.root.path().join("sub")).unwrap();

    assert_eq!(w.sftp.home().await.unwrap(), "/");
    let mut entries = w.sftp.list("/").await.unwrap();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(entries.len(), 2);
    assert_eq!(
        (entries[0].name.as_str(), entries[0].kind),
        ("a.txt", FileKind::File)
    );
    assert_eq!(entries[0].size, Some(5));
    assert_eq!(entries[0].path, "/a.txt");
    assert_eq!(entries[1].kind, FileKind::Dir);
    assert_eq!(entries[1].size, None);

    // One login only: SFTP is a second channel on the same connection.
    assert_eq!(w.server.stats.opened.load(Ordering::SeqCst), 1);
    assert_eq!(w.server.stats.sftp_sessions.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn it_makes_renames_and_removes_things() {
    let w = world().await;
    w.sftp.mkdir("/d").await.unwrap();
    assert_eq!(
        w.sftp.mkdir("/d").await.unwrap_err().code,
        ErrorCode::AlreadyExists
    );
    std::fs::write(w.root.path().join("d").join("f"), "x").unwrap();

    assert!(w.sftp.remove_dir("/d").await.is_err(), "not empty");
    w.sftp.rename("/d/f", "/d/g").await.unwrap();
    assert!(w.root.path().join("d").join("g").exists());
    w.sftp.remove_file("/d/g").await.unwrap();
    w.sftp.remove_dir("/d").await.unwrap();
    assert!(!w.root.path().join("d").exists());

    assert_eq!(
        w.sftp.stat("/nothing").await.unwrap_err().code,
        ErrorCode::NotFound
    );
    assert_eq!(
        w.sftp.list("/nothing").await.unwrap_err().code,
        ErrorCode::NotFound
    );
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn permissions_can_be_read_and_changed() {
    use std::os::unix::fs::PermissionsExt;
    let w = world().await;
    std::fs::write(w.root.path().join("f"), "x").unwrap();
    assert!(w.sftp.capabilities().chmod);
    w.sftp.chmod("/f", 0o640).await.unwrap();
    let mode = std::fs::metadata(w.root.path().join("f"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o640);
    assert_eq!(w.sftp.stat("/f").await.unwrap().permissions, Some(0o640));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_download_and_an_upload_move_every_byte() {
    let w = world().await;
    let big = data(400_000);
    std::fs::write(w.root.path().join("big.bin"), &big).unwrap();
    let local_dir = tempfile::tempdir().unwrap();
    let local = Arc::new(LocalFiles);
    let queue = TransferQueue::new(Arc::new(Answers(OverwriteChoice::Overwrite)), || {});

    // Down.
    let down = local_dir.path().join("big.bin").display().to_string();
    let (s, t): (Arc<dyn FileBackend>, Arc<dyn FileBackend>) = (w.sftp.clone(), local.clone());
    queue.add_copy(CopyRequest {
        source: s,
        from: "/big.bin".into(),
        target: t,
        to: down.clone(),
        size: Some(big.len() as u64),
    });
    let jobs = finished(&queue).await;
    assert_eq!(jobs[0].state, JobState::Done, "{jobs:?}");
    assert_eq!(std::fs::read(&down).unwrap(), big);
    assert_eq!(jobs[0].done, big.len() as u64);
    queue.clear_finished();

    // Up, to a new name.
    let (s, t): (Arc<dyn FileBackend>, Arc<dyn FileBackend>) = (local.clone(), w.sftp.clone());
    queue.add_copy(CopyRequest {
        source: s,
        from: down,
        target: t,
        to: "/copy.bin".into(),
        size: None,
    });
    assert_eq!(finished(&queue).await[0].state, JobState::Done);
    assert_eq!(std::fs::read(w.root.path().join("copy.bin")).unwrap(), big);
    // No half-written file was left behind.
    for entry in std::fs::read_dir(w.root.path()).unwrap().flatten() {
        assert!(
            !entry.file_name().to_string_lossy().ends_with(PART_SUFFIX),
            "{:?}",
            entry.file_name()
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn replacing_a_file_works_even_though_sftp_refuses_to_rename_onto_one() {
    let w = world().await;
    std::fs::write(w.root.path().join("f.txt"), "OLD").unwrap();
    let local_dir = tempfile::tempdir().unwrap();
    std::fs::write(local_dir.path().join("f.txt"), "NEW CONTENT").unwrap();
    let queue = TransferQueue::new(Arc::new(Answers(OverwriteChoice::Overwrite)), || {});

    let (s, t): (Arc<dyn FileBackend>, Arc<dyn FileBackend>) =
        (Arc::new(LocalFiles), w.sftp.clone());
    queue.add_copy(CopyRequest {
        source: s,
        from: local_dir.path().join("f.txt").display().to_string(),
        target: t,
        to: "/f.txt".into(),
        size: None,
    });
    assert_eq!(finished(&queue).await[0].state, JobState::Done);
    assert_eq!(
        std::fs::read_to_string(w.root.path().join("f.txt")).unwrap(),
        "NEW CONTENT"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_upload_leaves_the_old_remote_file_and_removes_the_part_file() {
    let w = world().await;
    std::fs::write(w.root.path().join("keep.bin"), "PRECIOUS").unwrap();
    let slow = Arc::new(MemoryFiles::new());
    slow.put("/src/keep.bin", &data(300_000));
    slow.read_delay_ms.store(4, Ordering::SeqCst);
    let queue = TransferQueue::new(Arc::new(Answers(OverwriteChoice::Overwrite)), || {});

    let (s, t): (Arc<dyn FileBackend>, Arc<dyn FileBackend>) = (slow, w.sftp.clone());
    let id = queue.add_copy(CopyRequest {
        source: s,
        from: "/src/keep.bin".into(),
        target: t,
        to: "/keep.bin".into(),
        size: None,
    });
    for _ in 0..400 {
        if queue.snapshot()[0].done > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(queue.snapshot()[0].done > 0, "never started");
    queue.cancel(id);

    assert_eq!(finished(&queue).await[0].state, JobState::Cancelled);
    assert_eq!(
        std::fs::read_to_string(w.root.path().join("keep.bin")).unwrap(),
        "PRECIOUS"
    );
    let leftovers: Vec<String> = std::fs::read_dir(w.root.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(PART_SUFFIX))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lost_connection_is_reported_as_such() {
    let w = world().await;
    std::fs::write(w.root.path().join("a"), "x").unwrap();
    assert!(w.sftp.list("/").await.is_ok());
    w.server.kill();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let err = w.sftp.list("/").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::ConnectionLost, "{err:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_without_sftp_says_so() {
    // A server with no folder to serve refuses the subsystem.
    let other = tempfile::tempdir().unwrap();
    let host_key = key(KEY_A);
    let server = spawn_server(
        host_key.clone(),
        Accepts {
            user: "tester".into(),
            password: Some("pw".into()),
            ..Default::default()
        },
    )
    .await;
    let known_hosts = other.path().join("known_hosts");
    learn(
        "127.0.0.1",
        server.port,
        &public_of(&host_key),
        &known_hosts,
    )
    .unwrap();
    let secrets = Arc::new(MemoryStore::default());
    secrets
        .items
        .lock()
        .unwrap()
        .insert("session:s1:password".into(), "pw".into());
    let dyn_secrets: Arc<dyn SecretStore> = secrets;
    let keychain = Keychain::with_store(Some(dyn_secrets), Duration::from_millis(500));
    let prompter = Prompter::new(
        PromptBroker::new(),
        Arc::new(RecordingSink::default()),
        CancellationToken::new(),
    );
    let mut connection = connect("127.0.0.1", server.port, known_hosts, prompter.clone())
        .await
        .unwrap();
    login(
        &mut connection,
        &Login {
            session_id: "s1".into(),
            host: "127.0.0.1".into(),
            user: "tester".into(),
            method: LoginMethod::Password,
        },
        &keychain,
        &prompter,
    )
    .await
    .unwrap();
    let err = match SftpFiles::open(SharedConnection::new(connection)).await {
        Err(e) => e,
        Ok(_) => panic!("SFTP should have been refused"),
    };
    assert_eq!(err.code, ErrorCode::Unsupported);
    assert!(err.message.contains("SFTP"), "{}", err.message);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_file_can_be_edited_here_and_sent_back_over_sftp() {
    use super::{EditSessions, UploadResult};
    let w = world().await;
    std::fs::write(w.root.path().join("compose.yaml"), "services: {}\n").unwrap();
    let edits_dir = tempfile::tempdir().unwrap();
    let edits = EditSessions::new(edits_dir.path().join("edits"));

    let info = edits
        .open(w.sftp.clone(), "/compose.yaml", |_| {})
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&info.local_path).unwrap(),
        "services: {}\n"
    );

    std::fs::write(&info.local_path, "services:\n  web:\n    image: nginx\n").unwrap();
    assert_eq!(edits.upload(&info.id, false).await.unwrap(), UploadResult::Uploaded);
    assert_eq!(
        std::fs::read_to_string(w.root.path().join("compose.yaml")).unwrap(),
        "services:\n  web:\n    image: nginx\n"
    );

    // Saving again right after is not mistaken for a change made by someone else.
    std::fs::write(&info.local_path, "services:\n  web:\n    image: caddy\n").unwrap();
    assert_eq!(edits.upload(&info.id, false).await.unwrap(), UploadResult::Uploaded);

    // Someone else changes the server's file later: nothing is sent unless forced.
    tokio::time::sleep(Duration::from_millis(1200)).await;
    std::fs::write(w.root.path().join("compose.yaml"), "theirs\n").unwrap();
    std::fs::write(&info.local_path, "mine\n").unwrap();
    assert_eq!(
        edits.upload(&info.id, false).await.unwrap(),
        UploadResult::RemoteChanged
    );
    assert_eq!(
        std::fs::read_to_string(w.root.path().join("compose.yaml")).unwrap(),
        "theirs\n"
    );
    assert_eq!(edits.upload(&info.id, true).await.unwrap(), UploadResult::Uploaded);
    assert_eq!(
        std::fs::read_to_string(w.root.path().join("compose.yaml")).unwrap(),
        "mine\n"
    );
    edits.stop_all();
}
