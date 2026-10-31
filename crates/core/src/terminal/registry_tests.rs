use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::timeout;

use super::testing::{FakeBackend, RecordingSink};
use super::*;
use crate::error::ErrorCode;

fn open(registry: &SessionRegistry) -> (String, Arc<FakeBackend>) {
    let backend = Arc::new(FakeBackend::default());
    let (_tx, rx) = mpsc::channel(QUEUE_SIZE);
    let pump = spawn_pump(
        rx,
        Arc::new(RecordingSink::default()),
        PumpConfig::default(),
    );
    let id = registry.insert(Session::new(backend.clone(), pump));
    (id, backend)
}

#[tokio::test]
async fn sessions_get_different_ids_and_can_be_found() {
    let registry = SessionRegistry::new();
    let (a, _) = open(&registry);
    let (b, _) = open(&registry);
    assert_ne!(a, b);
    assert_eq!(registry.len(), 2);
    assert!(registry.get(&a).is_ok());
    assert!(registry.get(&b).is_ok());
}

#[tokio::test]
async fn an_unknown_id_is_not_found() {
    let registry = SessionRegistry::new();
    let error = registry.get("nope").err().expect("should fail");
    assert_eq!(error.code, ErrorCode::NotFound);
}

#[tokio::test]
async fn write_and_resize_reach_the_right_backend() {
    let registry = SessionRegistry::new();
    let (a, backend_a) = open(&registry);
    let (b, backend_b) = open(&registry);

    registry.get(&a).unwrap().write(b"hello").await.unwrap();
    registry
        .get(&b)
        .unwrap()
        .resize(TermSize {
            cols: 100,
            rows: 30,
        })
        .await
        .unwrap();

    assert_eq!(*backend_a.written.lock().unwrap(), b"hello");
    assert!(backend_b.written.lock().unwrap().is_empty());
    assert_eq!(
        *backend_b.resizes.lock().unwrap(),
        vec![TermSize {
            cols: 100,
            rows: 30
        }]
    );
    assert!(backend_a.resizes.lock().unwrap().is_empty());
}

#[tokio::test]
async fn closing_removes_the_session_closes_the_backend_and_stops_the_pump() {
    let registry = SessionRegistry::new();
    let (id, backend) = open(&registry);
    let session = registry.get(&id).unwrap();

    registry.close(&id).await.unwrap();

    assert_eq!(backend.closes.load(Ordering::SeqCst), 1);
    assert!(registry.is_empty());
    assert_eq!(registry.get(&id).err().unwrap().code, ErrorCode::NotFound);
    timeout(Duration::from_secs(2), session.pump().finished())
        .await
        .expect("pump should stop when the session closes");
}

#[tokio::test]
async fn closing_twice_is_harmless() {
    let registry = SessionRegistry::new();
    let (id, backend) = open(&registry);
    registry.close(&id).await.unwrap();
    registry.close(&id).await.unwrap();
    registry.close("never-existed").await.unwrap();
    assert_eq!(backend.closes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn closing_one_session_leaves_the_others_open() {
    let registry = SessionRegistry::new();
    let (a, backend_a) = open(&registry);
    let (b, backend_b) = open(&registry);
    registry.close(&a).await.unwrap();
    assert_eq!(backend_a.closes.load(Ordering::SeqCst), 1);
    assert_eq!(backend_b.closes.load(Ordering::SeqCst), 0);
    assert!(registry.get(&b).is_ok());
}

#[tokio::test]
async fn close_all_closes_every_session() {
    let registry = SessionRegistry::new();
    let (_, backend_a) = open(&registry);
    let (_, backend_b) = open(&registry);
    registry.close_all().await;
    assert!(registry.is_empty());
    assert_eq!(backend_a.closes.load(Ordering::SeqCst), 1);
    assert_eq!(backend_b.closes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn acks_reach_the_sessions_pump() {
    let registry = SessionRegistry::new();
    let (id, _) = open(&registry);
    let session = registry.get(&id).unwrap();
    session.ack(10);
    assert_eq!(session.pump().unacked(), 0);
}
