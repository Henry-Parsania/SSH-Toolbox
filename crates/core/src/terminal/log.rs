//! Session logging (task 7.5): a copy of what a terminal shows, as plain text in a file.
//!
//! It is only used when the user has chosen a log folder. The text is what the program printed,
//! so it can contain anything the server showed. Typed passwords aren't echoed, so they don't
//! appear, but the user is told to treat the files as private.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::sink::{OutputSink, TerminalStatus};
use crate::error::AppError;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Text,
    /// After Escape.
    Escape,
    /// Inside `ESC [ ... final`.
    Csi,
    /// Inside `ESC ] ... BEL` or `ESC ] ... ESC \`.
    Osc,
    /// Escape seen inside an OSC, waiting for `\`.
    OscEscape,
    /// A two-byte sequence such as `ESC ( B`: one more byte to skip.
    Skip,
}

/// Turns terminal output into plain text: colours, cursor moves and window titles are removed, and
/// so are carriage returns and other control characters. It remembers where it was between
/// chunks, so a sequence split in two is still removed.
pub struct PlainText {
    state: State,
}

impl PlainText {
    pub fn new() -> Self {
        Self { state: State::Text }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(bytes.len());
        for &b in bytes {
            self.state = match (self.state, b) {
                (State::Text, 0x1b) => State::Escape,
                (State::Text, b'\n' | b'\t') => {
                    out.push(b);
                    State::Text
                }
                // Other control characters (including \r, backspace and bell) are dropped.
                (State::Text, 0x00..=0x1f | 0x7f) => State::Text,
                (State::Text, _) => {
                    out.push(b);
                    State::Text
                }
                (State::Escape, b'[') => State::Csi,
                (State::Escape, b']') => State::Osc,
                (State::Escape, b'(' | b')' | b'*' | b'+' | b'#') => State::Skip,
                (State::Escape, _) => State::Text,
                (State::Csi, 0x40..=0x7e) => State::Text,
                (State::Csi, _) => State::Csi,
                (State::Osc, 0x07) => State::Text,
                (State::Osc, 0x1b) => State::OscEscape,
                (State::Osc, _) => State::Osc,
                (State::OscEscape, b'\\') => State::Text,
                (State::OscEscape, _) => State::Osc,
                (State::Skip, _) => State::Text,
            };
        }
        out
    }
}

impl Default for PlainText {
    fn default() -> Self {
        Self::new()
    }
}

/// The current time as `YYYYMMDD-HHMMSS` in UTC, for log file names.
pub fn utc_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    stamp_from_unix(secs)
}

fn stamp_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    // Days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// A name that is safe in a file name on every system.
pub fn safe_file_part(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('-');
    let cut: String = cleaned.chars().take(40).collect();
    if cut.is_empty() {
        "session".into()
    } else {
        cut
    }
}

/// Where the log of a new session goes: `<folder>/<name>-<time>.log`. A number is added if the
/// file already exists. The folder is created if it is missing.
pub fn log_path(folder: &Path, name: &str, stamp: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(folder)?;
    let base = format!("{}-{}", safe_file_part(name), stamp);
    let mut path = folder.join(format!("{base}.log"));
    let mut n = 2;
    while path.exists() {
        path = folder.join(format!("{base}-{n}.log"));
        n += 1;
    }
    Ok(path)
}

/// Passes everything on to `next` and also writes the plain text of the output to a file. A
/// problem writing the file stops the logging but never disturbs the terminal.
pub struct LogSink {
    next: Arc<dyn OutputSink>,
    file: Mutex<Option<(BufWriter<File>, PlainText)>>,
}

impl LogSink {
    pub fn new(next: Arc<dyn OutputSink>, path: &Path) -> std::io::Result<Self> {
        let file = File::options().create_new(true).append(true).open(path)?;
        Ok(Self {
            next,
            file: Mutex::new(Some((BufWriter::new(file), PlainText::new()))),
        })
    }

