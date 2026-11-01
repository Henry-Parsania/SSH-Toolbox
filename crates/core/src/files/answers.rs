//! Holds the questions "this file exists, what now?" until the user answers them.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use tokio::sync::oneshot;

use super::backend::FileEntry;
use super::queue::{JobSnapshot, OverwriteChoice, OverwriteResolver};
use crate::error::{AppError, ErrorCode};

#[derive(Default)]
pub struct PendingAnswers {
    waiting: Mutex<HashMap<u64, oneshot::Sender<OverwriteChoice>>>,
}

impl PendingAnswers {
    /// The user's answer for a job. An unknown job (already answered or cancelled) is an error.
    pub fn respond(&self, job_id: u64, choice: OverwriteChoice) -> Result<(), AppError> {
        match self.waiting.lock().unwrap().remove(&job_id) {
            Some(sender) => {
                let _ = sender.send(choice);
                Ok(())
            }
            None => Err(AppError::new(
                ErrorCode::NotFound,
                "That question has already been answered or cancelled.",
            )),
        }
    }
}

#[async_trait]
impl OverwriteResolver for PendingAnswers {
    async fn resolve(&self, job: &JobSnapshot, _existing: &FileEntry) -> OverwriteChoice {
        let (tx, rx) = oneshot::channel();
        self.waiting.lock().unwrap().insert(job.id, tx);
        // Nothing answering means skip: it's the choice that can't lose anything.
        rx.await.unwrap_or(OverwriteChoice::Skip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: u64) -> JobSnapshot {
        JobSnapshot {
            id,
            kind: super::super::queue::JobKind::Copy,
            name: "f".into(),
            from: "a".into(),
            to: "b".into(),
            state: super::super::queue::JobState::Asking,
            done: 0,
            total: None,
        }
    }

    fn entry() -> FileEntry {
        FileEntry {
            name: "f".into(),
            path: "b".into(),
            kind: super::super::backend::FileKind::File,
            size: Some(1),
            modified: None,
            permissions: None,
        }
    }

    #[tokio::test]
    async fn the_answer_reaches_the_waiting_job_once() {
        let answers = std::sync::Arc::new(PendingAnswers::default());
        let waiting = {
            let answers = answers.clone();
            tokio::spawn(async move { answers.resolve(&job(7), &entry()).await })
        };
        for _ in 0..200 {
            if answers.respond(7, OverwriteChoice::OverwriteAll).is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert_eq!(waiting.await.unwrap(), OverwriteChoice::OverwriteAll);
        assert_eq!(
            answers.respond(7, OverwriteChoice::Skip).unwrap_err().code,
            ErrorCode::NotFound
        );
    }

    #[test]
    fn answering_a_made_up_job_is_an_error() {
        let answers = PendingAnswers::default();
        assert!(answers.respond(99, OverwriteChoice::Skip).is_err());
    }
}
