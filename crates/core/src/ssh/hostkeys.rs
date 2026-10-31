//! Checking a server's host key against `known_hosts` (INTERFACES.md section 7, SEC1).

use std::path::Path;

use russh::keys::known_hosts::{
    check_known_hosts_path, known_host_keys_path, learn_known_hosts_path,
};
use russh::keys::{Error as KeyError, HashAlg, PublicKey};

use crate::error::{AppError, ErrorCode};

/// What to do about the key a server offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyDecision {
    /// The key is recorded and matches: connect.
    Trusted,
    /// First time this host is seen: ask the user.
    Unknown {
        algorithm: String,
        fingerprint: String,
    },
    /// The host is known, but by keys of other types: ask, with a warning.
    NewType {
        algorithm: String,
        fingerprint: String,
        known_algorithms: Vec<String>,
    },
}

pub fn fingerprint_of(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

fn unreadable(path: &Path) -> AppError {
    AppError::new(
        ErrorCode::ConnectionFailed,
        format!(
            "The file of known hosts ({}) can't be read, so the server can't be checked.",
            path.display()
        ),
    )
}

/// Decides what to do about `key`. A changed key is an error that can't be continued past.
pub fn decide(
    host: &str,
    port: u16,
    key: &PublicKey,
    known_hosts: &Path,
) -> Result<HostKeyDecision, AppError> {
    // The library treats a file it can't open as having no keys, which would show a changed key
    // as a harmless first connection. So read the file here first: a missing file is empty, and
    // any other failure stops the connection.
    let text = match std::fs::read(known_hosts) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return Err(unreadable(known_hosts)),
    };

    // Host names are not case sensitive, and the library compares them as written.
    let host = &host.to_ascii_lowercase();
    // Lines the library cannot read but OpenSSH honours would make a pinned host look new, so
    // the host cannot be checked at all rather than being trusted as a first connection.
    if has_unreadable_entry(&text, host, port) {
        return Err(AppError::new(
            ErrorCode::ConnectionFailed,
            format!(
                "{} has an entry for {host} that this app can't check (it uses tabs, wildcards or \
                 a marker such as @cert-authority). Replace it with a plain line, \
                 `host ssh-ed25519 key`, to connect.",
                known_hosts.display()
            ),
        ));
    }

    let fingerprint = fingerprint_of(key);
    match check_known_hosts_path(host, port, key, known_hosts) {
        Ok(true) => Ok(HostKeyDecision::Trusted),
        Err(KeyError::KeyChanged { line }) => {
            // The library's count skips comment lines, so it can point at the wrong line. Find
            // the real one, and also give the command that removes exactly this host.
            let line = physical_line(&text, host, port).unwrap_or(line);
            let name = host_form(host, port);
            Err(AppError::new(
                ErrorCode::HostKeyChanged,
                format!(
                    "The key of {host} has changed. It now offers {fingerprint}, but {} (line {line}) \
                     records a different one. This could be an attack, or the server may have been \
                     reinstalled. If you are sure it is safe, remove that entry (for example with \
                     ssh-keygen -R \"{name}\") and connect again.",
                    known_hosts.display()
                ),
            ))
        }
        Ok(false) => {
            // The library also says "no" when the host is known only by keys of another type.
            let known = known_host_keys_path(host, port, known_hosts)
                .map_err(|_| unreadable(known_hosts))?;
            let algorithm = key.algorithm().to_string();
            if known.is_empty() {
                Ok(HostKeyDecision::Unknown {
                    algorithm,
                    fingerprint,
                })
            } else {
                let mut known_algorithms: Vec<String> = known
                    .iter()
                    .map(|(_, k)| k.algorithm().to_string())
                    .collect();
                known_algorithms.sort();
                known_algorithms.dedup();
                Ok(HostKeyDecision::NewType {
                    algorithm,
                    fingerprint,
                    known_algorithms,
                })
            }
        }
        Err(_) => Err(unreadable(known_hosts)),
    }
}

/// The name a `known_hosts` line uses for this host, in the form OpenSSH writes it.
fn host_form(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

/// A small `*` and `?` matcher, for host patterns.
fn glob(pattern: &str, text: &str) -> bool {
    fn go(p: &[char], t: &[char]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some('*') => (0..=t.len()).any(|i| go(&p[1..], &t[i..])),
            Some('?') => !t.is_empty() && go(&p[1..], &t[1..]),
            Some(c) => t.first() == Some(c) && go(&p[1..], &t[1..]),
        }
    }
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    go(&p, &t)
}

/// The 1-based number of the first real line that names this host. Every physical line is
/// counted, comments and blank lines included.
fn physical_line(text: &str, host: &str, port: u16) -> Option<usize> {
    let wanted = host_form(host, port);
    text.lines().enumerate().find_map(|(i, line)| {
        let first = line.split_whitespace().next()?;
        if first.starts_with('#') || first.starts_with('@') {
            return None;
        }
        first
            .split(',')
            .any(|name| name.eq_ignore_ascii_case(&wanted))
            .then_some(i + 1)
    })
}

