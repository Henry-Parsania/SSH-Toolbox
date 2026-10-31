//! Tests against real shells: `cmd.exe` on Windows, `sh` elsewhere.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio::time::timeout;

use super::testing::RecordingSink;
use super::*;
use crate::error::ErrorCode;

const SIZE: TermSize = TermSize { cols: 80, rows: 24 };
const WAIT: Duration = Duration::from_secs(20);

#[cfg(windows)]
const EOL: &str = "\r\n";
#[cfg(not(windows))]
const EOL: &str = "\n";

fn command(windows: &[&str], unix: &[&str]) -> ShellSpec {
    if cfg!(windows) {
        ShellSpec::new("cmd.exe", windows)
    } else {
        ShellSpec::new("sh", unix)
    }
}

fn interactive_shell() -> ShellSpec {
    command(&[], &[])
}

fn start(spec: &ShellSpec) -> (LocalShell, mpsc::Receiver<BackendEvent>) {
    let (tx, rx) = mpsc::channel(QUEUE_SIZE);
    let shell = LocalShell::spawn(spec, SIZE, tx).expect("spawn");
    (shell, rx)
}

struct Finished {
    output: String,
    code: Option<i32>,
}

/// Reads events until the program ends. Windows' pseudo-terminal asks for the cursor position
/// and prints nothing until it is answered. xterm.js answers that in the real app, so the test
/// answers it here.
async fn run_to_exit(shell: &LocalShell, rx: &mut mpsc::Receiver<BackendEvent>) -> Finished {
    let mut output = String::new();
    let mut answered = false;
    let result = timeout(WAIT, async {
        loop {
            match rx.recv().await.expect("queue closed before the exit event") {
                BackendEvent::Data(data) => {
                    output.push_str(&String::from_utf8_lossy(&data));
                    if !answered && output.contains("\x1b[6n") {
                        shell
                            .write(b"\x1b[1;1R")
                            .await
                            .expect("answer cursor query");
                        answered = true;
                    }
                }
                BackendEvent::Exited { code } => return code,
                BackendEvent::Failed(e) => panic!("unexpected failure: {e}"),
            }
        }
    })
    .await;
    let code = result.unwrap_or_else(|_| panic!("timed out. Output so far: {output:?}"));
    // Exactly one final event: the queue must now close with nothing else in it.
    let rest = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("queue never closed after the exit event");
    assert!(rest.is_none(), "an event arrived after the exit: {rest:?}");
    Finished { output, code }
}

