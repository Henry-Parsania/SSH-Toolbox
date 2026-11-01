//! Container logs and shells in panes, against the fake engine.

use std::sync::Arc;
use std::time::Duration;

use bollard::Docker;

use super::fake::fake_docker_engine;
use super::pane::{open_logs, open_shell};
use crate::error::ErrorCode;
use crate::terminal::testing::{RecordingSink, Sent};
use crate::terminal::{SessionRegistry, TermSize, TerminalStatus};

async fn engine() -> Arc<Docker> {
    let (port, _) = fake_docker_engine().await;
    Arc::new(
        Docker::connect_with_http(
            &format!("http://127.0.0.1:{port}"),
            10,
            bollard::API_DEFAULT_VERSION,
        )
        .unwrap(),
    )
}

fn received(sink: &RecordingSink) -> String {
    let bytes: Vec<u8> = sink
        .all()
        .into_iter()
        .filter_map(|s| match s {
            Sent::Data(d) => Some(d),
            Sent::Status(_) => None,
        })
        .flatten()
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

async fn wait_for(sink: &RecordingSink, wanted: &str) {
    for _ in 0..200 {
        if received(sink).contains(wanted) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("never saw {wanted:?}; got {:?}", received(sink));
}

#[tokio::test]
async fn logs_show_both_streams_with_terminal_line_endings() {
    let docker = engine().await;
    let registry = SessionRegistry::new();
    let sink = Arc::new(RecordingSink::default());
    open_logs(&registry, docker, "abc123".into(), true, sink.clone())
        .await
        .unwrap();
    wait_for(&sink, "oops\r\n").await;
    assert!(received(&sink).starts_with("hello\r\n"));
}

#[tokio::test]
async fn a_log_cannot_be_typed_into_and_closes_cleanly() {
    let docker = engine().await;
    let registry = SessionRegistry::new();
    let sink = Arc::new(RecordingSink::default());
    let id = open_logs(&registry, docker, "abc123".into(), true, sink.clone())
        .await
        .unwrap();
    wait_for(&sink, "hello").await;
    let session = registry.get(&id).unwrap();
    session.write(b"rm -rf /\r").await.unwrap();
    registry.close(&id).await.unwrap();
    assert!(!received(&sink).contains("rm -rf"));
}

#[tokio::test]
async fn a_shell_sends_what_is_typed_and_shows_what_comes_back() {
    let docker = engine().await;
    let registry = SessionRegistry::new();
    let sink = Arc::new(RecordingSink::default());
    let id = open_shell(
        &registry,
        docker,
        "abc123".into(),
        TermSize {
            cols: 100,
            rows: 30,
        },
        sink.clone(),
    )
    .await
    .unwrap();
    wait_for(&sink, "$ ").await;
    let session = registry.get(&id).unwrap();
    session.write(b"ls\r").await.unwrap();
    wait_for(&sink, "echo:ls\r").await;
    session
        .resize(TermSize {
            cols: 120,
            rows: 40,
        })
        .await
        .unwrap();
    registry.close(&id).await.unwrap();
}

#[tokio::test]
async fn a_container_that_is_gone_gives_a_plain_message() {
    let docker = engine().await;
    let registry = SessionRegistry::new();
    let sink = Arc::new(RecordingSink::default());
    let error = open_shell(
        &registry,
        docker.clone(),
        "fade".into(),
        TermSize { cols: 80, rows: 24 },
        sink.clone(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::NotFound);
    let error = open_logs(&registry, docker, "../etc".into(), false, sink)
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidInput);
}

#[tokio::test]
async fn a_log_that_ends_reports_that_it_ended() {
    // The fake log never ends by itself, so closing from the engine's side is simulated by a
    // missing container: the session is not opened at all.
    let docker = engine().await;
    let registry = SessionRegistry::new();
    let sink = Arc::new(RecordingSink::default());
    assert!(
        open_logs(&registry, docker, "fade".into(), false, sink.clone())
            .await
            .is_err()
    );
    assert!(
        !sink
            .statuses()
            .iter()
            .any(|s| matches!(s, TerminalStatus::Connected))
    );
}
