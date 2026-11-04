//! `sessions.toml`: saved sessions and their folders (INTERFACES.md section 9).
//!
//! Passwords and passphrases never appear here: there is no field to hold one, and anything
//! unknown in a hand-edited file is dropped the next time the file is saved.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::{set_aside, write_atomic};
use crate::error::{AppError, ErrorCode};

const MAX_ITEMS: usize = 5000;
const MAX_TEXT: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
    /// Empty means top level.
    #[serde(default)]
    pub parent: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SshAuth {
    Password,
    Key,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Parity {
    None,
    Odd,
    Even,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowControl {
    None,
    Hardware,
    Software,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineEnding {
    Cr,
    Lf,
    Crlf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FtpSecurity {
    Plain,
    Explicit,
    Implicit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SavedSession {
    Ssh {
        id: String,
        name: String,
        #[serde(default)]
        folder: String,
        host: String,
        port: u16,
        user: String,
        auth: SshAuth,
        /// Where the private key is. The key itself is never copied (SEC5).
        #[serde(default)]
        key_path: String,
        /// The ID of another SSH session to go through, or empty.
        #[serde(default)]
        jump: String,
        /// Show windows of programs run on the server on this computer.
        #[serde(default)]
        x11: bool,
        /// Tab and terminal colour as "#rrggbb", or empty to follow the theme.
        #[serde(default)]
        color: String,
    },
    Serial {
        id: String,
        name: String,
        #[serde(default)]
        folder: String,
        port: String,
        baud: u32,
        data_bits: u8,
        parity: Parity,
        stop_bits: u8,
        flow_control: FlowControl,
        #[serde(default)]
        local_echo: bool,
        line_ending: LineEnding,
    },
    Ftp {
        id: String,
        name: String,
        #[serde(default)]
        folder: String,
        host: String,
        port: u16,
        user: String,
        security: FtpSecurity,
        /// Set when the user accepts a certificate (SEC8).
        #[serde(default)]
        trusted_cert_sha256: String,
    },
}

impl SavedSession {
    pub fn id(&self) -> &str {
        match self {
            Self::Ssh { id, .. } | Self::Serial { id, .. } | Self::Ftp { id, .. } => id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Ssh { name, .. } | Self::Serial { name, .. } | Self::Ftp { name, .. } => name,
        }
    }

    pub fn folder(&self) -> &str {
        match self {
            Self::Ssh { folder, .. } | Self::Serial { folder, .. } | Self::Ftp { folder, .. } => {
                folder
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionsFile {
    pub version: u32,
    #[serde(rename = "folder")]
    pub folders: Vec<Folder>,
    #[serde(rename = "session")]
    pub sessions: Vec<SavedSession>,
}

impl Default for SessionsFile {
    fn default() -> Self {
        Self {
            version: 1,
            folders: Vec::new(),
            sessions: Vec::new(),
        }
    }
}

fn invalid(message: &str) -> AppError {
    AppError::new(ErrorCode::InvalidInput, message)
}

fn clean_text(text: &str) -> bool {
    text.len() <= MAX_TEXT && !text.chars().any(|c| c.is_control())
}

impl SessionsFile {
    /// Checks a file from the interface, or one that may have been edited by hand.
    pub fn validate(&self) -> Result<(), AppError> {
        if self.folders.len() > MAX_ITEMS || self.sessions.len() > MAX_ITEMS {
            return Err(invalid("There are too many saved sessions."));
        }

        let mut ids = HashSet::new();
        for id in self
            .folders
            .iter()
            .map(|f| f.id.as_str())
            .chain(self.sessions.iter().map(|s| s.id()))
        {
            if id.is_empty() || !clean_text(id) || !ids.insert(id) {
                return Err(invalid("Every session and folder needs its own ID."));
            }
        }

        let folder_ids: HashSet<&str> = self.folders.iter().map(|f| f.id.as_str()).collect();
        let parents: HashMap<&str, &str> = self
            .folders
            .iter()
            .map(|f| (f.id.as_str(), f.parent.as_str()))
            .collect();
        for folder in &self.folders {
            if !clean_text(&folder.name) || folder.name.trim().is_empty() {
                return Err(invalid("A folder needs a name."));
            }
            if !folder.parent.is_empty() && !folder_ids.contains(folder.parent.as_str()) {
                return Err(invalid("A folder is inside a folder that doesn't exist."));
            }
            // Walking up must reach the top, never come round to the same folder.
            let mut seen = HashSet::new();
            let mut at = folder.id.as_str();
            while !at.is_empty() {
                if !seen.insert(at) {
                    return Err(invalid("Folders can't be inside each other in a loop."));
                }
                at = parents.get(at).copied().unwrap_or("");
            }
        }

        let ssh_ids: HashSet<&str> = self
            .sessions
            .iter()
            .filter(|s| matches!(s, SavedSession::Ssh { .. }))
            .map(|s| s.id())
            .collect();
        let jumps: HashMap<&str, &str> = self
            .sessions
            .iter()
            .filter_map(|s| match s {
                SavedSession::Ssh { id, jump, .. } => Some((id.as_str(), jump.as_str())),
                _ => None,
            })
            .collect();

        for session in &self.sessions {
            if !clean_text(session.name()) || session.name().trim().is_empty() {
                return Err(invalid("A session needs a name."));
            }
            let folder = session.folder();
            if !folder.is_empty() && !folder_ids.contains(folder) {
                return Err(invalid("A session is in a folder that doesn't exist."));
            }
            match session {
                SavedSession::Ssh {
                    id,
                    host,
                    port,
                    user,
                    key_path,
                    jump,
                    color,
                    ..
                } => {
                    let hex = color.len() == 7
                        && color.starts_with('#')
                        && color[1..].chars().all(|c| c.is_ascii_hexdigit());
                    if !color.is_empty() && !hex {
                        return Err(invalid("A session colour must look like #1a2b3c."));
                    }
                    if host.trim().is_empty()
                        || !clean_text(host)
                        || host.contains(|c: char| c.is_whitespace() || ",[]\"'".contains(c))
                    {
                        return Err(invalid("The host isn't valid."));
                    }
                    if *port == 0 || !clean_text(user) || !clean_text(key_path) {
                        return Err(invalid("The port, user or key path isn't valid."));
                    }
                    if !jump.is_empty() && !ssh_ids.contains(jump.as_str()) {
                        return Err(invalid("The jump host isn't a saved SSH session."));
                    }
                    let mut seen = HashSet::new();
                    let mut at = id.as_str();
                    while !at.is_empty() {
                        if !seen.insert(at) {
                            return Err(invalid("Jump hosts can't go round in a loop."));
                        }
                        at = jumps.get(at).copied().unwrap_or("");
                    }
                }
                SavedSession::Serial {
                    port,
                    baud,
                    data_bits,
                    stop_bits,
                    ..
                } => {
                    if port.trim().is_empty() || !clean_text(port) {
                        return Err(invalid("The serial port isn't valid."));
                    }
                    if *baud == 0 || !(5..=8).contains(data_bits) || !(1..=2).contains(stop_bits) {
                        return Err(invalid("The serial settings aren't valid."));
                    }
                }
                SavedSession::Ftp {
                    host,
                    port,
                    user,
                    trusted_cert_sha256,
                    ..
                } => {
                    if host.trim().is_empty()
                        || !clean_text(host)
                        || host.contains(|c: char| c.is_whitespace() || ",[]\"'".contains(c))
                    {
                        return Err(invalid("The host isn't valid."));
                    }
                    if *port == 0 || !clean_text(user) || !clean_text(trusted_cert_sha256) {
                        return Err(invalid("The port, user or certificate isn't valid."));
                    }
                }
            }
        }
        Ok(())
    }
}

/// What reading the file found.
/// What a restore from a backup file did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MergeResult {
    pub sessions_added: usize,
    pub folders_added: usize,
    /// Sessions in the file that were left out because one with the same ID was already here.
    pub skipped: usize,
}

impl SessionsFile {
    /// The file as text, for a backup.
    pub fn to_toml(&self) -> Result<String, AppError> {
        toml::to_string_pretty(self)
            .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't prepare the backup."))
    }

    /// Reads and checks a backup. Nothing is trusted: it is validated like any saved file.
    pub fn from_toml(text: &str) -> Result<Self, AppError> {
        let file: Self = toml::from_str(text).map_err(|_| {
            AppError::new(
                ErrorCode::InvalidInput,
                "That file isn't a sessions backup from this app.",
            )
        })?;
        file.validate()?;
        Ok(file)
    }

    /// Adds what `other` has that this file doesn't (matched by ID), and keeps what is here.
    /// Pinned FTPS certificates are dropped, so a backup can never make the app trust a
    /// certificate on its own (SEC8): the user accepts it again on first use.
    pub fn merge(&mut self, other: SessionsFile) -> MergeResult {
        let mut result = MergeResult::default();
        let mut folder_ids: HashSet<String> = self.folders.iter().map(|f| f.id.clone()).collect();
        let mut taken: HashSet<String> = folder_ids.clone();
        taken.extend(self.sessions.iter().map(|s| s.id().to_string()));

        // Folders first, in an order where a parent comes before what is inside it.
        let mut waiting: Vec<Folder> = other
            .folders
            .into_iter()
            .filter(|f| !folder_ids.contains(&f.id) && !taken.contains(&f.id))
            .collect();
        loop {
            let before = waiting.len();
            let mut later = Vec::new();
            for f in waiting {
                if f.parent.is_empty() || folder_ids.contains(&f.parent) {
                    folder_ids.insert(f.id.clone());
                    taken.insert(f.id.clone());
                    self.folders.push(f);
                    result.folders_added += 1;
                } else {
                    later.push(f);
                }
            }
            waiting = later;
            if waiting.is_empty() || waiting.len() == before {
                break;
            }
        }

        let ssh_ids: HashSet<String> = self
            .sessions
            .iter()
            .filter(|s| matches!(s, SavedSession::Ssh { .. }))
            .map(|s| s.id().to_string())
            .collect();
        let incoming: Vec<SavedSession> = other
            .sessions
            .into_iter()
            .filter(|s| {
                let fresh = !taken.contains(s.id());
                if !fresh {
                    result.skipped += 1;
                }
                fresh
            })
            .collect();
        let incoming_ssh: HashSet<String> = incoming
            .iter()
            .filter(|s| matches!(s, SavedSession::Ssh { .. }))
            .map(|s| s.id().to_string())
            .collect();
        for mut session in incoming {
            match &mut session {
                SavedSession::Ssh { folder, jump, .. } => {
                    if !folder.is_empty() && !folder_ids.contains(folder.as_str()) {
                        folder.clear();
                    }
                    if !jump.is_empty()
                        && !ssh_ids.contains(jump.as_str())
                        && !incoming_ssh.contains(jump.as_str())
                    {
                        jump.clear();
                    }
                }
                SavedSession::Serial { folder, .. } => {
                    if !folder.is_empty() && !folder_ids.contains(folder.as_str()) {
                        folder.clear();
                    }
                }
                SavedSession::Ftp {
                    folder,
                    trusted_cert_sha256,
                    ..
                } => {
                    if !folder.is_empty() && !folder_ids.contains(folder.as_str()) {
                        folder.clear();
                    }
                    trusted_cert_sha256.clear();
                }
            }
            self.sessions.push(session);
            result.sessions_added += 1;
        }
        result
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Loaded {
    pub file: SessionsFile,
    /// Set when the file exists but can't be used. Nothing is overwritten until the user decides.
    pub problem: Option<String>,
}

/// The saved-sessions file, with its rules about never overwriting what it can't read.
pub struct SessionStore {
    path: PathBuf,
}

impl SessionStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn read(&self) -> Result<Option<SessionsFile>, String> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("The saved sessions file can't be read.".into()),
        };
        let file: SessionsFile = toml::from_str(&text).map_err(|_| {
            "The saved sessions file isn't in a form the app understands.".to_string()
        })?;
        file.validate()
            .map_err(|e| format!("The saved sessions file has a problem: {}", e.message))?;
        Ok(Some(file))
    }

    pub fn load(&self) -> Loaded {
        match self.read() {
            Ok(file) => Loaded {
                file: file.unwrap_or_default(),
                problem: None,
            },
            Err(problem) => Loaded {
                file: SessionsFile::default(),
                problem: Some(problem),
            },
        }
    }

    /// Saves, unless the file on disk exists and can't be read: that one is never overwritten.
    pub fn save(&self, file: &SessionsFile) -> Result<(), AppError> {
        file.validate()?;
        if self.read().is_err() {
            return Err(AppError::new(
                ErrorCode::AlreadyExists,
                "The saved sessions file can't be read, so it wasn't overwritten.",
            ));
        }
        let text = toml::to_string_pretty(file)
            .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't save the sessions."))?;
        write_atomic(&self.path, &text)
    }

    /// The user's choice to start again: the unreadable file is moved to `sessions.toml.bad`.
    pub fn set_aside_unreadable(&self) {
        if self.read().is_err() {
            set_aside(&self.path);
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ssh(id: &str, folder: &str, jump: &str) -> SavedSession {
        SavedSession::Ssh {
            id: id.into(),
            name: format!("host {id}"),
            folder: folder.into(),
            host: "build.example.com".into(),
            port: 22,
            user: "alex".into(),
            auth: SshAuth::Key,
            key_path: "~/.ssh/id_ed25519".into(),
            jump: jump.into(),
            x11: false,
            color: String::new(),
        }
    }

    fn sample() -> SessionsFile {
        SessionsFile {
            version: 1,
            folders: vec![
                Folder {
                    id: "f1".into(),
                    name: "Work".into(),
                    parent: String::new(),
                },
                Folder {
                    id: "f2".into(),
                    name: "Lab".into(),
                    parent: "f1".into(),
                },
            ],
            sessions: vec![
                ssh("s1", "f2", ""),
                ssh("s2", "", "s1"),
                SavedSession::Serial {
                    id: "s3".into(),
                    name: "Router console".into(),
                    folder: String::new(),
                    port: "COM3".into(),
                    baud: 115_200,
                    data_bits: 8,
                    parity: Parity::None,
                    stop_bits: 1,
                    flow_control: FlowControl::None,
                    local_echo: false,
                    line_ending: LineEnding::Cr,
                },
                SavedSession::Ftp {
                    id: "s4".into(),
                    name: "NAS".into(),
                    folder: String::new(),
                    host: "nas.local".into(),
                    port: 21,
                    user: "alex".into(),
                    security: FtpSecurity::Explicit,
                    trusted_cert_sha256: String::new(),
                },
            ],
        }
    }

    fn store() -> (tempfile::TempDir, SessionStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::new(dir.path().join("sessions.toml"));
        (dir, store)
    }

    #[test]
    fn saves_and_reloads_every_kind_of_session() {
        let (_dir, store) = store();
        store.save(&sample()).unwrap();
        let loaded = store.load();
        assert_eq!(loaded.problem, None);
        assert_eq!(loaded.file, sample());
    }

    #[test]
    fn the_file_matches_the_documented_layout() {
        let (_dir, store) = store();
        store.save(&sample()).unwrap();
        let text = std::fs::read_to_string(store.path()).unwrap();
        for expected in [
            "version = 1",
            "[[folder]]",
            "[[session]]",
            "type = \"ssh\"",
            "auth = \"key\"",
            "type = \"serial\"",
            "flow_control = \"none\"",
            "type = \"ftp\"",
            "security = \"explicit\"",
        ] {
            assert!(text.contains(expected), "missing {expected:?} in\n{text}");
        }
    }

    #[test]
    fn no_secret_ever_appears_in_the_file() {
        let (_dir, store) = store();
        // A hand-edited file with a password in it: the password is dropped on the next save.
        let planted = "version = 1\n[[session]]\nid = \"s1\"\nname = \"x\"\ntype = \"ssh\"\n\
                       host = \"h\"\nport = 22\nuser = \"u\"\nauth = \"password\"\n\
                       password = \"hunter2-secret\"\npassphrase = \"also-secret\"\n";
        std::fs::write(store.path(), planted).unwrap();
        let loaded = store.load();
        assert_eq!(loaded.problem, None);
        store.save(&loaded.file).unwrap();
        let text = std::fs::read_to_string(store.path()).unwrap();
        assert!(!text.contains("hunter2"), "{text}");
        assert!(!text.contains("also-secret"), "{text}");
        assert!(!text.to_lowercase().contains("passphrase"), "{text}");
        // The only mention of a password is the name of the login method.
        assert!(!text.contains("password ="), "{text}");
    }

    #[test]
    fn a_missing_file_is_an_empty_list() {
        let (_dir, store) = store();
        let loaded = store.load();
        assert_eq!(loaded.file, SessionsFile::default());
        assert_eq!(loaded.problem, None);
    }

    #[test]
    fn a_corrupt_file_is_reported_and_never_overwritten() {
        let (dir, store) = store();
        std::fs::write(store.path(), "[[session]\nbroken").unwrap();
        let loaded = store.load();
        assert!(loaded.problem.is_some());
        assert_eq!(loaded.file, SessionsFile::default());

        let err = store.save(&sample()).unwrap_err();
        assert_eq!(err.code, ErrorCode::AlreadyExists);
        assert_eq!(
            std::fs::read_to_string(store.path()).unwrap(),
            "[[session]\nbroken"
        );
        assert!(!dir.path().join("sessions.toml.bad").exists());
    }

    #[test]
    fn starting_again_keeps_the_old_file_as_bad() {
        let (dir, store) = store();
        std::fs::write(store.path(), "nonsense = [").unwrap();
        store.set_aside_unreadable();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("sessions.toml.bad")).unwrap(),
            "nonsense = ["
        );
        store.save(&sample()).unwrap();
        assert_eq!(store.load().file, sample());
    }

    #[test]
    fn setting_aside_leaves_a_good_file_alone() {
        let (_dir, store) = store();
        store.save(&sample()).unwrap();
        store.set_aside_unreadable();
        assert_eq!(store.load().file, sample());
    }

    #[test]
    fn the_previous_version_is_kept_as_a_backup() {
        let (dir, store) = store();
        store.save(&sample()).unwrap();
        let mut second = sample();
        second.sessions.pop();
        store.save(&second).unwrap();
        let backup = std::fs::read_to_string(dir.path().join("sessions.toml.bak")).unwrap();
        let old: SessionsFile = toml::from_str(&backup).unwrap();
        assert_eq!(old, sample());
    }

    #[test]
    fn a_file_that_parses_but_is_wrong_counts_as_unreadable() {
        let (_dir, store) = store();
        let mut bad = sample();
        bad.sessions.push(ssh("s1", "", "")); // same ID twice
        let text = toml::to_string_pretty(&bad).unwrap();
        std::fs::write(store.path(), text).unwrap();
        assert!(store.load().problem.is_some());
    }

    #[test]
    fn validation_catches_broken_references_and_loops() {
        let mut f = sample();
        f.sessions[0] = ssh("s1", "nope", "");
        assert!(f.validate().is_err(), "missing folder");

        let mut f = sample();
        f.sessions[1] = ssh("s2", "", "ghost");
        assert!(f.validate().is_err(), "missing jump host");

        let mut f = sample();
        f.sessions[0] = ssh("s1", "f2", "s2"); // s1 -> s2 -> s1
        assert!(f.validate().is_err(), "jump loop");

        let mut f = sample();
        f.sessions[0] = ssh("s1", "", "s1");
        assert!(f.validate().is_err(), "jumps to itself");

        let mut f = sample();
        f.folders[0].parent = "f2".into(); // f1 in f2 in f1
        assert!(f.validate().is_err(), "folder loop");

        let mut f = sample();
        f.sessions[2] = SavedSession::Serial {
            id: "s3".into(),
            name: "x".into(),
            folder: String::new(),
            port: "COM3".into(),
            baud: 0,
            data_bits: 8,
            parity: Parity::None,
            stop_bits: 1,
            flow_control: FlowControl::None,
            local_echo: false,
            line_ending: LineEnding::Cr,
        };
        assert!(f.validate().is_err(), "zero baud");
    }

    #[test]
    fn validation_catches_bad_text_and_ports() {
        let mut f = sample();
        if let SavedSession::Ssh { host, .. } = &mut f.sessions[0] {
            *host = "bad host".into();
        }
        assert!(f.validate().is_err());

        let mut f = sample();
        if let SavedSession::Ssh { port, .. } = &mut f.sessions[0] {
            *port = 0;
        }
        assert!(f.validate().is_err());

        let mut f = sample();
        if let SavedSession::Ssh { name, .. } = &mut f.sessions[0] {
            *name = "line\nbreak".into();
        }
        assert!(f.validate().is_err());

        let mut f = sample();
        if let SavedSession::Ssh { color, .. } = &mut f.sessions[0] {
            *color = "red; background:url(x)".into();
        }
        assert!(f.validate().is_err());
        if let SavedSession::Ssh { color, .. } = &mut f.sessions[0] {
            *color = "#1a2B3c".into();
        }
        assert!(f.validate().is_ok());

        let mut f = sample();
        f.folders[0].name = "  ".into();
        assert!(f.validate().is_err());
    }

    #[test]
    fn restoring_adds_only_what_is_new_and_never_changes_what_is_here() {
        let mut here = sample();
        let before = here.clone();
        let mut backup = sample();
        // One new session, and a changed copy of one that already exists.
        backup.sessions.push(ssh("brand-new", "", ""));
        if let SavedSession::Ssh { host, .. } = &mut backup.sessions[0] {
            *host = "other.example.com".into();
        }
        let result = here.merge(backup);
        assert_eq!(result.sessions_added, 1);
        assert_eq!(result.skipped, before.sessions.len());
        assert_eq!(
            &here.sessions[..before.sessions.len()],
            &before.sessions[..]
        );
        assert!(here.validate().is_ok());
    }

    #[test]
    fn restoring_drops_pinned_certificates_and_dangling_links() {
        let mut here = SessionsFile::default();
        let mut backup = SessionsFile::default();
        backup.sessions.push(SavedSession::Ftp {
            id: "f1".into(),
            name: "ftp".into(),
            folder: "gone".into(),
            host: "h.example.com".into(),
            port: 21,
            user: "u".into(),
            security: FtpSecurity::Explicit,
            trusted_cert_sha256: "ab".repeat(32),
        });
        backup.sessions.push(ssh("s1", "", "missing-jump"));
        let result = here.merge(backup);
        assert_eq!(result.sessions_added, 2);
        match &here.sessions[0] {
            SavedSession::Ftp {
                trusted_cert_sha256,
                folder,
                ..
            } => assert!(trusted_cert_sha256.is_empty() && folder.is_empty()),
            _ => panic!(),
        }
        assert!(here.validate().is_ok(), "{:?}", here.validate());
    }
}
