use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};

use super::testing::{RecordingSink, Sent};
use super::*;
use crate::error::{AppError, ErrorCode};

const KIB16: usize = 16 * 1024;

fn start() -> (mpsc::Sender<BackendEvent>, Arc<RecordingSink>, PumpHandle) {
    let (tx, rx) = mpsc::channel(QUEUE_SIZE);
    let sink = Arc::new(RecordingSink::default());
    let pump = spawn_pump(rx, sink.clone(), PumpConfig::default());
    (tx, sink, pump)
}

fn data(len: usize) -> BackendEvent {
    BackendEvent::Data(vec![b'x'; len])
}

/// Sends `count` 16 KiB pieces from a separate task, as a busy program would.
fn flood(tx: &mpsc::Sender<BackendEvent>, count: usize) -> tokio::task::JoinHandle<()> {
    let tx = tx.clone();
    tokio::spawn(async move {
        for _ in 0..count {
            tx.send(data(KIB16)).await.unwrap();
        }
    })
}

#[tokio::test(start_paused = true)]
async fn joins_small_pieces_into_one_chunk() {
    let (tx, sink, _pump) = start();
    for _ in 0..100 {
        tx.send(data(10)).await.unwrap();
    }
    sleep(Duration::from_millis(20)).await;
    assert_eq!(sink.chunk_sizes(), vec![1000]);
}

#[tokio::test(start_paused = true)]
async fn never_sends_more_than_the_largest_chunk() {
    let (tx, sink, _pump) = start();
    flood(&tx, 10).await.unwrap();
    sleep(Duration::from_millis(20)).await;
    assert_eq!(sink.chunk_sizes(), vec![MAX_CHUNK, MAX_CHUNK, 32 * 1024]);
}

#[tokio::test(start_paused = true)]
async fn splits_one_oversized_piece() {
    let (tx, sink, _pump) = start();
    tx.send(data(MAX_CHUNK * 2 + 5)).await.unwrap();
    sleep(Duration::from_millis(20)).await;
    assert_eq!(sink.chunk_sizes(), vec![MAX_CHUNK, MAX_CHUNK, 5]);
}

#[tokio::test(start_paused = true)]
async fn sends_small_output_after_the_flush_timer_not_before() {
    let (tx, sink, _pump) = start();
    tx.send(data(10)).await.unwrap();
    sleep(Duration::from_millis(7)).await;
    assert!(sink.all().is_empty(), "sent before the flush timer");
    sleep(Duration::from_millis(2)).await;
    assert_eq!(sink.chunk_sizes(), vec![10]);
}

#[tokio::test(start_paused = true)]
async fn stops_at_high_and_resumes_only_at_low() {
    let (tx, sink, pump) = start();
    // 40 pieces of 16 KiB. The pump takes 32 of them (8 full chunks, 524,288 bytes), which
    // reaches HIGH, and then stops taking from the queue.
    let producer = flood(&tx, 40);
    sleep(Duration::from_millis(100)).await;
    assert_eq!(sink.data_total(), 8 * MAX_CHUNK);
    assert_eq!(pump.unacked(), 8 * MAX_CHUNK);
    assert!(pump.unacked() >= HIGH);
    assert_eq!(
        tx.capacity(),
        QUEUE_SIZE - 8,
        "the other 8 pieces wait in the queue"
    );

    // One byte above LOW: still stopped.
    pump.ack(8 * MAX_CHUNK - LOW - 1);
    sleep(Duration::from_millis(100)).await;
    assert_eq!(sink.data_total(), 8 * MAX_CHUNK, "resumed above LOW");
    assert_eq!(tx.capacity(), QUEUE_SIZE - 8);

    // Down to LOW: the pump starts again and takes the rest.
    pump.ack(1);
    producer.await.unwrap();
    sleep(Duration::from_millis(100)).await;
    assert_eq!(sink.data_total(), 10 * MAX_CHUNK);
    assert_eq!(tx.capacity(), QUEUE_SIZE);
}

