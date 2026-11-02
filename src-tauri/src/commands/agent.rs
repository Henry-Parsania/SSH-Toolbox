//! The agent's side of the backend: the user's model providers (and their keys), asking a model,
//! and the tools that act on a server. The key never leaves this file: no command returns it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::State;
use tokio_util::sync::CancellationToken;
use toolbox_core::agent::llm::{self, Endpoint, Message, Reply, ToolDef};
use toolbox_core::agent::policy::{self, Risk};
use toolbox_core::agent::providers::{Provider, ProviderStore};
use toolbox_core::agent::redact::redact;
use toolbox_core::agent::tools::{self, ExecResult};
use toolbox_core::keychain::{Keychain, SecretKind};
use toolbox_core::secret::Secret;
use toolbox_core::ssh::backend::SshServices;
use toolbox_core::{AppError, ErrorCode};

use super::files::Files;

pub struct Agent {
    providers: ProviderStore,
    /// Keys kept only for this run, when the system keychain can't hold them.
    memory_keys: Mutex<HashMap<String, String>>,
    /// Requests that are waiting for a model, so they can be stopped.
    runs: Mutex<HashMap<String, CancellationToken>>,
}

impl Agent {
    pub fn new(file: &std::path::Path) -> Self {
        Self {
            providers: ProviderStore::load(file),
            memory_keys: Mutex::default(),
            runs: Mutex::default(),
        }
    }

    fn account(id: &str) -> String {
        format!("agent-{id}")
    }

    async fn key(&self, keychain: &Keychain, id: &str) -> Option<Secret> {
        let remembered = self.memory_keys.lock().unwrap().get(id).cloned();
        if let Some(k) = remembered {
            return Some(Secret::new(k));
        }
        keychain
            .get(&Self::account(id), SecretKind::ApiKey)
            .await
            .ok()
            .flatten()
    }
}

#[derive(Serialize)]
pub struct ProviderView {
    #[serde(flatten)]
    provider: Provider,
    has_key: bool,
    /// True when the key is only kept until the app closes.
    key_for_now_only: bool,
}

async fn view(agent: &Agent, keychain: &Keychain, provider: Provider) -> ProviderView {
    let in_memory = agent.memory_keys.lock().unwrap().contains_key(&provider.id);
    let has_key = in_memory || agent.key(keychain, &provider.id).await.is_some();
    ProviderView {
        provider,
        has_key,
        key_for_now_only: in_memory,
    }
}

#[tauri::command]
pub async fn agent_providers(
    agent: State<'_, Agent>,
    keychain: State<'_, Arc<Keychain>>,
) -> Result<Vec<ProviderView>, AppError> {
    let mut out = Vec::new();
    for provider in agent.providers.list() {
        out.push(view(&agent, &keychain, provider).await);
    }
    Ok(out)
}

/// Saves a provider. `api_key` is kept in the keychain (or, if that is unavailable, until the app
/// closes); leave it out or empty to keep the key already stored.
#[tauri::command]
pub async fn agent_provider_save(
    agent: State<'_, Agent>,
    keychain: State<'_, Arc<Keychain>>,
    provider: Provider,
    api_key: Option<String>,
) -> Result<ProviderView, AppError> {
    let saved = agent.providers.save(provider)?;
    if let Some(key) = api_key
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
    {
        if key.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "The key can't contain spaces or line breaks.",
            ));
        }
        let stored = keychain
            .set(
                &Agent::account(&saved.id),
                SecretKind::ApiKey,
                Secret::new(key.clone()),
            )
            .await;
        let mut memory = agent.memory_keys.lock().unwrap();
        match stored {
            Ok(()) => {
                memory.remove(&saved.id);
            }
            Err(_) => {
                memory.insert(saved.id.clone(), key);
            }
        }
    }
    Ok(view(&agent, &keychain, saved).await)
}

#[tauri::command]
pub async fn agent_provider_delete(
    agent: State<'_, Agent>,
    keychain: State<'_, Arc<Keychain>>,
    id: String,
) -> Result<(), AppError> {
    agent.providers.delete(&id)?;
    agent.memory_keys.lock().unwrap().remove(&id);
    let _ = keychain
        .delete(&Agent::account(&id), SecretKind::ApiKey)
        .await;
    Ok(())
}

/// Asks the model one question and returns its answer (text and any tool calls).
#[tauri::command]
pub async fn agent_complete(
    agent: State<'_, Agent>,
    keychain: State<'_, Arc<Keychain>>,
    request_id: String,
    provider_id: String,
    system: String,
    messages: Vec<Message>,
    tools: Vec<ToolDef>,
) -> Result<Reply, AppError> {
    let provider = agent
        .providers
        .get(&provider_id)
        .ok_or_else(|| AppError::new(ErrorCode::NotFound, "That provider no longer exists."))?;
    let key = agent.key(&keychain, &provider_id).await;
    let token = CancellationToken::new();
    agent
        .runs
        .lock()
        .unwrap()
        .insert(request_id.clone(), token.clone());
    let reply = llm::complete(
        &Endpoint {
            kind: provider.kind,
            base_url: &provider.base_url,
            model: &provider.model,
        },
        key.as_ref(),
        &system,
        &messages,
        &tools,
        &token,
    )
    .await;
    agent.runs.lock().unwrap().remove(&request_id);
    reply
}

/// Stops a request that is waiting for a model.
#[tauri::command]
pub fn agent_cancel(agent: State<'_, Agent>, request_id: String) {
    if let Some(token) = agent.runs.lock().unwrap().get(&request_id) {
        token.cancel();
    }
}

/// How risky a command is, for the screen to show before asking.
#[tauri::command]
pub fn agent_classify(command: String) -> Risk {
    policy::classify(&command)
}

/// Text with secrets removed, for anything the screen sends to the model itself.
#[tauri::command]
pub fn agent_redact(text: String) -> String {
    redact(&text)
}

/// Runs a command on the server of an open SSH session. Commands that change things need
/// `approved`; blocked ones never run.
#[tauri::command]
pub async fn agent_exec(
    services: State<'_, Arc<SshServices>>,
    saved_session_id: String,
    command: String,
    approved: bool,
) -> Result<ExecResult, AppError> {
    let shared = services
        .pool
        .existing(&saved_session_id)
        .await
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::ConnectionLost,
                "That SSH session isn't connected.",
            )
        })?;
    tools::run_command(&shared.handle, &command, approved).await
}

#[tauri::command]
pub async fn agent_read_file(
    files: State<'_, Files>,
    files_id: String,
    path: String,
    approved: bool,
) -> Result<String, AppError> {
    let backend = files.backend(&files_id)?;
    tools::read_text_file(&*backend, &path, approved).await
}

#[tauri::command]
pub async fn agent_write_file(
    files: State<'_, Files>,
    files_id: String,
    path: String,
    content: String,
    approved: bool,
) -> Result<(), AppError> {
    let backend = files.backend(&files_id)?;
    tools::write_text_file(&*backend, &path, &content, approved).await
}
