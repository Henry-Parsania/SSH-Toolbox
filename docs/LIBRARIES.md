# Library checks (task 0.7)

Each library was tried with a small program in `crates/core/examples/`. Run one with
`cargo run -p toolbox-core --example <name>`. CI runs them on every push (see `.github/workflows/ci.yml`).

Results marked **CI** are decided by the first green CI run, since this PC can't run them.

| Library | Version | What was tried | Where | Result |
|---|---|---|---|---|
| `portable-pty` | 0.9.0 | `check_pty`: start a shell, run `echo`, read the output, resize | Windows (this PC) | **Works.** See note 1 |
| `portable-pty` | 0.9.0 | Same check | Linux | **CI** |
| `russh` (`ring`) | =0.65.0 | `check_ssh`: password login, key login, agent login to the test container | Linux | **CI** |
| `russh` (`ring`) | =0.65.0 | Builds on Windows with no extra build tools | Windows (this PC) | **Works.** The build needed only the C++ tools already installed |
| `russh` Windows agent | =0.65.0 | `check_win_agent`: connect to `\\.\pipe\openssh-ssh-agent`, list keys | Windows (this PC) | **Works.** Connected, 1 key listed |
| `russh` host keys | =0.65.0 | `check_hostkeys`: every row of the INTERFACES.md section 7 table, with a temporary file | Windows (this PC) | **Works, with one deviation.** See note 2 |
| `russh-sftp` | 3.0.1 | `check_ssh`: list a folder on the same connection as the password login | Linux | **CI** |
| `suppaftp` | 12.2.1 | `check_ftp`: builds with `tokio-rustls-ring` and `deprecated` | Windows (this PC) | **Works.** A real login is tested in phase 3 against the `vsftpd` container |
| `tokio-serial` | 5.5.0 | `check_serial`: build and list ports | Windows (this PC) | **Works.** Found COM8 and COM9 |
| `bollard` | 0.21.1 | `check_docker`: ping and list containers | Windows (this PC) | **Untested here.** It built, but Docker Desktop wasn't running |
| `bollard` | 0.21.1 | Same check | Linux | **CI** |
| `keyring-core` + `windows-native-keyring-store` | 1.0.1 + 1.1.0 | `check_keychain`: set, read and delete `SSHToolbox-test`, then confirm it's gone | Windows (this PC) | **Works** |
| `keyring-core` + `zbus-secret-service-keyring-store` | 1.0.1 + 1.0.1 | `check_keychain`: builds only | Linux | **CI.** See note 3 |
| Tauri raw channel | 2.12 | Send bytes with `InvokeResponseBody::Raw`; check what type the interface receives | Windows (this PC) | **Works.** See note 4 |

## Notes

1. **ConPTY waits for a reply on Windows.** The pseudo-terminal sends a cursor-position request (`ESC[6n`) and prints nothing until it gets an answer. xterm.js answers automatically, so the real app is fine, but any test that reads a pty without a terminal emulator must answer `ESC[1;1R` itself, as `check_pty` does. Task 1.2's tests need the same.

2. **`russh` treats an unreadable `known_hosts` file as "no keys".** `known_host_keys_path` does `File::open(path)` and returns an empty list on *any* error, so `check_known_hosts_path` returns `Ok(false)`. That would show an attacker's key as a harmless first connection, which breaks the last row of the INTERFACES.md table. **Task 2.3 must read the file itself before calling `russh`:** a missing file means empty, and any other read error fails the connection. `check_hostkeys` shows both behaviors and asserts them, so it will fail if a future `russh` fixes this, as a reminder to simplify.
   The other rows behave as documented: a changed key of the same type gives `KeyChanged { line }`, and a key of a different type gives `Ok(false)` while the host still has recorded keys. A corrupt entry gives an error, so it fails closed. Two small points: the first entry in a new file is written on line 2, because `russh` adds a leading blank line, and the reported line number matches the file. Non-default ports use the `[host]:port` form as a separate entry.

3. **`zbus-secret-service-keyring-store` has no default crypto feature.** It must be given one. This project uses `crypto-rust`, with no async runtime, so there's no OpenSSL dependency and every call runs on `spawn_blocking` as INTERFACES.md section 10 requires.

4. **Raw bytes arrive as an ArrayBuffer.** A Rust command sent `InvokeResponseBody::Raw(vec![0, 1, 2, 255, 128])` through a `Channel<InvokeResponseBody>`, and the page's `Channel<ArrayBuffer>` received an `ArrayBuffer` with exactly those bytes, including values above 127. This was a temporary command, removed after the check. Task 1.1 builds the real one.
