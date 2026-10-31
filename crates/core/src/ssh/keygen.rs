//! Making a new key (INTERFACES.md section 15, step 1), and finding existing ones.
//!
//! An existing file is never overwritten: files are created in "create new" mode, which fails
//! instead of replacing, and a taken name moves on to the next one.

use std::io::Write;
use std::path::{Path, PathBuf};

use russh::keys::{Algorithm, PrivateKey};
use serde::Serialize;

use crate::error::{AppError, ErrorCode};
use crate::secret::Secret;

const BASE_NAME: &str = "id_ed25519_sshtoolbox";
const COMMENT: &str = "sshtoolbox";
const MAX_TRIES: u32 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewKey {
    pub private_path: PathBuf,
    pub public_path: PathBuf,
    /// The line to put in `authorized_keys`: `ssh-ed25519 <base64> sshtoolbox`.
    pub public_line: String,
    pub fingerprint: String,
}

fn failed(message: &str) -> AppError {
    AppError::new(ErrorCode::Internal, message)
}

fn candidate(dir: &Path, attempt: u32) -> (PathBuf, PathBuf) {
    let name = if attempt == 0 {
        BASE_NAME.to_string()
    } else {
        format!("{BASE_NAME}_{}", attempt + 1)
    };
    (dir.join(&name), dir.join(format!("{name}.pub")))
}

/// Creates a file that must not exist yet. On Unix it is private from the start.
fn create_new(path: &Path, private: bool) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if private { 0o600 } else { 0o644 });
    }
    #[cfg(not(unix))]
    let _ = private;
    options.open(path)
}

/// Windows: the private key may be read only by its owner, SYSTEM and administrators, the same
/// pattern as for `authorized_keys` (INTERFACES.md section 15).
#[cfg(windows)]
fn restrict_to_owner(path: &Path) -> Result<(), AppError> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let denied = || failed("Couldn't restrict the private key file to your account.");

    let who = std::process::Command::new("whoami")
        .args(["/user", "/fo", "csv", "/nh"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|_| denied())?;
    // The output is `"DOMAIN\user","S-1-5-21-..."`.
    let text = String::from_utf8_lossy(&who.stdout);
    let sid = text
        .trim()
        .rsplit(',')
        .next()
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| s.starts_with("S-1-"))
        .ok_or_else(denied)?;

    let status = std::process::Command::new("icacls.exe")
        .arg(path)
        .args([
            "/inheritance:r",
            "/grant",
            &format!("*{sid}:F"),
            "/grant",
            "*S-1-5-18:F",
            "/grant",
            "*S-1-5-32-544:F",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|_| denied())?;
    if status.status.success() {
        Ok(())
    } else {
        Err(denied())
    }
}

#[cfg(not(windows))]
fn restrict_to_owner(_path: &Path) -> Result<(), AppError> {
    Ok(()) // Already mode 600 from the moment it was created.
}

