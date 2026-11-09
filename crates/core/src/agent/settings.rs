//! How the agent behaves, as the user has set it: the instructions it is given, how long a task may
//! run, and whether commands that are normally blocked may be proposed. Stored next to the
//! providers. Only the user changes these (from the settings window); the model never can.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::config::{set_aside, write_atomic};
use crate::error::{AppError, ErrorCode};

pub const DEFAULT_MAX_ROUNDS: u32 = 15;
pub const DEFAULT_TOKEN_CAP: u32 = 400_000;
const MAX_PROMPT: usize = 20_000;
const MAX_EXTRA: usize = 4_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSettings {
    /// Replaces the built-in instructions. Empty means "use the built-in ones".
    #[serde(default)]
    pub system_prompt: String,
    /// Added after the instructions, for example "answer in German".
    #[serde(default)]
    pub extra_instructions: String,
    /// The most steps (model calls) in one task.
    #[serde(default = "default_rounds")]
    pub max_rounds: u32,
    /// Stops a task that has used this many tokens in all.
    #[serde(default = "default_cap")]
    pub token_cap: u32,
    /// Lets commands that are normally blocked (wiping a disk, `rm -rf /`...) be proposed. They
    /// still always need the user's approval, one by one.
    #[serde(default)]
    pub allow_dangerous: bool,
}

fn default_rounds() -> u32 {
    DEFAULT_MAX_ROUNDS
}
fn default_cap() -> u32 {
    DEFAULT_TOKEN_CAP
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            system_prompt: String::new(),
            extra_instructions: String::new(),
            max_rounds: DEFAULT_MAX_ROUNDS,
            token_cap: DEFAULT_TOKEN_CAP,
            allow_dangerous: false,
        }
    }
}

fn invalid(message: &str) -> AppError {
    AppError::new(ErrorCode::InvalidInput, message)
}

fn clean_text(text: &str) -> bool {
    text.chars()
        .all(|c| !c.is_control() || c == '\n' || c == '\t' || c == '\r')
}

impl AgentSettings {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.system_prompt.chars().count() > MAX_PROMPT || !clean_text(&self.system_prompt) {
            return Err(invalid(
                "The instructions are too long (at most 20,000 characters).",
            ));
        }
        if self.extra_instructions.chars().count() > MAX_EXTRA
            || !clean_text(&self.extra_instructions)
        {
            return Err(invalid(
                "The extra instructions are too long (at most 4,000 characters).",
            ));
        }
        if !(1..=100).contains(&self.max_rounds) {
            return Err(invalid("Steps per task must be between 1 and 100."));
        }
        if !(10_000..=5_000_000).contains(&self.token_cap) {
            return Err(invalid(
                "The token limit must be between 10,000 and 5,000,000.",
            ));
        }
        Ok(())
    }
}

pub struct AgentSettingsStore {
    path: PathBuf,
    value: Mutex<AgentSettings>,
}

impl AgentSettingsStore {
    /// Reads the file. A file that can't be read is set aside and the defaults are used.
    pub fn load(path: &Path) -> Self {
        let value = match std::fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str::<AgentSettings>(&text) {
                Ok(s) if s.validate().is_ok() => s,
                _ => {
                    set_aside(path);
                    AgentSettings::default()
                }
            },
            Err(_) => AgentSettings::default(),
        };
        Self {
            path: path.to_path_buf(),
            value: Mutex::new(value),
        }
    }

    pub fn get(&self) -> AgentSettings {
        self.value.lock().unwrap().clone()
    }

    pub fn set(&self, next: AgentSettings) -> Result<(), AppError> {
        next.validate()?;
        let text = serde_json::to_string_pretty(&next)
            .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't save the agent settings."))?;
        write_atomic(&self.path, &text)?;
        *self.value.lock().unwrap() = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_safe_ones() {
        let s = AgentSettings::default();
        assert!(!s.allow_dangerous);
        assert!(s.system_prompt.is_empty());
        assert!(s.validate().is_ok());
    }

    #[test]
    fn limits_are_checked() {
        let mut s = AgentSettings {
            max_rounds: 0,
            ..AgentSettings::default()
        };
        assert!(s.validate().is_err());
        s.max_rounds = 101;
        assert!(s.validate().is_err());
        s.max_rounds = 30;
        s.token_cap = 5;
        assert!(s.validate().is_err());
        s.token_cap = 1_000_000;
        s.extra_instructions = "x".repeat(4001);
        assert!(s.validate().is_err());
        s.extra_instructions = "Answer in German.\nBe brief.".into();
        assert!(s.validate().is_ok());
        s.system_prompt = "bad \u{0}".into();
        assert!(s.validate().is_err());
    }

    #[test]
    fn settings_are_saved_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("agent_settings.json");
        let store = AgentSettingsStore::load(&file);
        assert_eq!(store.get(), AgentSettings::default());
        let next = AgentSettings {
            allow_dangerous: true,
            max_rounds: 40,
            ..AgentSettings::default()
        };
        store.set(next.clone()).unwrap();
        assert_eq!(AgentSettingsStore::load(&file).get(), next);
    }

    #[test]
    fn a_bad_save_changes_nothing_and_a_bad_file_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("agent_settings.json");
        let store = AgentSettingsStore::load(&file);
        let bad = AgentSettings {
            max_rounds: 0,
            ..AgentSettings::default()
        };
        assert!(store.set(bad).is_err());
        assert_eq!(store.get(), AgentSettings::default());
        std::fs::write(&file, "{ not json").unwrap();
        assert_eq!(
            AgentSettingsStore::load(&file).get(),
            AgentSettings::default()
        );
    }

    #[test]
    fn an_old_file_without_the_new_fields_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("agent_settings.json");
        std::fs::write(&file, r#"{"extra_instructions":"hi"}"#).unwrap();
        let s = AgentSettingsStore::load(&file).get();
        assert_eq!(s.extra_instructions, "hi");
        assert_eq!(s.max_rounds, DEFAULT_MAX_ROUNDS);
    }
}