/// True if some line applies to this host but is written in a way the library would skip: split
/// by a tab or several spaces, using a wildcard or negation, or starting with a marker.
fn has_unreadable_entry(text: &str, host: &str, port: u16) -> bool {
    let wanted = host_form(host, port);
    text.lines().any(|line| {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return false;
        }
        let odd_spacing = line.contains('\t') || line.contains("  ") || line.starts_with(' ');
        let mut tokens = trimmed.split_whitespace();
        let first = tokens.next().unwrap_or("");
        let (marker, patterns) = if first.starts_with('@') {
            (true, tokens.next().unwrap_or(""))
        } else {
            (false, first)
        };
        patterns.split(',').any(|name| {
            let negated = name.starts_with('!');
            let name = name.trim_start_matches('!');
            let wild = name.contains(['*', '?']);
            let applies = if wild {
                glob(&name.to_ascii_lowercase(), &wanted) || glob(&name.to_ascii_lowercase(), host)
            } else {
                name.eq_ignore_ascii_case(&wanted)
            };
            applies && (marker || odd_spacing || wild || negated)
        })
    })
}

/// Records a key the user accepted.
pub fn learn(host: &str, port: u16, key: &PublicKey, known_hosts: &Path) -> Result<(), AppError> {
    if let Some(dir) = known_hosts.parent() {
        std::fs::create_dir_all(dir).map_err(|_| unreadable(known_hosts))?;
    }
    learn_known_hosts_path(host, port, key, known_hosts).map_err(|_| {
        AppError::new(
            ErrorCode::Internal,
            "Couldn't add the server to the file of known hosts.",
        )
    })
}

