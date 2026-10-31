//! Opening a terminal session: backend, output pump and registry entry (INTERFACES.md section 5).

use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::mpsc;

use super::backend::{QUEUE_SIZE, TermSize, TerminalBackend};
use super::local::LocalShell;
use super::pump::{PumpConfig, spawn_pump};
use super::registry::{Session, SessionRegistry};
use super::shells::shell_spec;
use super::sink::{OutputSink, TerminalStatus};
use super::wsl::wsl_spec;
use crate::error::{AppError, ErrorCode};

/// What to open in a pane. More types are added in the phases that build them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionSpec {
    Local { shell_id: String },
    Wsl { distro: String },
}

const MAX_DIMENSION: u16 = 1000;

/// Checks a terminal size from the interface. A zero size breaks pseudo-terminals, and a huge
/// one wastes memory.
pub fn checked_size(cols: u16, rows: u16) -> Result<TermSize, AppError> {
    if cols == 0 || rows == 0 || cols > MAX_DIMENSION || rows > MAX_DIMENSION {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "That terminal size isn't valid.",
        ));
    }
    Ok(TermSize { cols, rows })
}

fn blocking_failed() -> AppError {
    AppError::new(ErrorCode::Internal, "Couldn't start the terminal.")
}

/// Opens a session and returns its ID. Output and statuses go to `sink`.
pub async fn open_terminal(
    registry: &SessionRegistry,
    spec: SessionSpec,
    size: TermSize,
    sink: Arc<dyn OutputSink>,
) -> Result<String, AppError> {
    let (events, queue) = mpsc::channel(QUEUE_SIZE);

    let backend: Arc<dyn TerminalBackend> = match spec {
        SessionSpec::Local { shell_id } => {
            let shell = tokio::task::spawn_blocking(move || {
                let spec = shell_spec(&shell_id)?;
                LocalShell::spawn(&spec, size, events)
            })
            .await
            .map_err(|_| blocking_failed())??;
            Arc::new(shell)
        }
        SessionSpec::Wsl { distro } => {
            let shell = tokio::task::spawn_blocking(move || {
                let spec = wsl_spec(&distro)?;
                LocalShell::spawn(&spec, size, events)
            })
            .await
            .map_err(|_| blocking_failed())??;
            Arc::new(shell)
        }
    };

    let pump = spawn_pump(queue, Arc::clone(&sink), PumpConfig::default());
    let id = registry.insert(Session::new(backend, pump));
    let _ = sink.send_status(TerminalStatus::Connected);
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::shells::list_shells;
    use crate::terminal::testing::{RecordingSink, Sent};
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    #[test]
    fn sizes_must_be_sensible() {
        assert_eq!(
            checked_size(80, 24).unwrap(),
            TermSize { cols: 80, rows: 24 }
        );
        assert!(checked_size(1000, 1000).is_ok());
        for (cols, rows) in [(0, 24), (80, 0), (1001, 24), (80, 1001), (u16::MAX, 24)] {
            let error = checked_size(cols, rows).unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidInput, "{cols}x{rows}");
        }
    }

    #[test]
    fn the_interface_spec_deserializes() {
        let spec: SessionSpec =
            serde_json::from_str(r#"{"type":"local","shell_id":"cmd"}"#).unwrap();
        assert_eq!(
            spec,
            SessionSpec::Local {
                shell_id: "cmd".into()
            }
        );
        assert!(serde_json::from_str::<SessionSpec>(r#"{"type":"nope"}"#).is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_unknown_shell_is_not_opened_or_registered() {
        let registry = SessionRegistry::new();
        let sink = Arc::new(RecordingSink::default());
        let error = open_terminal(
            &registry,
            SessionSpec::Local {
                shell_id: "no-such-shell".into(),
            },
            TermSize { cols: 80, rows: 24 },
            sink,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
        assert!(registry.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn opens_a_real_shell_and_runs_it_to_the_end() {
        let registry = SessionRegistry::new();
        let sink = Arc::new(RecordingSink::default());
        let shell = list_shells().remove(0);
        let id = open_terminal(
            &registry,
            SessionSpec::Local { shell_id: shell.id },
            TermSize { cols: 80, rows: 24 },
            sink.clone(),
        )
        .await
        .unwrap();
        assert_eq!(registry.len(), 1);

        let session = registry.get(&id).unwrap();
        let eol = if cfg!(windows) { "\r\n" } else { "\n" };
        session
            .write(format!("exit{eol}").as_bytes())
            .await
            .unwrap();

        // Windows' pseudo-terminal waits for a cursor position reply, which xterm.js sends.
        let finished = timeout(Duration::from_secs(20), async {
            let mut answered = false;
            loop {
                if !answered
                    && sink.all().iter().any(|s| {
                        matches!(s, Sent::Data(d) if String::from_utf8_lossy(d).contains("\x1b[6n"))
                    })
                {
                    session.write(b"\x1b[1;1R").await.unwrap();
                    answered = true;
                }
                if sink
                    .statuses()
                    .iter()
                    .any(|s| matches!(s, TerminalStatus::Exited { .. }))
                {
                    return;
                }
                sleep(Duration::from_millis(50)).await;
            }
        })
        .await;
        assert!(finished.is_ok(), "shell never exited: {:?}", sink.all());

        let statuses = sink.statuses();
        assert_eq!(statuses.first(), Some(&TerminalStatus::Connected));
        assert!(matches!(
            statuses.last(),
            Some(TerminalStatus::Exited { .. })
        ));

        registry.close(&id).await.unwrap();
        assert!(registry.is_empty());
    }
}
