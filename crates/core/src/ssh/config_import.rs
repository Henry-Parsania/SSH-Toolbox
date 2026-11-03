//! Importing hosts from `~/.ssh/config` (INTERFACES.md section 7, S4).
//!
//! Only `Host`, `HostName`, `Port`, `User`, `IdentityFile` and `ProxyJump` are read. Host
//! patterns (`*`, `?`, `!`) are skipped, `Include` is followed one level, and the user's file is
//! never changed.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::sessions::{SavedSession, SessionsFile, SshAuth};

/// A host as the file describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedHost {
    pub alias: String,
    pub hostname: String,
    pub port: u16,
    pub user: Option<String>,
    pub identity_file: Option<String>,
    /// The raw `ProxyJump` value, such as `bastion` or `me@a:2222,b`.
    pub proxy_jump: Option<String>,
}

#[derive(Default)]
struct Block {
    aliases: Vec<String>,
    hostname: Option<String>,
    port: Option<u16>,
    user: Option<String>,
    identity_file: Option<String>,
    proxy_jump: Option<String>,
}

/// Splits `Keyword value` or `Keyword=value`.
fn split_keyword(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let end = line.find(|c: char| c.is_whitespace() || c == '=')?;
    let key = line[..end].to_ascii_lowercase();
    let rest = line[end..].trim_start_matches(|c: char| c.is_whitespace() || c == '=');
    Some((key, unquote(rest.trim())))
}

fn unquote(value: &str) -> String {
    let v = value.trim();
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

fn is_pattern(alias: &str) -> bool {
    alias.contains(['*', '?', '!'])
}

/// Reads one file's text. Returns the hosts and the `Include` values found.
pub fn parse_config(text: &str) -> (Vec<ImportedHost>, Vec<String>) {
    let mut hosts = Vec::new();
    let mut includes = Vec::new();
    let mut current: Option<Block> = None;
    let mut skipping = false; // Inside a `Match` block.

    let finish = |block: Option<Block>, hosts: &mut Vec<ImportedHost>| {
        let Some(block) = block else { return };
        for alias in block.aliases {
            hosts.push(ImportedHost {
                hostname: block.hostname.clone().unwrap_or_else(|| alias.clone()),
                alias,
                port: block.port.unwrap_or(22),
                user: block.user.clone(),
                identity_file: block.identity_file.clone(),
                proxy_jump: block.proxy_jump.clone(),
            });
        }
    };

    for line in text.lines() {
        let Some((key, value)) = split_keyword(line) else {
            continue;
        };
        match key.as_str() {
            "host" => {
                finish(current.take(), &mut hosts);
                skipping = false;
                let aliases: Vec<String> = value
                    .split_whitespace()
                    .map(unquote)
                    .filter(|a| !a.is_empty() && !is_pattern(a))
                    .collect();
                current = Some(Block {
                    aliases,
                    ..Block::default()
                });
            }
            "match" => {
                finish(current.take(), &mut hosts);
                skipping = true;
            }
            "include" => includes.push(value),
            _ if skipping => {}
            // The first value given for a setting is the one that counts.
            "hostname" => {
                if let Some(b) = current.as_mut().filter(|b| b.hostname.is_none()) {
                    b.hostname = Some(value);
                }
            }
            "port" => {
                if let Some(b) = current.as_mut().filter(|b| b.port.is_none()) {
                    b.port = value.parse().ok().filter(|p| *p != 0);
                }
            }
            "user" => {
                if let Some(b) = current.as_mut().filter(|b| b.user.is_none()) {
                    b.user = Some(value);
                }
            }
            "identityfile" => {
                // Names built with %d, %u and so on can't be known here.
                if let Some(b) = current.as_mut().filter(|b| b.identity_file.is_none())
                    && !value.contains('%')
                {
                    b.identity_file = Some(value);
                }
            }
            "proxyjump" => {
                if let Some(b) = current.as_mut().filter(|b| b.proxy_jump.is_none())
                    && !value.eq_ignore_ascii_case("none")
                {
                    b.proxy_jump = Some(value);
                }
            }
            _ => {}
        }
    }
    finish(current, &mut hosts);
    (hosts, includes)
}

/// Reads a config file and the files it includes, one level deep.
pub fn load_config(config: &Path, ssh_dir: &Path) -> Vec<ImportedHost> {
    let Ok(text) = std::fs::read_to_string(config) else {
        return Vec::new();
    };
    let (mut hosts, includes) = parse_config(&text);
    for pattern in includes {
        for path in expand_include(&pattern, ssh_dir) {
            if let Ok(text) = std::fs::read_to_string(path) {
                // An included file's own `Include` lines are not followed.
                hosts.extend(parse_config(&text).0);
            }
        }
    }
    hosts
}

/// An `Include` path: `~` is the home folder, a relative path is relative to `~/.ssh`, and a `*`
/// in the file name matches any run of characters.
fn expand_include(pattern: &str, ssh_dir: &Path) -> Vec<std::path::PathBuf> {
    let expanded = if let Some(rest) = pattern.strip_prefix("~/") {
        directories::UserDirs::new()
            .map(|d| d.home_dir().join(rest))
            .unwrap_or_else(|| pattern.into())
    } else {
        let p = std::path::PathBuf::from(pattern);
        if p.is_absolute() { p } else { ssh_dir.join(p) }
    };
    let Some(name) = expanded.file_name().and_then(|n| n.to_str()) else {
        return Vec::new();
    };
    if !name.contains('*') {
        return vec![expanded];
    }
    let Some(dir) = expanded.parent() else {
        return Vec::new();
    };
    let (prefix, suffix) = name.split_once('*').unwrap_or((name, ""));
    let mut found: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.len() >= prefix.len() + suffix.len()
                    && n.starts_with(prefix)
                    && n.ends_with(suffix)
            })
        })
        .collect();
    found.sort();
    found
}