    fn finish(&self) {
        if let Some((mut writer, _)) = self.file.lock().unwrap().take() {
            let _ = writer.flush();
        }
    }
}

impl OutputSink for LogSink {
    fn send_data(&self, chunk: Vec<u8>) -> Result<(), AppError> {
        {
            let mut guard = self.file.lock().unwrap();
            if let Some((writer, plain)) = guard.as_mut() {
                let text = plain.feed(&chunk);
                if writer.write_all(&text).is_err() || writer.flush().is_err() {
                    *guard = None;
                }
            }
        }
        self.next.send_data(chunk)
    }

    fn send_status(&self, status: TerminalStatus) -> Result<(), AppError> {
        if matches!(
            status,
            TerminalStatus::Exited { .. } | TerminalStatus::Failed { .. }
        ) {
            self.finish();
        }
        self.next.send_status(status)
    }
}

impl Drop for LogSink {
    fn drop(&mut self) {
        self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(chunks: &[&[u8]]) -> String {
        let mut p = PlainText::new();
        let mut out = Vec::new();
        for c in chunks {
            out.extend(p.feed(c));
        }
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn colours_titles_and_cursor_moves_are_removed() {
        assert_eq!(
            plain(&[b"\x1b[1;32mok\x1b[0m done\r\n\x1b]0;title\x07next\x1b[2K\x1b(Bend"]),
            "ok done\nnextend"
        );
    }

    #[test]
    fn a_sequence_split_between_chunks_is_still_removed() {
        assert_eq!(
            plain(&[b"a\x1b[3", b"1mb\x1b]0;ti", b"tle\x1b", b"\\c"]),
            "abc"
        );
    }

    #[test]
    fn text_with_non_ascii_letters_survives() {
        assert_eq!(plain(&["héllo ✓\n".as_bytes()]), "héllo ✓\n");
    }

    #[test]
    fn the_time_stamp_is_right() {
        assert_eq!(stamp_from_unix(0), "19700101-000000");
        assert_eq!(stamp_from_unix(1_709_210_096), "20240229-123456");
        assert_eq!(stamp_from_unix(1_793_790_000), "20261104-110000");
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(safe_file_part("my server: prod/1"), "my-server--prod-1");
        assert_eq!(safe_file_part("../../x"), "x");
        assert_eq!(safe_file_part("///"), "session");
    }

    struct Collect(Mutex<Vec<u8>>);
    impl OutputSink for Collect {
        fn send_data(&self, chunk: Vec<u8>) -> Result<(), AppError> {
            self.0.lock().unwrap().extend(chunk);
            Ok(())
        }
        fn send_status(&self, _: TerminalStatus) -> Result<(), AppError> {
            Ok(())
        }
    }

    #[test]
    fn the_sink_writes_plain_text_and_still_passes_output_on() {
        let dir = tempfile::tempdir().unwrap();
        let path = log_path(&dir.path().join("logs"), "box", "20260101-000000").unwrap();
        let next = Arc::new(Collect(Mutex::new(Vec::new())));
        let sink = LogSink::new(next.clone(), &path).unwrap();
        sink.send_data(b"\x1b[31mhello\x1b[0m\r\n".to_vec())
            .unwrap();
        sink.send_status(TerminalStatus::Exited { code: Some(0) })
            .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello\n");
        assert_eq!(
            next.0.lock().unwrap().as_slice(),
            b"\x1b[31mhello\x1b[0m\r\n"
        );
    }

    #[test]
    fn a_second_log_with_the_same_name_gets_a_number() {
        let dir = tempfile::tempdir().unwrap();
        let first = log_path(dir.path(), "box", "t").unwrap();
        std::fs::write(&first, "x").unwrap();
        let second = log_path(dir.path(), "box", "t").unwrap();
        assert_ne!(first, second);
        assert!(second.to_string_lossy().ends_with("box-t-2.log"));
    }
}