/// Starts an interactive shell, types `lines`, then `exit`, and returns everything it printed.
async fn type_into_shell(lines: &[&str], size: Option<TermSize>) -> Finished {
    let (shell, mut rx) = start(&interactive_shell());
    if let Some(size) = size {
        shell.resize(size).await.expect("resize");
    }
    for line in lines {
        shell
            .write(format!("{line}{EOL}").as_bytes())
            .await
            .unwrap();
    }
    shell.write(format!("exit{EOL}").as_bytes()).await.unwrap();
    run_to_exit(&shell, &mut rx).await
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_a_command_and_reports_its_output_and_exit_code() {
    let spec = command(
        &["/C", "echo toolbox-marker"],
        &["-c", "echo toolbox-marker"],
    );
    let (shell, mut rx) = start(&spec);
    let done = run_to_exit(&shell, &mut rx).await;
    assert!(done.output.contains("toolbox-marker"), "{:?}", done.output);
    assert_eq!(done.code, Some(0));
}

#[tokio::test(flavor = "multi_thread")]
async fn reports_a_non_zero_exit_code() {
    let (shell, mut rx) = start(&command(&["/C", "exit 7"], &["-c", "exit 7"]));
    let done = run_to_exit(&shell, &mut rx).await;
    assert_eq!(done.code, Some(7));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_interactive_shell_runs_what_is_typed() {
    // The sum is computed by the shell, so it only appears in the output and not in the echo.
    let line = if cfg!(windows) {
        "set /a 6000+7"
    } else {
        "echo $((6000+7))"
    };
    let done = type_into_shell(&[line], None).await;
    assert!(done.output.contains("6007"), "{:?}", done.output);
    assert_eq!(done.code, Some(0));
}

#[tokio::test(flavor = "multi_thread")]
async fn sets_the_terminal_type_and_colour_support() {
    let line = if cfg!(windows) {
        "echo %TERM% %COLORTERM%"
    } else {
        "echo $TERM $COLORTERM"
    };
    let done = type_into_shell(&[line], None).await;
    assert!(
        done.output.contains("xterm-256color truecolor"),
        "{:?}",
        done.output
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn starts_in_the_home_folder() {
    let home = directories::BaseDirs::new()
        .unwrap()
        .home_dir()
        .to_string_lossy()
        .into_owned();
    let line = if cfg!(windows) { "cd" } else { "pwd" };
    let done = type_into_shell(&[line], None).await;
    assert!(
        done.output.contains(&home),
        "{home} not in {:?}",
        done.output
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn resizing_reaches_the_program() {
    let line = if cfg!(windows) {
        "mode con"
    } else {
        "stty size"
    };
    let done = type_into_shell(
        &[line],
        Some(TermSize {
            cols: 100,
            rows: 30,
        }),
    )
    .await;
    if cfg!(windows) {
        let columns = done
            .output
            .lines()
            .find(|l| l.contains("Columns:"))
            .unwrap_or_else(|| panic!("no Columns line in {:?}", done.output));
        assert_eq!(
            columns.split_whitespace().last(),
            Some("100"),
            "{columns:?}"
        );
    } else {
        assert!(done.output.contains("30 100"), "{:?}", done.output);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn closing_ends_a_running_program() {
    let spec = command(&["/C", "ping -n 60 127.0.0.1 >nul"], &["-c", "sleep 60"]);
    let (shell, mut rx) = start(&spec);
    let started = Instant::now();
    shell.close().await.unwrap();
    shell.close().await.unwrap(); // Safe to call twice.
    run_to_exit(&shell, &mut rx).await;
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "the program kept running after close"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn writing_and_resizing_after_the_program_ended_are_quiet() {
    let (shell, mut rx) = start(&command(&["/C", "exit 0"], &["-c", "exit 0"]));
    run_to_exit(&shell, &mut rx).await;
    // Give the waiter thread time to close the terminal.
    tokio::time::sleep(Duration::from_millis(200)).await;
    shell.write(b"ignored").await.expect("write after exit");
    shell.resize(SIZE).await.expect("resize after exit");
    shell.close().await.expect("close after exit");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_program_gives_a_clear_error() {
    let (tx, _rx) = mpsc::channel(QUEUE_SIZE);
    let spec = ShellSpec::new("definitely-not-a-real-program-xyz", &[]);
    let error = LocalShell::spawn(&spec, SIZE, tx)
        .err()
        .expect("should fail");
    assert!(
        error.message.contains("definitely-not-a-real-program-xyz"),
        "{error}"
    );
    assert_eq!(error.code, ErrorCode::NotFound, "{error}");
}

#[tokio::test(flavor = "multi_thread")]
async fn output_flows_through_the_pump_in_order() {
    // About 200 KB of numbered lines, kept under HIGH so the test needs no acks.
    let (last, windows_loop) = ("line4000", "for /L %i in (1,1,4000) do @echo line%i");
    let spec = command(
        &["/C", windows_loop],
        &[
            "-c",
            "i=1; while [ $i -le 4000 ]; do echo line$i; i=$((i+1)); done",
        ],
    );
    let (tx, rx) = mpsc::channel(QUEUE_SIZE);
    let shell = Arc::new(LocalShell::spawn(&spec, SIZE, tx).unwrap());
    let sink = Arc::new(RecordingSink::default());
    let pump = spawn_pump(rx, sink.clone(), PumpConfig::default());

    // Answer the Windows cursor query, as xterm.js would.
    let answer = {
        let shell = Arc::clone(&shell);
        let sink = Arc::clone(&sink);
        tokio::spawn(async move {
            for _ in 0..200 {
                let text: String = sink
                    .all()
                    .iter()
                    .filter_map(|s| match s {
                        super::testing::Sent::Data(d) => {
                            Some(String::from_utf8_lossy(d).into_owned())
                        }
                        _ => None,
                    })
                    .collect();
                if text.contains("\x1b[6n") {
                    let _ = shell.write(b"\x1b[1;1R").await;
                    return;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
    };

    timeout(WAIT, pump.finished())
        .await
        .expect("pump never finished");
    answer.abort();

    let text: String = sink
        .all()
        .iter()
        .filter_map(|s| match s {
            super::testing::Sent::Data(d) => Some(String::from_utf8_lossy(d).into_owned()),
            _ => None,
        })
        .collect();
    assert!(
        text.contains(last),
        "last line missing from {} bytes",
        text.len()
    );
    assert!(sink.chunk_sizes().iter().all(|&n| n <= MAX_CHUNK));
    assert_eq!(
        sink.statuses(),
        vec![TerminalStatus::Exited { code: Some(0) }]
    );
}