/// What the import dialog shows for each host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewItem {
    pub alias: String,
    pub hostname: String,
    pub port: u16,
    pub user: String,
    /// The session is already saved (same host, port and user), so importing it again is skipped.
    pub duplicate: bool,
    /// What it goes through, as written in the file, or empty.
    pub jump: String,
}

fn os_user() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default()
}

fn same_place(s: &SavedSession, host: &str, port: u16, user: &str) -> bool {
    matches!(s, SavedSession::Ssh { host: h, port: p, user: u, .. }
        if h.eq_ignore_ascii_case(host) && *p == port && u == user)
}

pub fn preview(existing: &SessionsFile, hosts: &[ImportedHost]) -> Vec<PreviewItem> {
    let fallback = os_user();
    hosts
        .iter()
        .map(|h| {
            let user = h.user.clone().unwrap_or_else(|| fallback.clone());
            PreviewItem {
                duplicate: existing
                    .sessions
                    .iter()
                    .any(|s| same_place(s, &h.hostname, h.port, &user)),
                alias: h.alias.clone(),
                hostname: h.hostname.clone(),
                port: h.port,
                user,
                jump: h.proxy_jump.clone().unwrap_or_default(),
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportResult {
    pub added: Vec<String>,
    /// Hosts left out, each with the reason.
    pub skipped: Vec<String>,
}

/// One hop of a `ProxyJump`: `user@host:port`, with the user and port optional.
fn parse_hop(text: &str) -> (Option<String>, String, Option<u16>) {
    let (user, rest) = match text.split_once('@') {
        Some((u, r)) => (Some(u.to_string()), r),
        None => (None, text),
    };
    match rest.rsplit_once(':') {
        Some((h, p)) if p.parse::<u16>().is_ok() && !h.contains(':') => {
            (user, h.to_string(), p.parse().ok())
        }
        _ => (user, rest.to_string(), None),
    }
}

/// Adds the chosen hosts to `file` and says what happened. Nothing already saved is changed, and
/// no host that is already saved is added again. A `ProxyJump` becomes a jump host: an alias from
/// the file is imported too if needed, and a plain host name becomes a session of its own.
pub fn import(
    file: &mut SessionsFile,
    all: &[ImportedHost],
    chosen: &[String],
    mut new_id: impl FnMut() -> String,
) -> ImportResult {
    let fallback = os_user();
    let by_alias: HashMap<&str, &ImportedHost> =
        all.iter().map(|h| (h.alias.as_str(), h)).collect();
    let mut importer = Importer {
        by_alias,
        fallback,
        new_id: &mut new_id,
        result: ImportResult {
            added: vec![],
            skipped: vec![],
        },
        in_progress: Vec::new(),
    };

    for alias in chosen {
        let Some(host) = importer.by_alias.get(alias.as_str()).copied() else {
            importer
                .result
                .skipped
                .push(format!("{alias}: not in the file"));
            continue;
        };
        let user = host
            .user
            .clone()
            .unwrap_or_else(|| importer.fallback.clone());
        if file
            .sessions
            .iter()
            .any(|s| same_place(s, &host.hostname, host.port, &user))
        {
            importer
                .result
                .skipped
                .push(format!("{alias}: already saved"));
            continue;
        }
        if let Err(reason) = importer.ensure(host, file) {
            importer.result.skipped.push(reason);
        }
    }
    importer.result
}

/// What importing one host needs to remember while it follows jump hosts.
struct Importer<'a> {
    by_alias: HashMap<&'a str, &'a ImportedHost>,
    fallback: String,
    new_id: &'a mut dyn FnMut() -> String,
    result: ImportResult,
    /// Hosts being imported right now, to notice jump hosts that go round in a circle.
    in_progress: Vec<String>,
}

impl Importer<'_> {
    /// Finds or creates the session for a host, returning its ID.
    fn ensure(&mut self, host: &ImportedHost, file: &mut SessionsFile) -> Result<String, String> {
        let user = host.user.clone().unwrap_or_else(|| self.fallback.clone());
        if let Some(existing) = file
            .sessions
            .iter()
            .find(|s| same_place(s, &host.hostname, host.port, &user))
        {
            return Ok(existing.id().to_string());
        }
        if self.in_progress.contains(&host.alias) {
            return Err(format!(
                "{}: its jump hosts go round in a circle",
                host.alias
            ));
        }
        self.in_progress.push(host.alias.clone());
        let outcome = self.create(host, user, file);
        self.in_progress.pop();
        outcome
    }

    fn create(
        &mut self,
        host: &ImportedHost,
        user: String,
        file: &mut SessionsFile,
    ) -> Result<String, String> {
        // Jump hosts first, so this session can point at the last one.
        let mut jump = String::new();
        if let Some(raw) = &host.proxy_jump {
            let hops: Vec<&str> = raw
                .split(',')
                .map(str::trim)
                .filter(|h| !h.is_empty())
                .collect();
            if hops.len() > super::chain::MAX_JUMPS {
                return Err(format!(
                    "{}: more than {} jump hosts",
                    host.alias,
                    super::chain::MAX_JUMPS
                ));
            }
            let mut previous = String::new();
            for hop in hops {
                let (hop_user, hop_host, hop_port) = parse_hop(hop);
                let mut entry = match self.by_alias.get(hop_host.as_str()) {
                    Some(h) => (*h).clone(),
                    None => ImportedHost {
                        alias: hop_host.clone(),
                        hostname: hop_host.clone(),
                        port: 22,
                        user: None,
                        identity_file: None,
                        proxy_jump: None,
                    },
                };
                if hop_user.is_some() {
                    entry.user = hop_user;
                }
                if let Some(p) = hop_port {
                    entry.port = p;
                }
                // Each hop after the first goes through the one before it, not through
                // whatever the file says for that host.
                if !previous.is_empty() {
                    entry.proxy_jump = None;
                }
                let id = self.ensure(&entry, file)?;
                if !previous.is_empty()
                    && let Some(SavedSession::Ssh { jump, .. }) =
                        file.sessions.iter_mut().find(|s| s.id() == id)
                    && jump.is_empty()
                {
                    *jump = previous.clone();
                }
                previous = id;
            }
            jump = previous;
        }

        let id = (self.new_id)();
        let (auth, key_path) = match &host.identity_file {
            Some(path) => (SshAuth::Key, path.clone()),
            None => (SshAuth::Agent, String::new()),
        };
        file.sessions.push(SavedSession::Ssh {
            id: id.clone(),
            name: host.alias.clone(),
            folder: String::new(),
            host: host.hostname.clone(),
            port: host.port,
            user,
            auth,
            key_path,
            jump,
            x11: false,
        });
        self.result.added.push(host.alias.clone());
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
# my machines
Host build
    HostName build.example.com
    User alex
    Port 2222
    IdentityFile ~/.ssh/id_build

Host web db
    HostName=10.0.0.5
    User = deploy
    ProxyJump bastion

Host bastion
    HostName bastion.example.com
    User jump

Host *.internal !secret old-*
    User ignored

Host *
    User everyone
    ForwardAgent yes

Host quoted
    HostName "quoted.example.com"
    IdentityFile "~/.ssh/with space"

Host first
    HostName one.example
    HostName two.example
    Port notaport

Host templated
    IdentityFile ~/.ssh/%d/key
    ProxyJump none

Match host weird
    HostName should-not-count

Host after-match
    HostName after.example
"#;

    fn by_alias<'a>(hosts: &'a [ImportedHost], alias: &str) -> &'a ImportedHost {
        hosts
            .iter()
            .find(|h| h.alias == alias)
            .unwrap_or_else(|| panic!("no {alias}"))
    }

    #[test]
    fn reads_the_six_settings_and_skips_patterns() {
        let (hosts, includes) = parse_config(SAMPLE);
        assert!(includes.is_empty());
        let aliases: Vec<&str> = hosts.iter().map(|h| h.alias.as_str()).collect();
        assert_eq!(
            aliases,
            [
                "build",
                "web",
                "db",
                "bastion",
                "quoted",
                "first",
                "templated",
                "after-match"
            ]
        );
        let build = by_alias(&hosts, "build");
        assert_eq!(build.hostname, "build.example.com");
        assert_eq!(build.port, 2222);
        assert_eq!(build.user.as_deref(), Some("alex"));
        assert_eq!(build.identity_file.as_deref(), Some("~/.ssh/id_build"));
    }

    #[test]
    fn accepts_equals_signs_and_one_block_for_several_names() {
        let (hosts, _) = parse_config(SAMPLE);
        for alias in ["web", "db"] {
            let h = by_alias(&hosts, alias);
            assert_eq!(h.hostname, "10.0.0.5");
            assert_eq!(h.user.as_deref(), Some("deploy"));
            assert_eq!(h.proxy_jump.as_deref(), Some("bastion"));
        }
    }

    #[test]
    fn the_first_value_wins_and_odd_values_fall_back() {
        let (hosts, _) = parse_config(SAMPLE);
        let first = by_alias(&hosts, "first");
        assert_eq!(first.hostname, "one.example");
        assert_eq!(first.port, 22);
    }

    #[test]
    fn quotes_are_removed_and_unknowable_values_are_dropped() {
        let (hosts, _) = parse_config(SAMPLE);
        let q = by_alias(&hosts, "quoted");
        assert_eq!(q.hostname, "quoted.example.com");
        assert_eq!(q.identity_file.as_deref(), Some("~/.ssh/with space"));
        let t = by_alias(&hosts, "templated");
        assert_eq!(t.identity_file, None, "%d can't be filled in");
        assert_eq!(t.proxy_jump, None, "ProxyJump none means no jump");
    }

    #[test]
    fn match_blocks_are_ignored() {
        let (hosts, _) = parse_config(SAMPLE);
        assert!(hosts.iter().all(|h| h.hostname != "should-not-count"));
        assert_eq!(by_alias(&hosts, "after-match").hostname, "after.example");
    }

    #[test]
    fn keywords_are_case_insensitive_and_a_missing_hostname_is_the_alias() {
        let (hosts, _) = parse_config("HOST Mixed\n  hostNAME m.example\nhost bare\n");
        assert_eq!(hosts[0].hostname, "m.example");
        assert_eq!(hosts[1].hostname, "bare");
    }

    #[test]
    fn include_is_followed_one_level_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("conf.d")).unwrap();
        std::fs::write(
            dir.path().join("config"),
            "Include conf.d/*.conf\nHost top\n  HostName top.example\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("conf.d").join("a.conf"),
            "Include deeper\nHost a\n  HostName a.example\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("conf.d").join("b.conf"),
            "Host b\n  HostName b.example\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("conf.d").join("notes.txt"), "Host no\n").unwrap();
        std::fs::write(
            dir.path().join("deeper"),
            "Host too-deep\n  HostName deep.example\n",
        )
        .unwrap();

        let hosts = load_config(&dir.path().join("config"), dir.path());
        let mut aliases: Vec<&str> = hosts.iter().map(|h| h.alias.as_str()).collect();
        aliases.sort();
        assert_eq!(aliases, ["a", "b", "top"]);
    }

    #[test]
    fn a_missing_file_gives_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_config(&dir.path().join("config"), dir.path()).is_empty());
    }

    fn ids() -> impl FnMut() -> String {
        let mut n = 0;
        move || {
            n += 1;
            format!("id{n}")
        }
    }

    fn empty() -> SessionsFile {
        SessionsFile::default()
    }

    fn ssh_of<'a>(file: &'a SessionsFile, name: &str) -> &'a SavedSession {
        file.sessions
            .iter()
            .find(|s| s.name() == name)
            .unwrap_or_else(|| panic!("no {name}"))
    }

    #[test]
    fn imports_with_the_right_login_method() {
        let (hosts, _) = parse_config(SAMPLE);
        let mut file = empty();
        let result = import(&mut file, &hosts, &["build".into(), "quoted".into()], ids());
        assert_eq!(result.added, ["build", "quoted"]);
        match ssh_of(&file, "build") {
            SavedSession::Ssh {
                host,
                port,
                user,
                auth,
                key_path,
                jump,
                ..
            } => {
                assert_eq!(
                    (host.as_str(), *port, user.as_str()),
                    ("build.example.com", 2222, "alex")
                );
                assert_eq!(*auth, SshAuth::Key);
                assert_eq!(key_path, "~/.ssh/id_build");
                assert!(jump.is_empty());
            }
            _ => panic!(),
        }
        file.validate().unwrap();
    }

    #[test]
    fn no_key_means_the_agent() {
        let (hosts, _) = parse_config("Host plain\n  HostName p.example\n  User u\n");
        let mut file = empty();
        import(&mut file, &hosts, &["plain".into()], ids());
        assert!(matches!(
            ssh_of(&file, "plain"),
            SavedSession::Ssh {
                auth: SshAuth::Agent,
                ..
            }
        ));
    }

    #[test]
    fn a_jump_alias_in_the_file_is_imported_and_linked() {
        let (hosts, _) = parse_config(SAMPLE);
        let mut file = empty();
        let result = import(&mut file, &hosts, &["web".into()], ids());
        assert_eq!(result.added, ["bastion", "web"]);
        let bastion_id = ssh_of(&file, "bastion").id().to_string();
        match ssh_of(&file, "web") {
            SavedSession::Ssh { jump, .. } => assert_eq!(*jump, bastion_id),
            _ => panic!(),
        }
        file.validate().unwrap();
    }

    #[test]
    fn a_plain_host_name_in_proxyjump_becomes_its_own_session_and_a_chain_is_linked() {
        let (hosts, _) = parse_config(
            "Host target\n  HostName t.example\n  User t\n  ProxyJump me@first.example:2200,second.example\n",
        );
        let mut file = empty();
        let result = import(&mut file, &hosts, &["target".into()], ids());
        assert_eq!(result.added, ["first.example", "second.example", "target"]);
        let first = ssh_of(&file, "first.example");
        let second = ssh_of(&file, "second.example");
        match (first, second, ssh_of(&file, "target")) {
            (
                SavedSession::Ssh {
                    id: first_id,
                    port,
                    user,
                    jump: j1,
                    ..
                },
                SavedSession::Ssh {
                    id: second_id,
                    jump: j2,
                    ..
                },
                SavedSession::Ssh { jump: j3, .. },
            ) => {
                assert_eq!((*port, user.as_str()), (2200, "me"));
                assert!(j1.is_empty());
                assert_eq!(j2, first_id, "second goes through first");
                assert_eq!(j3, second_id, "the target goes through second");
            }
            _ => panic!(),
        }
        file.validate().unwrap();
    }

    #[test]
    fn already_saved_hosts_are_skipped_and_used_as_the_jump_host() {
        let (hosts, _) = parse_config(SAMPLE);
        let mut file = empty();
        import(&mut file, &hosts, &["bastion".into()], ids());
        let before = file.sessions.len();
        let again = import(&mut file, &hosts, &["bastion".into()], ids());
        assert_eq!(again.added.len(), 0);
        assert_eq!(again.skipped, ["bastion: already saved"]);
        assert_eq!(file.sessions.len(), before);

        // A host that jumps through it reuses the saved one instead of adding a copy.
        import(&mut file, &hosts, &["web".into()], ids());
        assert_eq!(
            file.sessions
                .iter()
                .filter(|s| s.name() == "bastion")
                .count(),
            1
        );
    }

    #[test]
    fn too_many_jump_hosts_and_circles_are_left_out_with_a_reason() {
        let (hosts, _) = parse_config(
            "Host far\n  HostName f.example\n  User u\n  ProxyJump a,b,c,d\n\
             Host x\n  HostName x.example\n  User u\n  ProxyJump y\n\
             Host y\n  HostName y.example\n  User u\n  ProxyJump x\n",
        );
        let mut file = empty();
        let result = import(
            &mut file,
            &hosts,
            &["far".into(), "x".into(), "ghost".into()],
            ids(),
        );
        assert!(
            result
                .skipped
                .iter()
                .any(|s| s.starts_with("far: more than 3"))
        );
        assert!(
            result.skipped.iter().any(|s| s.contains("circle")),
            "{:?}",
            result.skipped
        );
        assert!(result.skipped.iter().any(|s| s == "ghost: not in the file"));
        file.validate().unwrap();
    }

    #[test]
    fn the_preview_marks_what_is_already_saved() {
        let (hosts, _) = parse_config(
            "Host a\n  HostName a.example\n  User u\nHost b\n  HostName b.example\n  User u\n",
        );
        let mut file = empty();
        import(&mut file, &hosts, &["a".into()], ids());
        let items = preview(&file, &hosts);
        assert!(items[0].duplicate);
        assert!(!items[1].duplicate);
        assert_eq!(items[1].user, "u");
    }
}
