//! The output pump: backend queue -> batching -> sink, with flow control (INTERFACES.md section 4).
//!
//! Each session has its own pump, so one busy session can't hold up another.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::{Notify, mpsc};
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep_until};
use tokio_util::sync::CancellationToken;

use super::backend::BackendEvent;
use super::sink::{OutputSink, TerminalStatus};
use crate::error::{AppError, ErrorCode};

/// Largest chunk sent to the sink. The pump joins smaller pieces up to this size.
pub const MAX_CHUNK: usize = 64 * 1024;
/// The pump sends what it has after this long, even if it is small.
pub const FLUSH_AFTER: Duration = Duration::from_millis(8);
/// Unacknowledged bytes at which the pump stops taking from the queue.
pub const HIGH: usize = 500_000;
/// Unacknowledged bytes at which the pump starts again.
pub const LOW: usize = 100_000;
/// The interface reports processed bytes after this many, or `ACK_DELAY` after the last unsent ack.
pub const ACK_THRESHOLD: usize = 32 * 1024;
pub const ACK_DELAY: Duration = Duration::from_millis(50);

// `LOW` must stay above the ack threshold, or the pump could wait forever for an ack the
// interface is holding back.
const _: () = assert!(LOW > ACK_THRESHOLD);
const _: () = assert!(HIGH > LOW);

#[derive(Debug, Clone, Copy)]
pub struct PumpConfig {
    pub max_chunk: usize,
    pub flush_after: Duration,
    pub high: usize,
    pub low: usize,
}

impl Default for PumpConfig {
    fn default() -> Self {
        Self {
            max_chunk: MAX_CHUNK,
            flush_after: FLUSH_AFTER,
            high: HIGH,
            low: LOW,
        }
    }
}

/// Bytes sent to the interface that it hasn't reported processing yet.
#[derive(Default)]
struct Flow {
    unacked: AtomicUsize,
    notify: Notify,
}

impl Flow {
    fn unacked(&self) -> usize {
        self.unacked.load(Ordering::SeqCst)
    }

    fn sent(&self, bytes: usize) {
        self.unacked.fetch_add(bytes, Ordering::SeqCst);
    }

    /// Never goes below zero, so a wrong or repeated ack can't wrap around.
    fn acked(&self, bytes: usize) {
        let mut current = self.unacked.load(Ordering::SeqCst);
        while let Err(actual) = self.unacked.compare_exchange(
            current,
            current.saturating_sub(bytes),
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            current = actual;
        }
        // `notify_one` keeps a permit if the pump isn't waiting yet, so a wake-up can't be lost.
        self.notify.notify_one();
    }
}

/// The controls for a running pump.
pub struct PumpHandle {
    flow: Arc<Flow>,
    cancel: CancellationToken,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl PumpHandle {
    /// The interface reports that it has processed `bytes` of output.
    pub fn ack(&self, bytes: usize) {
        self.flow.acked(bytes);
    }

    /// Forgets output that was sent but never acknowledged. Used when the window that was
    /// showing a session is gone, so the next window doesn't wait for acknowledgements that will
    /// never come.
    pub fn reset_flow(&self) {
        self.flow.unacked.store(0, Ordering::SeqCst);
        self.flow.notify.notify_one();
    }

    /// Bytes sent but not yet acknowledged.
    pub fn unacked(&self) -> usize {
        self.flow.unacked()
    }

    /// Stops the pump. Output still queued is dropped. Safe to call twice.
    pub fn stop(&self) {
        self.cancel.cancel();
    }

    /// Waits for the pump to finish. Returns at once if it already has, or if called twice.
    pub async fn finished(&self) {
        let task = self.task.lock().expect("pump task lock").take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }
}

impl Drop for PumpHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Starts a pump reading from `queue` and delivering to `sink`.
pub fn spawn_pump(
    queue: mpsc::Receiver<BackendEvent>,
    sink: Arc<dyn OutputSink>,
    config: PumpConfig,
) -> PumpHandle {
    let flow = Arc::new(Flow::default());
    let cancel = CancellationToken::new();
    let task = tokio::spawn(run(queue, sink, Arc::clone(&flow), config, cancel.clone()));
    PumpHandle {
        flow,
        cancel,
        task: Mutex::new(Some(task)),
    }
}

/// Sends everything in `buf` as chunks of at most `max_chunk`. Returns false if the sink failed.
fn flush(buf: &mut Vec<u8>, sink: &dyn OutputSink, flow: &Flow, max_chunk: usize) -> bool {
    for chunk in buf.chunks(max_chunk) {
        let len = chunk.len();
        if sink.send_data(chunk.to_vec()).is_err() {
            return false;
        }
        flow.sent(len);
    }
    buf.clear();
    true
}

/// Waits until unacknowledged bytes fall to `low`. Returns false if the pump was stopped.
async fn wait_until_low(flow: &Flow, low: usize, cancel: &CancellationToken) -> bool {
    while flow.unacked() > low {
        tokio::select! {
            _ = flow.notify.notified() => {}
            _ = cancel.cancelled() => return false,
        }
    }
    true
}

async fn run(
    mut queue: mpsc::Receiver<BackendEvent>,
    sink: Arc<dyn OutputSink>,
    flow: Arc<Flow>,
    config: PumpConfig,
    cancel: CancellationToken,
) {
    let sink = sink.as_ref();
    let mut buf: Vec<u8> = Vec::new();
    let mut deadline: Option<Instant> = None;
    let mut paused = false;

    loop {
        // Stop taking from the queue at HIGH, and start again at LOW. While stopped the queue
        // fills, which stops the backend reading, which slows the running program down.
        if flow.unacked() >= config.high {
            paused = true;
        }
        if paused {
            if !wait_until_low(&flow, config.low, &cancel).await {
                return;
            }
            paused = false;
        }

        let event = match deadline {
            Some(due) => tokio::select! {
                biased;
                _ = cancel.cancelled() => return,
                event = queue.recv() => event,
                _ = sleep_until(due) => {
                    if !flush(&mut buf, sink, &flow, config.max_chunk) {
                        return;
                    }
                    deadline = None;
                    continue;
                }
            },
            None => tokio::select! {
                biased;
                _ = cancel.cancelled() => return,
                event = queue.recv() => event,
            },
        };

        // The last message. Any data already taken goes out first, then the status.
        let status = match event {
            Some(BackendEvent::Data(data)) => {
                if buf.is_empty() {
                    deadline = Some(Instant::now() + config.flush_after);
                }
                buf.extend_from_slice(&data);
                if buf.len() >= config.max_chunk {
                    if !flush(&mut buf, sink, &flow, config.max_chunk) {
                        return;
                    }
                    deadline = None;
                }
                continue;
            }
            Some(BackendEvent::Exited { code }) => TerminalStatus::Exited { code },
            Some(BackendEvent::Failed(error)) => TerminalStatus::Failed { error },
            None => TerminalStatus::Failed {
                error: AppError::new(ErrorCode::Internal, "The session ended unexpectedly."),
            },
        };
        if flush(&mut buf, sink, &flow, config.max_chunk) {
            let _ = sink.send_status(status);
        }
        return;
    }
}
