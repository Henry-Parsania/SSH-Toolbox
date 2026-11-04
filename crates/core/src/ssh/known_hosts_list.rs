//! Listing and removing entries of the `known_hosts` file (task 7.7).
//!
//! The file belongs to OpenSSH too, so nothing is ever rewritten except to take out the one line
//! the user chose, and a copy of the file is kept next to it as `known_hosts.bak` first.

use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::config::write_atomic;
use crate::error::{AppError, ErrorCode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KnownHostEntry {
    /// The line in the file, counting from 1.
    pub line: usize,
    /// The host names the key is for, as written. `(hashed)` when the file hides them.
    pub hosts: String,
    pub algorithm: String,
    /// `SHA256:...`, the same text `ssh-keygen -l` shows.
    pub fingerprint: String,
    /// `@cert-authority` or `@revoked`, or empty.
    pub marker: String,
}

fn fingerprint(blob_base64: &str) -> Option<String> {
    let blob = STANDARD.decode(blob_base64).ok()?;
    Some(format!(
        "SHA256:{}",
        STANDARD_NO_PAD.encode(Sha256::digest(blob))
    ))
}

fn parse_line(number: usize, line: &str) -> Option<KnownHostEntry> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let mut parts = line.split_whitespace();
    let mut first = parts.next()?;
    let mut marker = String::new();
    if first.starts_with('@') {
        marker = first.to_string();
        first = parts.next()?;
    }
    let algorithm = parts.next()?.to_string();
    let key = parts.next()?;
    Some(KnownHostEntry {
        line: number,
        hosts: if first.starts_with('|') {
            "(hashed)".into()
        } else {
            first.to_string()
        },
        algorithm,
        fingerprint: fingerprint(key)?,
        marker,
    })
}

/// The entries of the file. A missing file is an empty list; lines that can't be understood are
/// left out of the list (and are never touched).
pub fn list(path: &Path) -> Result<Vec<KnownHostEntry>, AppError> {
    let text = match std::fs::read(path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => {
            return Err(AppError::new(
                ErrorCode::Internal,
                "The file of known hosts can't be read.",
            ));
        }
    };
    Ok(text
        .lines()
        .enumerate()
        .filter_map(|(i, l)| parse_line(i + 1, l))
        .collect())
}

/// Takes out line `line` if it still has the fingerprint the user saw (so a change made in the
/// meantime can't make the wrong line go). Keeps a copy first.
pub fn remove(path: &Path, line: usize, fingerprint_seen: &str) -> Result<(), AppError> {
    let bytes = std::fs::read(path).map_err(|_| {
        AppError::new(
            ErrorCode::NotFound,
            "The file of known hosts can't be read.",
        )
    })?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let mut lines: Vec<&str> = text.split_inclusive('\n').collect();
    let matches = line >= 1
        && lines
            .get(line - 1)
            .and_then(|l| parse_line(line, l.trim_end_matches(['\r', '\n'])))
            .is_some_and(|e| e.fingerprint == fingerprint_seen);
    if !matches {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "That entry has changed since the list was shown. Refresh and try again.",
        ));
    }
    let mut backup: PathBuf = path.as_os_str().to_owned().into();
    backup.set_extension("bak");
    std::fs::write(&backup, &bytes).map_err(|_| {
        AppError::new(
            ErrorCode::Internal,
            "Couldn't keep a copy of the known hosts, so nothing was removed.",
        )
    })?;
    lines.remove(line - 1);
    write_atomic(path, &lines.concat())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Not real servers: any valid base64 will do for the listing.
    const KEY_A: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIPbm3d0dJ8o4o4eB0o1l3o3u3z6a4n8t0Q2M5v9m1p7k";
    const KEY_B: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    fn file(text: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        std::fs::write(&path, text).unwrap();
        (dir, path)
    }

    fn sample() -> String {
        format!(
            "# a comment\n[box.example.com]:2222,10.0.0.5 ssh-ed25519 {KEY_A} me\n\n|1|abc=|def= ssh-ed25519 {KEY_B}\n@cert-authority *.example.com ssh-ed25519 {KEY_A}\nnot a valid line\n"
        )
    }

    #[test]
    fn the_list_shows_hosts_fingerprints_and_hides_nothing_else() {
        let (_dir, path) = file(&sample());
        let entries = list(&path).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].line, 2);
        assert_eq!(entries[0].hosts, "[box.example.com]:2222,10.0.0.5");
        assert!(entries[0].fingerprint.starts_with("SHA256:"));
        assert_eq!(entries[1].hosts, "(hashed)");
        assert_eq!(entries[2].marker, "@cert-authority");
    }

    #[test]
    fn a_missing_file_is_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list(&dir.path().join("nope")).unwrap().is_empty());
    }

    #[test]
    fn removing_takes_out_only_that_line_and_keeps_a_copy() {
        let (_dir, path) = file(&sample());
        let seen = list(&path).unwrap()[1].clone();
        remove(&path, seen.line, &seen.fingerprint).unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        let before = sample();
        assert_eq!(after.lines().count(), before.lines().count() - 1);
        assert!(after.contains("[box.example.com]:2222") && after.contains("not a valid line"));
        assert!(!after.contains("|1|abc="));
        assert_eq!(
            std::fs::read_to_string(path.with_extension("bak")).unwrap(),
            before
        );
    }

    #[test]
    fn a_line_that_changed_since_it_was_listed_is_not_removed() {
        let (_dir, path) = file(&sample());
        let seen = list(&path).unwrap()[0].clone();
        assert!(remove(&path, seen.line, "SHA256:something-else").is_err());
        assert!(remove(&path, 999, &seen.fingerprint).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), sample());
    }
}
