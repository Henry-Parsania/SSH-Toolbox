//! `snippets.toml`: saved commands that can be sent to a terminal (task 7.1).
//! A snippet is plain text. Never put a password in one: the file is not protected.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::{set_aside, write_atomic};
use crate::error::{AppError, ErrorCode};

const MAX_SNIPPETS: usize = 500;
const MAX_NAME: usize = 100;
const MAX_TEXT: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    pub id: String,
    pub name: String,
    /// What is sent. It may have several lines.
    pub text: String,
    /// Press Enter after the text, so the command runs straight away.
    #[serde(default)]
    pub run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Snippets {
    pub version: u32,
    #[serde(rename = "snippet")]
    pub snippets: Vec<Snippet>,
}

impl Default for Snippets {
    fn default() -> Self {
        Self {
            version: 1,
            snippets: Vec::new(),
        }
    }
}

fn invalid(message: &str) -> AppError {
    AppError::new(ErrorCode::InvalidInput, message)
}

impl Snippets {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.snippets.len() > MAX_SNIPPETS {
            return Err(invalid("There are too many snippets."));
        }
        let mut ids = HashSet::new();
        for s in &self.snippets {
            if s.id.is_empty() || s.id.len() > 64 || !ids.insert(s.id.as_str()) {
                return Err(invalid("A snippet has a missing or repeated ID."));
            }
            if s.name.trim().is_empty() || s.name.len() > MAX_NAME || s.name.contains('\n') {
                return Err(invalid("A snippet needs a short one-line name."));
            }
            if s.text.is_empty() || s.text.len() > MAX_TEXT {
                return Err(invalid(
                    "A snippet needs some text, up to 10,000 characters.",
                ));
            }
            // A stray control character (such as Escape) could do surprising things in a terminal.
            if s.text
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
            {
                return Err(invalid("A snippet can't contain control characters."));
            }
        }
        Ok(())
    }
}

/// Reads the snippets. A missing or unreadable file gives an empty list; an unreadable one is
/// moved aside to `snippets.toml.bad` so it isn't lost.
pub fn load(path: &Path) -> Snippets {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Snippets::default();
    };
    match toml::from_str::<Snippets>(&text) {
        Ok(s) if s.validate().is_ok() => s,
        _ => {
            set_aside(path);
            Snippets::default()
        }
    }
}

pub fn save(path: &Path, snippets: &Snippets) -> Result<(), AppError> {
    snippets.validate()?;
    let text = toml::to_string_pretty(snippets)
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't save the snippets."))?;
    write_atomic(path, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(id: &str, text: &str) -> Snippet {
        Snippet {
            id: id.into(),
            name: format!("snippet {id}"),
            text: text.into(),
            run: true,
        }
    }

    fn path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snippets.toml");
        (dir, path)
    }

    #[test]
    fn snippets_save_and_come_back_with_several_lines() {
        let (_dir, path) = path();
        let s = Snippets {
            version: 1,
            snippets: vec![one("a", "ls -la"), one("b", "cd /srv\ndocker compose ps")],
        };
        save(&path, &s).unwrap();
        assert_eq!(load(&path), s);
    }

    #[test]
    fn no_file_is_an_empty_list() {
        let (_dir, path) = path();
        assert!(load(&path).snippets.is_empty());
    }

    #[test]
    fn bad_snippets_are_refused() {
        let mut s = Snippets::default();
        s.snippets.push(one("a", ""));
        assert!(s.validate().is_err());
        s.snippets = vec![one("a", "x"), one("a", "y")];
        assert!(s.validate().is_err());
        s.snippets = vec![one("a", "echo \u{1b}[31m")];
        assert!(s.validate().is_err());
        s.snippets = vec![one("a", &"x".repeat(MAX_TEXT + 1))];
        assert!(s.validate().is_err());
        s.snippets = vec![Snippet {
            name: "  ".into(),
            ..one("a", "x")
        }];
        assert!(s.validate().is_err());
    }

    #[test]
    fn a_broken_file_is_moved_aside_not_lost() {
        let (dir, path) = path();
        std::fs::write(&path, "this is not toml [[").unwrap();
        assert!(load(&path).snippets.is_empty());
        assert!(dir.path().join("snippets.toml.bad").exists());
    }
}
