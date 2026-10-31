//! Questions the backend asks the user while connecting (INTERFACES.md section 7, "Prompts").
//!
//! The backend sends a `prompt` status and waits. The interface answers with `prompt_respond`.
//! Closing the session cancels anything still waiting.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::error::{AppError, ErrorCode};
use crate::secret::Secret;
use crate::terminal::{OutputSink, TerminalStatus};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Prompt {
    HostKeyUnknown {
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
    },
    HostKeyNewType {
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
        known_algorithms: Vec<String>,
    },
    Password {
        user: String,
        host: String,
        offer_save: bool,
    },
    Passphrase {
        key_path: String,
        offer_save: bool,
    },
    FtpsCertificate {
        host: String,
        reason: String,
        fingerprint: String,
        subject: String,
        expires: String,
    },
    PlainFtp {
        host: String,
    },
}

/// The user's answer.
#[derive(Debug, Deserialize)]
pub struct PromptResponse {
    /// Yes (true) or no/cancel (false).
    pub accepted: bool,
    /// What was typed, for password and passphrase prompts.
    #[serde(default)]
    pub text: Option<Secret>,
    /// Whether to remember the secret in the keychain.
    #[serde(default)]
    pub save: bool,
}

/// All the prompts waiting for an answer, across sessions.
#[derive(Default)]
pub struct PromptBroker {
    waiting: Mutex<HashMap<String, oneshot::Sender<PromptResponse>>>,
}

impl PromptBroker {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// The interface's answer. An unknown ID (already answered or cancelled) is an error.
    pub fn respond(&self, prompt_id: &str, response: PromptResponse) -> Result<(), AppError> {
        let sender = self.waiting.lock().unwrap().remove(prompt_id);
        match sender {
            Some(sender) => {
                // The asker may have just been cancelled; that is not the user's problem.
                let _ = sender.send(response);
                Ok(())
            }
            None => Err(AppError::new(
                ErrorCode::NotFound,
                "That question has already been answered or cancelled.",
            )),
        }
    }

    pub fn pending(&self) -> usize {
        self.waiting.lock().unwrap().len()
    }
}

/// What one connection uses to ask questions: it knows where to send them and when to give up.
#[derive(Clone)]
pub struct Prompter {
    broker: Arc<PromptBroker>,
    sink: Arc<dyn OutputSink>,
    cancel: CancellationToken,
}

