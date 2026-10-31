//! Library check: portable-pty starts a shell, runs echo, reads output, and resizes.
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

fn main() {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("open pty");

    let marker = "toolbox-pty-ok";
    let mut cmd = if cfg!(windows) {
        let mut c = CommandBuilder::new("cmd.exe");
        c.args(["/C", &format!("echo {marker}")]);
        c
    } else {
        let mut c = CommandBuilder::new("sh");
        c.args(["-c", &format!("echo {marker}")]);
        c
    };
    cmd.env("TERM", "xterm-256color");

    let mut child = pair.slave.spawn_command(cmd).expect("spawn");
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().expect("reader");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });

    // Windows ConPTY asks for the cursor position (ESC[6n) and prints nothing until answered.
    // xterm.js answers this itself in the real app, so the check has to do it here.
    let mut writer = pair.master.take_writer().expect("writer");
    let mut output = Vec::new();
    let mut answered = false;
    while let Ok(chunk) = rx.recv_timeout(Duration::from_secs(5)) {
        output.extend(chunk);
        if !answered && output.windows(4).any(|w| w == b"\x1b[6n") {
            writer.write_all(b"\x1b[1;1R").expect("answer cursor query");
            answered = true;
        }
        if String::from_utf8_lossy(&output).contains(marker) {
            break;
        }
    }
    let text = String::from_utf8_lossy(&output);
    assert!(text.contains(marker), "marker not seen in output: {text:?}");

    pair.master
        .resize(PtySize {
            rows: 40,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("resize");
    let size = pair.master.get_size().expect("get_size");
    assert_eq!((size.rows, size.cols), (40, 120));

    let status = child.wait().expect("wait");
    println!(
        "portable-pty OK: output seen, resized to 120x40, exit success={}",
        status.success()
    );
}
