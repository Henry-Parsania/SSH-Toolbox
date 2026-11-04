//! Reading and saving a text file for the built-in editor (task 7.10).
//!
//! Only small UTF-8 text files are opened. The editor works with `\n` line ends; files that used
//! `\r\n` are noted and written back the same way. Saving checks that the file on the server is
//! still the one that was opened, so someone else's change is never overwritten without asking.

use serde::Serialize;
use tokio_util::sync::CancellationToken;

use super::backend::{FileBackend, FileKind};
use crate::error::{AppError, ErrorCode};

/// The largest file the editor opens.
pub const MAX_TEXT_FILE: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextFile {
    /// The text, with `\n` line ends.
    pub text: String,
    /// The file used `\r\n`, and is written back that way.
    pub crlf: bool,
    /// When the file was last changed (seconds since 1970), if the server says.
    pub modified: Option<i64>,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum SaveOutcome {
    Saved {
        modified: Option<i64>,
    },
    /// The file on the server changed after it was opened. Nothing was written.
    RemoteChanged,
}

fn refuse(message: &str) -> AppError {
    AppError::new(ErrorCode::InvalidInput, message)
}

pub async fn read_text(backend: &dyn FileBackend, path: &str) -> Result<TextFile, AppError> {
    let info = backend.stat(path).await?;
    if info.kind != FileKind::File {
        return Err(refuse("That isn't a file."));
    }
    if info.size.is_some_and(|s| s > MAX_TEXT_FILE) {
        return Err(refuse(
            "That file is too big for the editor (2 MB). Open it with another program.",
        ));
    }
    let mut bytes: Vec<u8> = Vec::new();
    backend
        .read_to(path, &mut bytes, &|_| {}, CancellationToken::new())
        .await?;
    if bytes.len() as u64 > MAX_TEXT_FILE {
        return Err(refuse(
            "That file is too big for the editor (2 MB). Open it with another program.",
        ));
    }
    if bytes.contains(&0) {
        return Err(refuse(
            "That looks like a binary file, not text. Open it with another program.",
        ));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| refuse("That file isn't UTF-8 text, so the editor can't open it safely."))?;
    let crlf = text.contains("\r\n");
    Ok(TextFile {
        size: text.len() as u64,
        text: if crlf {
            text.replace("\r\n", "\n")
        } else {
            text
        },
        crlf,
        modified: info.modified,
    })
}

/// Saves `text` over the file. Unless `force`, it is refused when the file changed on the server
/// since `opened_modified`.
pub async fn write_text(
    backend: &dyn FileBackend,
    path: &str,
    text: &str,
    crlf: bool,
    opened_modified: Option<i64>,
    force: bool,
) -> Result<SaveOutcome, AppError> {
    if text.len() as u64 > MAX_TEXT_FILE {
        return Err(refuse("That's too much text for the editor (2 MB)."));
    }
    if !force {
        let now = backend.stat(path).await?;
        if now.modified != opened_modified {
            return Ok(SaveOutcome::RemoteChanged);
        }
    }
    let out = if crlf {
        text.replace('\n', "\r\n")
    } else {
        text.to_string()
    };
    let mut src: &[u8] = out.as_bytes();
    backend
        .write_from(path, &mut src, &|_| {}, CancellationToken::new())
        .await?;
    let modified = backend.stat(path).await.ok().and_then(|e| e.modified);
    Ok(SaveOutcome::Saved { modified })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::memory::MemoryFiles;

    #[tokio::test]
    async fn a_text_file_is_read_and_written_back_with_its_line_ends() {
        let fs = MemoryFiles::new();
        fs.put("/srv/compose.yml", b"a: 1\r\nb: 2\r\n");
        let file = read_text(&fs, "/srv/compose.yml").await.unwrap();
        assert_eq!(file.text, "a: 1\nb: 2\n");
        assert!(file.crlf);

        let out = write_text(
            &fs,
            "/srv/compose.yml",
            "a: 1\nb: 3\n",
            true,
            file.modified,
            false,
        )
        .await
        .unwrap();
        assert!(matches!(out, SaveOutcome::Saved { .. }));
        let again = read_text(&fs, "/srv/compose.yml").await.unwrap();
        assert_eq!(again.text, "a: 1\nb: 3\n");
        assert!(again.crlf);
    }

    #[tokio::test]
    async fn binary_big_and_non_utf8_files_are_refused() {
        let fs = MemoryFiles::new();
        fs.put("/bin", &[1, 2, 0, 3]);
        fs.put("/latin", &[0x63, 0x61, 0x66, 0xe9]);
        fs.put("/big", &vec![b'x'; (MAX_TEXT_FILE + 1) as usize]);
        for path in ["/bin", "/latin", "/big", "/missing"] {
            assert!(read_text(&fs, path).await.is_err(), "{path}");
        }
    }

    #[tokio::test]
    async fn a_file_changed_elsewhere_is_not_overwritten_unless_forced() {
        let fs = MemoryFiles::new();
        fs.put("/f", b"one\n");
        // The editor opened it when it was last changed at 100; now it says 200.
        let outcome = write_text(&fs, "/f", "two\n", false, Some(100), false).await;
        // The in-memory files have no modification time, so they never match a real one.
        assert_eq!(outcome.unwrap(), SaveOutcome::RemoteChanged);
        assert_eq!(read_text(&fs, "/f").await.unwrap().text, "one\n");
        let forced = write_text(&fs, "/f", "two\n", false, Some(100), true)
            .await
            .unwrap();
        assert!(matches!(forced, SaveOutcome::Saved { .. }));
        assert_eq!(read_text(&fs, "/f").await.unwrap().text, "two\n");
    }
}
