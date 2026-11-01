//! Where the app's files live, and how they are written (INTERFACES.md section 2 and 9).

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{AppError, ErrorCode};

/// The config folder, for example under `%APPDATA%` on Windows.
///
/// Setting `SSH_TOOLBOX_CONFIG_DIR` uses another folder instead. That is for trying the app
/// out, or taking screenshots, without touching real saved sessions and settings.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SSH_TOOLBOX_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    directories::ProjectDirs::from("dev", "sshtoolbox", "SSHToolbox")
        .map(|dirs| dirs.config_dir().to_path_buf())
}

fn write_failed() -> AppError {
    AppError::new(ErrorCode::Internal, "Couldn't save the file.")
}

/// Writes a file so that a crash half way never leaves a broken one: the text goes to a
/// temporary file beside it, which then replaces the real one.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), AppError> {
    let dir = path.parent().ok_or_else(write_failed)?;
    std::fs::create_dir_all(dir).map_err(|_| write_failed())?;
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);

    let result = (|| {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        // Keep the version being replaced as `<name>.bak`.
        if path.exists() {
            let mut backup = path.as_os_str().to_owned();
            backup.push(".bak");
            std::fs::copy(path, PathBuf::from(backup))?;
        }
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(|_| write_failed())
}

/// Moves a file that can't be read to `<name>.bad`, so it is kept for the user to look at and
/// is never overwritten by a fresh default.
pub fn set_aside(path: &Path) {
    let mut bad = path.as_os_str().to_owned();
    bad.push(".bad");
    let _ = std::fs::rename(path, PathBuf::from(bad));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_new_file_and_replaces_an_old_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("a.toml");
        write_atomic(&path, "one").unwrap();
        write_atomic(&path, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
        assert!(!dir.path().join("sub").join("a.toml.tmp").exists());
    }

    #[test]
    fn set_aside_keeps_the_original_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.toml");
        std::fs::write(&path, "broken").unwrap();
        set_aside(&path);
        assert!(!path.exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.toml.bad")).unwrap(),
            "broken"
        );
    }
}