#[tokio::test(start_paused = true)]
async fn final_data_arrives_before_the_exit_status() {
    let (tx, sink, pump) = start();
    tx.send(data(5)).await.unwrap();
    tx.send(BackendEvent::Exited { code: Some(3) })
        .await
        .unwrap();
    pump.finished().await;
    assert_eq!(
        sink.all(),
        vec![
            Sent::Data(vec![b'x'; 5]),
            Sent::Status(TerminalStatus::Exited { code: Some(3) })
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn final_data_arrives_before_a_failure_status() {
    let (tx, sink, pump) = start();
    let error = AppError::new(ErrorCode::ConnectionLost, "The connection dropped.");
    tx.send(data(5)).await.unwrap();
    tx.send(BackendEvent::Failed(error.clone())).await.unwrap();
    pump.finished().await;
    assert_eq!(
        sink.all(),
        vec![
            Sent::Data(vec![b'x'; 5]),
            Sent::Status(TerminalStatus::Failed { error })
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn output_queued_behind_a_pause_still_comes_before_the_exit_status() {
    let (tx, sink, pump) = start();
    flood(&tx, 40).await.unwrap();
    tx.send(BackendEvent::Exited { code: Some(0) })
        .await
        .unwrap();
    sleep(Duration::from_millis(100)).await;
    assert!(
        sink.statuses().is_empty(),
        "exit reported while output was still held back"
    );

    pump.ack(usize::MAX);
    pump.finished().await;
    assert_eq!(sink.data_total(), 40 * KIB16);
    assert_eq!(
        sink.all().last(),
        Some(&Sent::Status(TerminalStatus::Exited { code: Some(0) }))
    );
    assert_eq!(sink.statuses().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn small_output_never_waits_for_an_ack() {
    let (tx, sink, pump) = start();
    tx.send(data(1)).await.unwrap();
    sleep(Duration::from_millis(9)).await;
    assert_eq!(sink.data_total(), 1);
    assert_eq!(pump.unacked(), 1);

    // 200,000 more bytes, still under HIGH, with no acks at all.
    for _ in 0..200 {
        tx.send(data(1000)).await.unwrap();
    }
    sleep(Duration::from_millis(20)).await;
    assert_eq!(sink.data_total(), 200_001);
}

#[tokio::test(start_paused = true)]
async fn an_ack_larger_than_whats_outstanding_does_not_wrap() {
    let (tx, _sink, pump) = start();
    tx.send(data(10)).await.unwrap();
    sleep(Duration::from_millis(20)).await;
    pump.ack(1_000_000);
    assert_eq!(pump.unacked(), 0);
    pump.ack(5);
    assert_eq!(pump.unacked(), 0);
}

#[tokio::test(start_paused = true)]
async fn stopping_while_paused_ends_the_pump() {
    let (tx, _sink, pump) = start();
    flood(&tx, 40);
    sleep(Duration::from_millis(100)).await;
    assert!(pump.unacked() >= HIGH);

    pump.stop();
    timeout(Duration::from_secs(1), pump.finished())
        .await
        .expect("pump should end after stop");
}

#[tokio::test(start_paused = true)]
async fn a_dropped_queue_reports_an_unexpected_end() {
    let (tx, sink, pump) = start();
    tx.send(data(3)).await.unwrap();
    drop(tx);
    pump.finished().await;
    let all = sink.all();
    assert_eq!(all[0], Sent::Data(vec![b'x'; 3]));
    match &all[1] {
        Sent::Status(TerminalStatus::Failed { error }) => {
            assert_eq!(error.code, ErrorCode::Internal);
        }
        other => panic!("expected a failed status, got {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn a_failing_sink_stops_the_pump_and_the_backend() {
    let (tx, sink, pump) = start();
    sink.fail.store(true, Ordering::SeqCst);
    for _ in 0..5 {
        // 5 x 14,000 bytes passes the largest chunk, so the pump tries to send and fails.
        let _ = tx.send(data(14_000)).await;
    }
    pump.finished().await;
    // With the pump gone the queue is closed, so a backend's next send fails and it stops.
    assert!(tx.send(data(1)).await.is_err());
    assert!(tx.is_closed());
}

#[tokio::test(start_paused = true)]
async fn each_session_has_its_own_flow_control() {
    let (busy_tx, busy_sink, busy_pump) = start();
    let (quiet_tx, quiet_sink, _quiet_pump) = start();
    flood(&busy_tx, 40);
    sleep(Duration::from_millis(100)).await;
    assert!(busy_pump.unacked() >= HIGH);

    quiet_tx.send(data(7)).await.unwrap();
    sleep(Duration::from_millis(20)).await;
    assert_eq!(
        quiet_sink.data_total(),
        7,
        "a stopped pump held up another session"
    );
    assert_eq!(busy_sink.data_total(), 8 * MAX_CHUNK);
}