/// The file OpenSSH uses: `~/.ssh/known_hosts`, or `%USERPROFILE%\.ssh\known_hosts` on Windows.
pub fn default_known_hosts() -> Option<std::path::PathBuf> {
    directories::UserDirs::new().map(|d| d.home_dir().join(".ssh").join("known_hosts"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::parse_public_key_base64;

    const ED_A: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIKgqmK9ropguFO7dbtvnHkHDGg1e+OzTvNO7hWlcj5yf";
    const ED_B: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIFD7C7zMBtOyZdIBLxajc17aZb7tI2Wqz3jw0uDu+gvo";
    const ECDSA: &str = "AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBGnaa8PYp9RS/n5ZoYO9Cwo9OFOdo3kAXbeuOc2PmnB4lv/DWGX61XoFcZTyvRGi39kPx133ETu3IsZLyMxLfhw=";

    fn key(b64: &str) -> PublicKey {
        parse_public_key_base64(b64).unwrap()
    }

    fn setup() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        (dir, path)
    }

    #[test]
    fn a_matching_key_is_trusted() {
        let (_d, path) = setup();
        learn("example.test", 22, &key(ED_A), &path).unwrap();
        assert_eq!(
            decide("example.test", 22, &key(ED_A), &path).unwrap(),
            HostKeyDecision::Trusted
        );
    }

    #[test]
    fn a_host_in_a_missing_file_is_unknown() {
        let (_d, path) = setup();
        match decide("example.test", 22, &key(ED_A), &path).unwrap() {
            HostKeyDecision::Unknown {
                algorithm,
                fingerprint,
            } => {
                assert_eq!(algorithm, "ssh-ed25519");
                assert!(fingerprint.starts_with("SHA256:"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_host_missing_from_a_file_with_other_hosts_is_unknown() {
        let (_d, path) = setup();
        learn("known.test", 22, &key(ED_A), &path).unwrap();
        assert!(matches!(
            decide("other.test", 22, &key(ED_A), &path).unwrap(),
            HostKeyDecision::Unknown { .. }
        ));
    }

    #[test]
    fn a_changed_key_of_the_same_type_blocks_and_says_where() {
        let (_d, path) = setup();
        learn("example.test", 22, &key(ED_A), &path).unwrap();
        let err = decide("example.test", 22, &key(ED_B), &path).unwrap_err();
        assert_eq!(err.code, ErrorCode::HostKeyChanged);
        assert!(err.message.contains("example.test"), "{}", err.message);
        assert!(err.message.contains("SHA256:"), "{}", err.message);
        assert!(err.message.contains("line"), "{}", err.message);
        assert!(
            err.message.contains(&path.display().to_string()),
            "{}",
            err.message
        );
    }

    #[test]
    fn a_key_of_a_new_type_is_a_warning_not_a_first_connection() {
        // The library says "no" rather than "changed" here, so this must not look unknown.
        let (_d, path) = setup();
        learn("example.test", 22, &key(ED_A), &path).unwrap();
        match decide("example.test", 22, &key(ECDSA), &path).unwrap() {
            HostKeyDecision::NewType {
                algorithm,
                known_algorithms,
                ..
            } => {
                assert!(algorithm.starts_with("ecdsa"), "{algorithm}");
                assert_eq!(known_algorithms, vec!["ssh-ed25519".to_string()]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn another_port_is_a_separate_host() {
        let (_d, path) = setup();
        learn("example.test", 22, &key(ED_A), &path).unwrap();
        assert!(matches!(
            decide("example.test", 2222, &key(ED_B), &path).unwrap(),
            HostKeyDecision::Unknown { .. }
        ));
        learn("example.test", 2222, &key(ED_B), &path).unwrap();
        assert_eq!(
            decide("example.test", 2222, &key(ED_B), &path).unwrap(),
            HostKeyDecision::Trusted
        );
        assert_eq!(
            decide("example.test", 22, &key(ED_A), &path).unwrap(),
            HostKeyDecision::Trusted
        );
    }

    #[test]
    fn a_corrupt_file_fails_the_connection_and_is_never_accepted() {
        let (_d, path) = setup();
        std::fs::write(&path, "example.test ssh-ed25519 !!!notbase64!!!\n").unwrap();
        let err = decide("example.test", 22, &key(ED_A), &path).unwrap_err();
        assert_ne!(err.code, ErrorCode::HostKeyChanged);
    }

    #[test]
    fn a_file_that_cannot_be_opened_fails_the_connection() {
        let (dir, _path) = setup();
        let unreadable_path = dir.path().join("unreadable");
        #[cfg(windows)]
        std::fs::create_dir(&unreadable_path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(&unreadable_path, "example.test ssh-ed25519 AAAA\n").unwrap();
            std::fs::set_permissions(&unreadable_path, std::fs::Permissions::from_mode(0o000))
                .unwrap();
        }
        // Running as root can open anything, so only check when the file really can't be read.
        if std::fs::read(&unreadable_path).is_err() {
            let err = decide("example.test", 22, &key(ED_A), &unreadable_path).unwrap_err();
            assert_eq!(err.code, ErrorCode::ConnectionFailed);
            assert!(err.message.contains("can't be read"), "{}", err.message);
        }
    }

    #[test]
    fn the_line_number_counts_comments_and_blank_lines() {
        let (_d, path) = setup();
        std::fs::write(&path, "# first comment\n# second comment\n\n").unwrap();
        learn("a.example", 22, &key(ED_A), &path).unwrap();
        learn("b.example", 22, &key(ED_A), &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let expected = text
            .lines()
            .position(|l| l.starts_with("b.example"))
            .unwrap()
            + 1;
        let err = decide("b.example", 22, &key(ED_B), &path).unwrap_err();
        assert_eq!(err.code, ErrorCode::HostKeyChanged);
        assert!(
            err.message.contains(&format!("(line {expected})")),
            "expected line {expected}: {}",
            err.message
        );
        assert!(
            err.message.contains("ssh-keygen -R \"b.example\""),
            "{}",
            err.message
        );
    }

    #[test]
    fn a_host_on_another_port_is_named_the_way_ssh_keygen_expects() {
        let (_d, path) = setup();
        learn("h.example", 2222, &key(ED_A), &path).unwrap();
        let err = decide("h.example", 2222, &key(ED_B), &path).unwrap_err();
        assert!(err.message.contains("[h.example]:2222"), "{}", err.message);
    }

    #[test]
    fn a_name_typed_in_capitals_still_matches_its_entry() {
        let (_d, path) = setup();
        learn("server.example", 22, &key(ED_A), &path).unwrap();
        assert_eq!(
            decide("Server.Example", 22, &key(ED_A), &path).unwrap(),
            HostKeyDecision::Trusted
        );
        // And a different key is a change, not a first connection.
        assert_eq!(
            decide("SERVER.example", 22, &key(ED_B), &path)
                .unwrap_err()
                .code,
            ErrorCode::HostKeyChanged
        );
    }

    #[test]
    fn entries_the_library_cannot_read_stop_the_check_instead_of_looking_new() {
        for line in [
            "server.example\tssh-ed25519 AAAA\n",
            "server.example  ssh-ed25519 AAAA\n",
            "*.example ssh-ed25519 AAAA\n",
            "@cert-authority server.example ssh-ed25519 AAAA\n",
            "@revoked server.example ssh-ed25519 AAAA\n",
            "server.* ssh-ed25519 AAAA\n",
        ] {
            let (_d, path) = setup();
            std::fs::write(&path, line).unwrap();
            let err = decide("server.example", 22, &key(ED_A), &path).unwrap_err();
            assert_eq!(err.code, ErrorCode::ConnectionFailed, "{line:?}");
            assert!(
                err.message.contains("can't check"),
                "{line:?}: {}",
                err.message
            );
        }
    }

    #[test]
    fn unrelated_odd_entries_do_not_get_in_the_way() {
        let (_d, path) = setup();
        std::fs::write(
            &path,
            "other.example\tssh-ed25519 AAAA\n*.corp ssh-ed25519 AAAA\n",
        )
        .unwrap();
        assert!(matches!(
            decide("server.example", 22, &key(ED_A), &path).unwrap(),
            HostKeyDecision::Unknown { .. }
        ));
    }

    #[test]
    fn learning_creates_the_folder_and_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".ssh").join("known_hosts");
        learn("example.test", 22, &key(ED_A), &path).unwrap();
        assert_eq!(
            decide("example.test", 22, &key(ED_A), &path).unwrap(),
            HostKeyDecision::Trusted
        );
    }
}