/// Makes an ed25519 key in `dir`, protected by `passphrase` if one is given.
pub fn generate_key(dir: &Path, passphrase: Option<&Secret>) -> Result<NewKey, AppError> {
    std::fs::create_dir_all(dir).map_err(|_| failed("Couldn't create the .ssh folder."))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Only tighten a folder this call may have just made; a user's own folder is left alone
        // unless it is open to others.
        if let Ok(meta) = std::fs::metadata(dir)
            && meta.permissions().mode() & 0o077 != 0
            && dir.ends_with(".ssh")
        {
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
    }

    let mut rng = rand::rng();
    let mut key = PrivateKey::random(&mut rng, Algorithm::Ed25519)
        .map_err(|_| failed("Couldn't make a key."))?;
    key.set_comment(COMMENT);
    let public = key.public_key().clone();
    let public_line = public
        .to_openssh()
        .map_err(|_| failed("Couldn't write the public key."))?;
    let fingerprint = public.fingerprint(russh::keys::HashAlg::Sha256).to_string();

    let to_write = match passphrase {
        Some(pass) if !pass.expose().is_empty() => key
            .encrypt(&mut rng, pass.expose())
            .map_err(|_| failed("Couldn't protect the key with that passphrase."))?,
        _ => key,
    };
    let private_text = to_write
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .map_err(|_| failed("Couldn't write the private key."))?;

    for attempt in 0..MAX_TRIES {
        let (private_path, public_path) = candidate(dir, attempt);
        // A lone `.pub` from an earlier key blocks the name too.
        if public_path.exists() {
            continue;
        }
        let mut private_file = match create_new(&private_path, true) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(failed("Couldn't create the private key file.")),
        };

        let written = (|| {
            private_file.write_all(private_text.as_bytes())?;
            private_file.sync_all()
        })();
        drop(private_file);
        if written.is_err() {
            let _ = std::fs::remove_file(&private_path);
            return Err(failed("Couldn't write the private key file."));
        }
        // Never leave a private key that others could read.
        if let Err(error) = restrict_to_owner(&private_path) {
            let _ = std::fs::remove_file(&private_path);
            return Err(error);
        }

        let public_written = create_new(&public_path, false)
            .and_then(|mut f| f.write_all(format!("{public_line}\n").as_bytes()));
        if public_written.is_err() {
            let _ = std::fs::remove_file(&private_path);
            let _ = std::fs::remove_file(&public_path);
            return Err(failed("Couldn't write the public key file."));
        }
        return Ok(NewKey {
            private_path,
            public_path,
            public_line,
            fingerprint,
        });
    }
    Err(failed("Too many keys with that name already exist."))
}

/// An existing key the user can pick: a private key file with its `.pub` beside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExistingKey {
    pub private_path: PathBuf,
    pub public_line: String,
}

