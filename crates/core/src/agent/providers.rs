//! The language-model services the user has set up: a name, which kind of service it is, its
//! address and the model to ask for. The API key is not here; it lives in the system keychain.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::llm::ProviderKind;
use crate::config::{set_aside, write_atomic};
use crate::error::{AppError, ErrorCode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provider {
    /// Empty when a new one is being saved; filled in by the store.
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    /// For example `https://api.openai.com/v1` or `http://localhost:11434/v1`.
    pub base_url: String,
    pub model: String,
}

fn invalid(message: &str) -> AppError {
    AppError::new(ErrorCode::InvalidInput, message)
}

/// Checks what the user typed. Keys travel in the request headers, so an address that isn't
/// encrypted is only accepted for a service on this very computer.
pub fn validate(p: &Provider) -> Result<(), AppError> {
    let name = p.name.trim();
    if name.is_empty() || name.chars().count() > 60 || name.chars().any(char::is_control) {
        return Err(invalid("Give the provider a short name."));
    }
    let model = p.model.trim();
    if model.is_empty()
        || model.len() > 120
        || model.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(invalid("Enter the model's name, for example gpt-4.1."));
    }
    let url = url::Url::parse(p.base_url.trim()).map_err(|_| {
        invalid("The address isn't valid. It should look like https://api.example.com/v1.")
    })?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid(
            "Don't put a name or password in the address. The key is entered separately.",
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(invalid("The address shouldn't have a ? or # part."));
    }
    let host = url.host_str().unwrap_or("");
    let local = matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1");
    match url.scheme() {
        "https" => Ok(()),
        "http" if local => Ok(()),
        "http" => Err(invalid(
            "Use https:// so the key and your data are encrypted. Plain http:// is only allowed for a service on this computer.",
        )),
        _ => Err(invalid("The address must start with https://.")),
    }
}

pub struct ProviderStore {
    path: PathBuf,
    items: Mutex<Vec<Provider>>,
}

impl ProviderStore {
    /// Reads the file. A file that can't be read is set aside and the list starts empty.
    pub fn load(path: &Path) -> Self {
        let items = match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str::<Vec<Provider>>(&text).unwrap_or_else(|_| {
                set_aside(path);
                Vec::new()
            }),
            Err(_) => Vec::new(),
        };
        Self {
            path: path.to_path_buf(),
            items: Mutex::new(items),
        }
    }

    pub fn list(&self) -> Vec<Provider> {
        self.items.lock().unwrap().clone()
    }

    pub fn get(&self, id: &str) -> Option<Provider> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.id == id)
            .cloned()
    }

    /// Adds a provider, or replaces the one with the same ID.
    pub fn save(&self, mut provider: Provider) -> Result<Provider, AppError> {
        validate(&provider)?;
        provider.name = provider.name.trim().to_string();
        provider.model = provider.model.trim().to_string();
        provider.base_url = provider.base_url.trim().trim_end_matches('/').to_string();
        let mut items = self.items.lock().unwrap();
        if provider.id.is_empty() {
            provider.id = uuid::Uuid::new_v4().to_string();
            items.push(provider.clone());
        } else if let Some(slot) = items.iter_mut().find(|p| p.id == provider.id) {
            *slot = provider.clone();
        } else {
            return Err(AppError::new(
                ErrorCode::NotFound,
                "That provider no longer exists.",
            ));
        }
        self.write(&items)?;
        Ok(provider)
    }

    pub fn delete(&self, id: &str) -> Result<(), AppError> {
        let mut items = self.items.lock().unwrap();
        items.retain(|p| p.id != id);
        self.write(&items)
    }

    fn write(&self, items: &[Provider]) -> Result<(), AppError> {
        let text = serde_json::to_string_pretty(items)
            .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't save the providers."))?;
        write_atomic(&self.path, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(url: &str) -> Provider {
        Provider {
            id: String::new(),
            name: "OpenAI".into(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: url.into(),
            model: "gpt-4.1".into(),
        }
    }

    #[test]
    fn addresses_are_checked() {
        assert!(validate(&provider("https://api.openai.com/v1")).is_ok());
        assert!(validate(&provider("http://localhost:11434/v1")).is_ok());
        assert!(validate(&provider("http://127.0.0.1:8080")).is_ok());
        for bad in [
            "http://api.example.com/v1",
            "ftp://x",
            "not a url",
            "https://user:pw@api.example.com",
            "https://api.example.com/v1?key=abc",
            "",
        ] {
            assert!(validate(&provider(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn providers_are_saved_and_found_again() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("providers.json");
        let store = ProviderStore::load(&file);
        let saved = store.save(provider("https://api.openai.com/v1/")).unwrap();
        assert!(!saved.id.is_empty());
        assert_eq!(saved.base_url, "https://api.openai.com/v1");
        let again = ProviderStore::load(&file);
        assert_eq!(again.get(&saved.id), Some(saved.clone()));
        let mut changed = saved.clone();
        changed.model = "gpt-4.1-mini".into();
        again.save(changed).unwrap();
        assert_eq!(again.list().len(), 1);
        again.delete(&saved.id).unwrap();
        assert!(ProviderStore::load(&file).list().is_empty());
        let mut ghost = provider("https://x.example.com");
        ghost.id = "nope".into();
        assert!(store.save(ghost).is_err());
    }

    #[test]
    fn a_damaged_file_is_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("providers.json");
        std::fs::write(&file, "{not json").unwrap();
        assert!(ProviderStore::load(&file).list().is_empty());
        assert!(!file.exists());
    }
}
