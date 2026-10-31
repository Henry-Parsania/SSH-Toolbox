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
    match std::fs::read(known_hosts) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(unreadable(known_hosts)),
    }

    let fingerprint = fingerprint_of(key);
    match check_known_hosts_path(host, port, key, known_hosts) {
        Ok(true) => Ok(HostKeyDecision::Trusted),
        Err(KeyError::KeyChanged { line }) => Err(AppError::new(
            ErrorCode::HostKeyChanged,
            format!(
                "The key of {host} has changed. It now offers {fingerprint}, but {} (line {line}) \
                 records a different one. This could be an attack, or the server may have been \
                 reinstalled. If you are sure it is safe, delete line {line} from that file and \
                 connect again.",
                known_hosts.display()
            ),
        )),
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