pub fn list_keys(dir: &Path) -> Vec<ExistingKey> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut keys: Vec<ExistingKey> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "pub") {
                return None;
            }
            let mut public = path.clone().into_os_string();
            public.push(".pub");
            let line = std::fs::read_to_string(PathBuf::from(public)).ok()?;
            let line = line.lines().next()?.trim().to_string();
            line.starts_with("ssh-").then_some(ExistingKey {
                private_path: path,
                public_line: line,
            })
        })
        .collect();
    keys.sort_by(|a, b| a.private_path.cmp(&b.private_path));
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::load_secret_key;

    #[test]
    fn makes_a_key_that_loads_and_a_matching_public_line() {
        let dir = tempfile::tempdir().unwrap();
        let key = generate_key(dir.path(), None).unwrap();
        assert_eq!(key.private_path, dir.path().join("id_ed25519_sshtoolbox"));
        assert_eq!(
            key.public_path,
            dir.path().join("id_ed25519_sshtoolbox.pub")
        );

        let loaded = load_secret_key(&key.private_path, None).unwrap();
        assert_eq!(loaded.algorithm(), Algorithm::Ed25519);

        let shape = regex_like(&key.public_line);
        assert!(shape, "{}", key.public_line);
        assert!(key.public_line.ends_with(" sshtoolbox"));
        assert_eq!(
            std::fs::read_to_string(&key.public_path).unwrap(),
            format!("{}\n", key.public_line)
        );
        assert!(key.fingerprint.starts_with("SHA256:"));
        // The public line is the key that was written.
        let written = loaded.public_key().to_openssh().unwrap();
        let blob = written.split(' ').nth(1).unwrap();
        assert!(key.public_line.contains(blob));
    }

    /// `^ssh-ed25519 [A-Za-z0-9+/=]+ sshtoolbox$`, without pulling in a regex library.
    fn regex_like(line: &str) -> bool {
        let parts: Vec<&str> = line.split(' ').collect();
        parts.len() == 3
            && parts[0] == "ssh-ed25519"
            && !parts[1].is_empty()
            && parts[1]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+/=".contains(c))
    }

    #[test]
    fn a_taken_name_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let taken = dir.path().join("id_ed25519_sshtoolbox");
        std::fs::write(&taken, "someone's real key").unwrap();

        let key = generate_key(dir.path(), None).unwrap();
        assert_eq!(key.private_path, dir.path().join("id_ed25519_sshtoolbox_2"));
        assert_eq!(
            std::fs::read_to_string(&taken).unwrap(),
            "someone's real key"
        );

        let third = generate_key(dir.path(), None).unwrap();
        assert_eq!(
            third.private_path,
            dir.path().join("id_ed25519_sshtoolbox_3")
        );
        assert_ne!(third.public_line, key.public_line);
    }

    #[test]
    fn a_lone_public_file_also_blocks_a_name() {
        let dir = tempfile::tempdir().unwrap();
        let orphan = dir.path().join("id_ed25519_sshtoolbox.pub");
        std::fs::write(&orphan, "keep me").unwrap();
        let key = generate_key(dir.path(), None).unwrap();
        assert_eq!(key.private_path, dir.path().join("id_ed25519_sshtoolbox_2"));
        assert_eq!(std::fs::read_to_string(&orphan).unwrap(), "keep me");
    }

    #[test]
    fn a_passphrase_protects_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let key = generate_key(dir.path(), Some(&Secret::new("correct horse"))).unwrap();
        assert!(matches!(
            load_secret_key(&key.private_path, None),
            Err(russh::keys::Error::KeyIsEncrypted)
        ));
        assert!(load_secret_key(&key.private_path, Some("wrong")).is_err());
        assert!(load_secret_key(&key.private_path, Some("correct horse")).is_ok());
        // The passphrase isn't anywhere in the files.
        let text = std::fs::read_to_string(&key.private_path).unwrap()
            + &std::fs::read_to_string(&key.public_path).unwrap();
        assert!(!text.contains("correct horse"));
    }

    #[test]
    fn an_empty_passphrase_means_none() {
        let dir = tempfile::tempdir().unwrap();
        let key = generate_key(dir.path(), Some(&Secret::new(""))).unwrap();
        assert!(load_secret_key(&key.private_path, None).is_ok());
    }

    #[test]
    fn creates_the_folder_if_it_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let ssh = dir.path().join(".ssh");
        let key = generate_key(&ssh, None).unwrap();
        assert!(key.private_path.starts_with(&ssh));
    }

    #[cfg(unix)]
    #[test]
    fn the_private_key_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let key = generate_key(dir.path(), None).unwrap();
        let mode = std::fs::metadata(&key.private_path)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(windows)]
    #[test]
    fn the_private_key_is_closed_to_other_accounts() {
        let dir = tempfile::tempdir().unwrap();
        let key = generate_key(dir.path(), None).unwrap();
        let out = std::process::Command::new("icacls.exe")
            .arg(&key.private_path)
            .output()
            .unwrap();
        let acl = String::from_utf8_lossy(&out.stdout).to_lowercase();
        for other in ["everyone", "builtin\\users", "authenticated users"] {
            assert!(!acl.contains(other), "{other} can reach the key:\n{acl}");
        }
        assert!(acl.contains("system"), "{acl}");
        // And the owner can still read it.
        assert!(load_secret_key(&key.private_path, None).is_ok());
    }

    #[test]
    fn lists_keys_that_have_a_public_file() {
        let dir = tempfile::tempdir().unwrap();
        let made = generate_key(dir.path(), None).unwrap();
        std::fs::write(dir.path().join("known_hosts"), "x").unwrap();
        std::fs::write(dir.path().join("lonely"), "x").unwrap();
        let keys = list_keys(dir.path());
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].private_path, made.private_path);
        assert_eq!(keys[0].public_line, made.public_line);
        assert!(list_keys(&dir.path().join("nope")).is_empty());
    }
}