impl Prompter {
    pub fn new(
        broker: Arc<PromptBroker>,
        sink: Arc<dyn OutputSink>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            broker,
            sink,
            cancel,
        }
    }

    /// Shows a prompt and waits for the answer. If the session is closed meanwhile, the prompt
    /// is dropped and this gives `Cancelled`.
    pub async fn ask(&self, prompt: Prompt) -> Result<PromptResponse, AppError> {
        let cancelled = || AppError::new(ErrorCode::Cancelled, "The connection was cancelled.");
        let prompt_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.broker
            .waiting
            .lock()
            .unwrap()
            .insert(prompt_id.clone(), tx);

        let sent = self.sink.send_status(TerminalStatus::Prompt {
            prompt_id: prompt_id.clone(),
            prompt,
        });
        if sent.is_err() {
            self.broker.waiting.lock().unwrap().remove(&prompt_id);
            return Err(cancelled());
        }

        let outcome = tokio::select! {
            answer = rx => answer.map_err(|_| cancelled()),
            _ = self.cancel.cancelled() => Err(cancelled()),
        };
        self.broker.waiting.lock().unwrap().remove(&prompt_id);
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::testing::{RecordingSink, Sent};
    use std::time::Duration;

    fn setup() -> (
        Arc<PromptBroker>,
        Arc<RecordingSink>,
        CancellationToken,
        Prompter,
    ) {
        let broker = PromptBroker::new();
        let sink = Arc::new(RecordingSink::default());
        let cancel = CancellationToken::new();
        let prompter = Prompter::new(broker.clone(), sink.clone(), cancel.clone());
        (broker, sink, cancel, prompter)
    }

    fn sample() -> Prompt {
        Prompt::HostKeyUnknown {
            host: "example.test".into(),
            port: 22,
            algorithm: "ssh-ed25519".into(),
            fingerprint: "SHA256:abc".into(),
        }
    }

    async fn the_prompt_id(sink: &RecordingSink) -> String {
        for _ in 0..200 {
            for item in sink.all() {
                if let Sent::Status(TerminalStatus::Prompt { prompt_id, .. }) = item {
                    return prompt_id;
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("no prompt was sent");
    }

    #[tokio::test]
    async fn a_prompt_reaches_the_interface_and_the_answer_comes_back() {
        let (broker, sink, _cancel, prompter) = setup();
        let asker = tokio::spawn(async move { prompter.ask(sample()).await });
        let id = the_prompt_id(&sink).await;
        assert_eq!(broker.pending(), 1);

        broker
            .respond(
                &id,
                PromptResponse {
                    accepted: true,
                    text: None,
                    save: false,
                },
            )
            .unwrap();
        let answer = asker.await.unwrap().unwrap();
        assert!(answer.accepted);
        assert_eq!(broker.pending(), 0);
    }

    #[tokio::test]
    async fn a_typed_secret_comes_back_for_a_password_prompt() {
        let (broker, sink, _cancel, prompter) = setup();
        let asker = tokio::spawn(async move {
            prompter
                .ask(Prompt::Password {
                    user: "alex".into(),
                    host: "h".into(),
                    offer_save: true,
                })
                .await
        });
        let id = the_prompt_id(&sink).await;
        let response: PromptResponse =
            serde_json::from_str(r#"{"accepted":true,"text":"s3cret","save":true}"#).unwrap();
        broker.respond(&id, response).unwrap();
        let answer = asker.await.unwrap().unwrap();
        assert_eq!(answer.text.unwrap().expose(), "s3cret");
        assert!(answer.save);
    }

    #[tokio::test]
    async fn closing_the_session_cancels_a_waiting_prompt() {
        let (broker, sink, cancel, prompter) = setup();
        let asker = tokio::spawn(async move { prompter.ask(sample()).await });
        the_prompt_id(&sink).await;
        cancel.cancel();
        let err = asker.await.unwrap().unwrap_err();
        assert_eq!(err.code, ErrorCode::Cancelled);
        assert_eq!(broker.pending(), 0);
    }

    #[tokio::test]
    async fn answering_twice_or_a_made_up_id_is_an_error() {
        let (broker, sink, _cancel, prompter) = setup();
        let asker = tokio::spawn(async move { prompter.ask(sample()).await });
        let id = the_prompt_id(&sink).await;
        let yes = || PromptResponse {
            accepted: true,
            text: None,
            save: false,
        };
        broker.respond(&id, yes()).unwrap();
        asker.await.unwrap().unwrap();
        assert_eq!(
            broker.respond(&id, yes()).unwrap_err().code,
            ErrorCode::NotFound
        );
        assert_eq!(
            broker.respond("made-up", yes()).unwrap_err().code,
            ErrorCode::NotFound
        );
    }

    #[tokio::test]
    async fn two_prompts_are_answered_independently() {
        let (broker, sink, _cancel, prompter) = setup();
        let p2 = prompter.clone();
        let first = tokio::spawn(async move { prompter.ask(sample()).await });
        the_prompt_id(&sink).await;
        let second = tokio::spawn(async move { p2.ask(sample()).await });
        for _ in 0..200 {
            if broker.pending() == 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(broker.pending(), 2);
        let ids: Vec<String> = sink
            .all()
            .into_iter()
            .filter_map(|i| match i {
                Sent::Status(TerminalStatus::Prompt { prompt_id, .. }) => Some(prompt_id),
                _ => None,
            })
            .collect();
        // Answer the second one "no" and the first one "yes".
        broker
            .respond(
                &ids[1],
                PromptResponse {
                    accepted: false,
                    text: None,
                    save: false,
                },
            )
            .unwrap();
        broker
            .respond(
                &ids[0],
                PromptResponse {
                    accepted: true,
                    text: None,
                    save: false,
                },
            )
            .unwrap();
        assert!(first.await.unwrap().unwrap().accepted);
        assert!(!second.await.unwrap().unwrap().accepted);
    }

    #[test]
    fn prompts_are_sent_to_the_interface_in_the_documented_shape() {
        let json = serde_json::to_value(sample()).unwrap();
        assert_eq!(json["kind"], "host_key_unknown");
        assert_eq!(json["fingerprint"], "SHA256:abc");
        let new_type = serde_json::to_value(Prompt::HostKeyNewType {
            host: "h".into(),
            port: 22,
            algorithm: "a".into(),
            fingerprint: "f".into(),
            known_algorithms: vec!["x".into()],
        })
        .unwrap();
        assert_eq!(new_type["kind"], "host_key_new_type");
        assert_eq!(new_type["known_algorithms"][0], "x");
    }
}
