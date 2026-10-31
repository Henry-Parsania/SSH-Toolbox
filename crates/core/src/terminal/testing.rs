//! Fakes shared by the terminal tests.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_trait::async_trait;

use super::backend::{TermSize, TerminalBackend};
use super::sink::{OutputSink, TerminalStatus};
use crate::error::{AppError, ErrorCode};

#[derive(Debug, Clone, PartialEq)]
pub enum Sent {
    Data(Vec<u8>),
    Status(TerminalStatus),
}

/// Records everything the pump delivers, in order.
#[derive(Default)]
pub struct RecordingSink {
    sent: Mutex<Vec<Sent>>,
    pub fail: AtomicBool,
}

impl RecordingSink {
    pub fn all(&self) -> Vec<Sent> {
        self.sent.lock().unwrap().clone()
    }

    /// The size of each data chunk, in order.
    pub fn chunk_sizes(&self) -> Vec<usize> {
        self.all()
            .iter()
            .filter_map(|s| match s {
                Sent::Data(d) => Some(d.len()),
                Sent::Status(_) => None,
            })
            .collect()
    }

    pub fn data_total(&self) -> usize {
        self.chunk_sizes().iter().sum()
    }

    pub fn statuses(&self) -> Vec<TerminalStatus> {
        self.all()
            .into_iter()
            .filter_map(|s| match s {
                Sent::Status(s) => Some(s),
                Sent::Data(_) => None,
            })
            .collect()
    }
}

impl OutputSink for RecordingSink {
    fn send_data(&self, chunk: Vec<u8>) -> Result<(), AppError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(AppError::new(ErrorCode::Internal, "sink failed"));
        }
        self.sent.lock().unwrap().push(Sent::Data(chunk));
        Ok(())
    }

    fn send_status(&self, status: TerminalStatus) -> Result<(), AppError> {
        self.sent.lock().unwrap().push(Sent::Status(status));
        Ok(())
    }
}

/// A backend that only counts what it is asked to do.
#[derive(Default)]
pub struct FakeBackend {
    pub written: Mutex<Vec<u8>>,
    pub resizes: Mutex<Vec<TermSize>>,
    pub closes: AtomicUsize,
}

#[async_trait]
impl TerminalBackend for FakeBackend {
    async fn write(&self, data: &[u8]) -> Result<(), AppError> {
        self.written.lock().unwrap().extend_from_slice(data);
        Ok(())
    }

    async fn resize(&self, size: TermSize) -> Result<(), AppError> {
        self.resizes.lock().unwrap().push(size);
        Ok(())
    }

    async fn close(&self) -> Result<(), AppError> {
        self.closes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
