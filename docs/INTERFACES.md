# SSH Toolbox: Interfaces and Exact Details

This is the design so far, and it's here to save working things out twice. Improve it when you find something better, and update this file when you do. Sections 7, 10 and 15 handle logins and passwords, so take a little more care when changing those.

What real testing found is in [LIBRARIES.md](LIBRARIES.md). Where that differs from this file, trust it.

## 1. App identity

| Item | Value |
|---|---|
| Product name | `SSH Toolbox` (working title) |
| Tauri identifier | `dev.sshtoolbox.desktop` |
| Core crate | `toolbox-core`, in `crates/core` |
| Config folder | `directories::ProjectDirs::from("dev", "sshtoolbox", "SSHToolbox")`, `config_dir()` |
| Keychain service name | `SSHToolbox` |

## 2. Errors and secrets

```rust
#[derive(Debug, Clone, serde::Serialize, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String, // Plain English, shown to the user. Never contains secrets
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotFound, PermissionDenied, ConnectionFailed, ConnectionLost, AuthFailed,
    HostKeyChanged, HostKeyRejected, CertificateRejected, KeychainUnavailable,
    AlreadyExists, Cancelled, Unsupported, InvalidInput, Internal,
}
```

Messages tell the user what happened and what to do next. Every Tauri command returns `Result<T, AppError>`.

`Secret` is the only type that holds passwords and passphrases:
- It wraps `zeroize::Zeroizing<String>`.
- Its `Debug` prints `[redacted]`, and it doesn't implement `Display`, `Clone` or `Serialize`. It implements `Deserialize` only so it can arrive in a command.
- It has one accessor, `expose() -> &str`.

Unit tests must confirm that `format!("{:?}", secret)` doesn't contain the value.

## 3. Terminal interface

```rust
pub struct TermSize { pub cols: u16, pub rows: u16 }

pub enum BackendEvent {
    Data(Vec<u8>),
    Exited { code: Option<i32> },   // Program ended normally
    Failed(AppError),               // Connection lost, device unplugged and so on
}

#[async_trait]
pub trait TerminalBackend: Send + Sync {
    async fn write(&self, data: &[u8]) -> Result<(), AppError>;
    /// Backends without a terminal size, such as serial, return Ok(()) and do nothing.
    async fn resize(&self, size: TermSize) -> Result<(), AppError>;
    /// Ends the session. Safe to call twice.
    async fn close(&self) -> Result<(), AppError>;
}
```

A backend's constructor receives a `tokio::sync::mpsc::Sender<BackendEvent>` with room for 32 messages. It sends each read as `Data` of at most 16 KiB. It must `await` the send, so that a full queue stops it reading. That's how slowing down reaches the running program. Send exactly one `Exited` or `Failed` as the last message.

`crates/core` delivers output through this trait, implemented in `src-tauri` using Tauri channels:

```rust
pub trait OutputSink: Send + Sync + 'static {
    fn send_data(&self, chunk: Vec<u8>) -> Result<(), AppError>;
    fn send_status(&self, status: TerminalStatus) -> Result<(), AppError>;
}
```

### Local shell details
- Use `portable_pty::native_pty_system()`. Run the blocking read on a dedicated `std::thread`, and use `Sender::blocking_send` there.
- Set `TERM=xterm-256color` and `COLORTERM=truecolor`. Start in the home folder.
- **Linux:** use `$SHELL`, otherwise `/bin/bash`, with `-l`.
- **Windows shell list:** each only if present: `powershell.exe`, `pwsh.exe`, `cmd.exe`, and Git Bash.
- **Git Bash:** look for `C:\Program Files\Git\bin\bash.exe`, or find `git.exe` on `PATH` and go to `..\bin\bash.exe` from its `cmd` folder. Start it with `--login -i`. **Never use `C:\Windows\System32\bash.exe`, which launches WSL instead.**

### WSL details
- List with `wsl.exe --list --quiet`. **Its output is UTF-16LE,** so decode it as that, and trim whitespace and any stray NUL characters. Confirmed on this PC.
- Remove `docker-desktop` and `docker-desktop-data`, ignoring case.
- Open with `wsl.exe -d <distro> --cd ~` through `portable-pty`. Pass the distro as its own argument, never through a shell string.
- If `wsl.exe` is missing or fails, return an empty list. It isn't an error.
- All WSL code is under `#[cfg(windows)]`. On other systems, `list_wsl_distros` returns an empty list.

## 4. Terminal output and flow control

Output moves like this:

```
Backend -> queue of 32 -> pump task -> Tauri channel -> xterm.write(chunk, callback) -> terminal_ack
```

Start with these numbers, and tune them if measurements suggest better ones:

| Name | Value | Meaning |
|---|---|---|
| Largest chunk | 64 KiB | Pump joins smaller pieces up to this size |
| Flush timer | 8 ms | Pump sends what it has after this long, even if small |
| `HIGH` | 500,000 bytes unacknowledged | Pump stops taking from the queue |
| `LOW` | 100,000 bytes unacknowledged | Pump starts again |
| Ack threshold | 32 KiB, or 50 ms after the last unsent ack | When the interface reports processed bytes |

`LOW` must stay above the ack threshold, or the pump could wait forever for an ack the interface is holding back. xterm's own guide says `HIGH` should stay at 500K or below to keep keystrokes responsive.

**Pump rules:**
- Send `Data` chunks as `InvokeResponseBody::Raw` through `tauri::ipc::Channel<InvokeResponseBody>`. Never use Tauri events for terminal output, as Tauri's docs say they aren't built for high throughput. Don't convert bytes to strings, because a multi-byte character can be split across chunks. xterm's `write` accepts `Uint8Array` and handles that.
- Each session has its own pump, so one busy pane can't hold up another.
- Send statuses on a separate channel. After `Exited` or `Failed`, send any remaining data first.
- Unit-test the pump with a fake backend and sink for: batching, stopping at `HIGH`, resuming at `LOW`, final data arriving before the exit status, and no deadlock with small outputs.

**Interface rules:**
- Convert each `ArrayBuffer` to `Uint8Array` and call `term.write(chunk, callback)`. The callback adds the chunk's size to a pending count, and sends `terminal_ack` when the threshold is reached.
- Keep xterm instances alive when a tab is hidden. Hide them with CSS, and never destroy them, or scrollback is lost.
- Don't call `fit()` on a hidden pane. Fit when it becomes visible, and debounce resizes by 50 ms.

## 5. Tauri commands

Only `src/lib/backend/` calls these. Add each in the phase that needs it.

| Command | Purpose | Phase |
|---|---|---|
| `platform_info() -> {os, wsl_available, windows_build}` | Hide WSL, set xterm's `windowsPty` | 0, 1 |
| `list_shells() -> Shell[]` | Local shell menu | 1 |
| `list_wsl_distros() -> string[]` | WSL menu | 1 |
| `terminal_open(spec, cols, rows, on_output, on_status) -> session_id` | Open any session type | 1 |
| `terminal_write(session_id, data: number[])` | Keystrokes and paste | 1 |
| `terminal_resize(session_id, cols, rows)` | | 1 |
| `terminal_ack(session_id, bytes)` | Flow control | 1 |
| `terminal_close(session_id)` | | 1 |
| `open_link(url)` | Backend rejects anything except `http` and `https` | 1 |
| `save_text_file(path, contents)` | Save session output (T9) | 1 |
| `settings_get()`, `settings_set(settings)` | | 1 |
| `layout_get()`, `layout_set(layout)` | P6 | 1 |
| `prompt_respond(prompt_id, answer)` | Answer a prompt from section 7 | 2 |
| `sessions_get()`, `sessions_save(file)` | Saved sessions, never with secrets | 2 |
| `secret_set(session_id, kind, value)`, `secret_delete(session_id, kind)`, `secret_exists(session_id, kind) -> bool` | **There is no command that returns a secret** | 2 |
| `import_ssh_config() -> SshSession[]` | S4 | 2 |
| `key_setup_start(session_id, options) -> job_id` plus progress channel | K1–K7 | 2 |
| `tunnel_start(session_id, spec)`, `tunnel_stop(tunnel_id)`, `tunnel_list()` | S7 | 2 |
| `files_open(spec) -> files_id`, `files_close(files_id)` | Start and end a file session | 3 |
| `files_list`, `files_mkdir`, `files_rename`, `files_remove`, `files_chmod` | Using `files_id` | 3 |
| `transfer_start(spec) -> transfer_id`, `transfer_cancel(transfer_id)`, plus progress channel | Queue | 3 |
| `serial_list_ports()` | R1 | 4 |
| `serial_set_control(session_id, dtr, rts)`, `serial_send_break(session_id)` | R7 | 4 |
| `docker_connect(target) -> docker_id`, `docker_list_containers`, `docker_action(id, action)`, `docker_inspect`, `docker_logs(id, on_output)`, `docker_stats(id, on_stats)` | Phase 5 | 5 |

`spec` for `terminal_open`, in TypeScript:

```ts
type SessionSpec =
  | { type: "local"; shell_id: string }
  | { type: "wsl"; distro: string }
  | { type: "ssh"; saved_session_id: string }
  | { type: "serial"; saved_session_id: string }
  | { type: "docker_exec"; docker_id: string; container_id: string; command: string[] };
```

Status messages:

```ts
type TerminalStatus =
  | { type: "connecting" }
  | { type: "connected" }
  | { type: "exited"; code: number | null }
  | { type: "failed"; error: { code: string; message: string } }
  | { type: "prompt"; prompt_id: string; prompt: Prompt };
```

## 6. File interface

```rust
pub struct FileEntry {
    pub name: String, pub path: String, pub kind: FileKind, // File, Dir, Symlink, Other
    pub size: Option<u64>, pub modified: Option<i64>, pub permissions: Option<u32>,
}

pub struct FileCapabilities { pub chmod: bool }

#[async_trait]
pub trait FileBackend: Send + Sync {
    fn capabilities(&self) -> FileCapabilities;
    async fn list(&self, path: &str) -> Result<Vec<FileEntry>, AppError>;
    async fn stat(&self, path: &str) -> Result<FileEntry, AppError>;
    async fn mkdir(&self, path: &str) -> Result<(), AppError>;
    async fn rename(&self, from: &str, to: &str) -> Result<(), AppError>;
    async fn remove_file(&self, path: &str) -> Result<(), AppError>;
    async fn remove_dir(&self, path: &str) -> Result<(), AppError>; // Empty folders only
    async fn chmod(&self, path: &str, mode: u32) -> Result<(), AppError>;
    /// Streams the file into `dest`. Stops with ErrorCode::Cancelled when `cancel` fires.
    async fn read_to(&self, path: &str, dest: &mut (dyn AsyncWrite + Send + Unpin),
        progress: &(dyn Fn(u64) + Send + Sync), cancel: CancellationToken) -> Result<(), AppError>;
    /// Streams `src` into the file, replacing it if present.
    async fn write_from(&self, path: &str, src: &mut (dyn AsyncRead + Send + Unpin),
        progress: &(dyn Fn(u64) + Send + Sync), cancel: CancellationToken) -> Result<(), AppError>;
}
```

- `chmod` returns `Unsupported` when `capabilities().chmod` is false, and the interface hides that option.
- Transfers between any two backends connect `read_to` and `write_from` with `tokio::io::duplex`. The queue never knows the backend type.
- **Safe writes:** the queue writes to `<name>.sshtoolbox-part` and renames it when finished. It removes the part file after a failure or cancel, so a broken transfer never replaces a good file.
- **Overwrites:** the queue calls `stat` first, and asks the user if the file exists (F4). Choices are overwrite, skip, overwrite all and skip all.
- **Folder delete:** the queue lists contents and removes them from the bottom up, after one confirmation that names the folder and item count.
- **SFTP:** open on the existing `russh` connection: `channel_open_session`, `request_subsystem(true, "sftp")`, then `SftpSession::new(channel.into_stream())`. This is what makes F1 possible with no second login.
- **FTP:** a control connection handles one thing at a time. Keep one for browsing, and open a new login for each transfer. Use passive mode.
- **WSL (W4):** use the local backend with paths under `\\wsl.localhost\<distro>\`.

**Remote file names (SEC11).** Before writing a downloaded folder to disk, reject any entry name that is empty, is `.` or `..`, or contains `/`, `\` or a NUL character. Also reject Windows drive prefixes such as `C:`. Report rejected names and carry on with the rest. Skip remote symlinks when downloading folders. A small unit test of the name check is enough.

## 7. SSH details

**Prompts.** When the backend needs an answer, it sends a `prompt` status and waits for `prompt_respond`. Closing the session cancels it.

```ts
type Prompt =
  | { kind: "host_key_unknown"; host: string; port: number; algorithm: string; fingerprint: string }
  | { kind: "host_key_new_type"; host: string; port: number; algorithm: string; fingerprint: string; known_algorithms: string[] }
  | { kind: "password"; user: string; host: string; offer_save: boolean }
  | { kind: "passphrase"; key_path: string; offer_save: boolean }
  | { kind: "ftps_certificate"; host: string; reason: string; fingerprint: string; subject: string; expires: string }
  | { kind: "plain_ftp"; host: string };
```

**Host keys (SEC1).** Use the `_path` functions in `russh::keys::known_hosts`, with the same `~/.ssh/known_hosts` file OpenSSH uses (`%USERPROFILE%\.ssh\known_hosts` on Windows), so tests can use a temporary file. Check inside `Handler::check_server_key`:

| Result | Action |
|---|---|
| `check_known_hosts_path` returns `Ok(true)` | Connect |
| `Err(Error::KeyChanged { line })` | **Block, with no way to continue.** Return `HostKeyChanged`, with a message giving the host, the new fingerprint, the file and the line, and explaining this could be an attack or a reinstalled server, and that the user removes that line to accept it |
| `Ok(false)`, and `known_host_keys_path` finds **no** keys for this host | `host_key_unknown` prompt. On yes, save with `learn_known_hosts_path` |
| `Ok(false)`, but the host **has** keys of other types | `host_key_new_type` prompt, in warning colors, with Cancel as the default button. Say a different type of key was offered and that this can happen after a server upgrade, but can also be an attack |
| Any other error, such as an unreadable file | Fail the connection with a clear message. Never treat this as "accepted". `russh` reports an unopenable file as "no keys", so read the file first (LIBRARIES.md note 2) |

**The last two rows matter:** `russh` returns `Ok(false)` rather than `KeyChanged` when the new key's type differs from the recorded one. A plain `Ok(false)` check would show an attacker's key as a harmless first connection. Unit-test every row, including this one. Jump hosts go through the same check at each hop.

**Login order (S1).** Use the method saved on the session: `password`, `key` or `agent`.
- **Password:** read the keychain; if absent or unavailable, prompt.
- **Key:** read the file from `key_path` each time, and never copy it. If it's encrypted, get the passphrase from the keychain, otherwise prompt.
- **Agent on Linux:** `AgentClient::connect_env()`. A clear error if `SSH_AUTH_SOCK` isn't set.
- **Agent on Windows:** `AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent")`, then `AgentClient::connect_pageant()` if that fails. A clear error naming both if neither works.
- A failed login returns `AuthFailed` and offers to try again.

**Connection (S5).** Set `keepalive_interval` to 30 seconds and `keepalive_max` to 3. When the connection drops, send a `failed` status with code `connection_lost`. The pane stays open with its output and shows a Reconnect button. There's no automatic reconnect. Each pane opens its own channel on a shared connection per saved session, and the connection closes when its last channel closes. A dropped connection affects only panes using it (Q8).

**Terminal.** Request a PTY with `xterm-256color`.

**`~/.ssh/config` import (S4).** Read `Host`, `HostName`, `Port`, `User`, `IdentityFile` and `ProxyJump` only. Skip patterns containing `*`, `?` or `!`. Show the user what's found before saving, and don't add duplicates. Expand `~` in `IdentityFile`. Follow `Include` one level. Don't change the user's file.

**Jump hosts (S6).** Connect to the jump host, open `channel_open_direct_tcpip` to the target, and use `russh::client::connect_stream` over that channel. Limit chains to 3 hops.

**Port forwarding (S7).** Local and SOCKS (SOCKS5, no-auth, `CONNECT` only) use `channel_open_direct_tcpip`. Remote uses `tcpip_forward`. Local and SOCKS listeners bind to `127.0.0.1` by default. Binding to anything else requires a warning that other machines could connect.

## 8. Docker over SSH

Local: use `Docker::connect_with_local_defaults()`.

Remote uses `Docker::connect_with_custom_transport`, with an HTTP/1 client running over `russh` channels. Try these in order and use the first that works:

1. An exec channel running `docker system dial-stdio`, which is the method the Docker command line uses over SSH.
2. `channel_open_direct_streamlocal("/var/run/docker.sock")`.

If access is denied, explain that the SSH user must be in the `docker` group. Never use `sudo`. Docker exec implements `TerminalBackend`, and uses `resize_exec`.

## 9. Saved data

All files are written safely: write to a temporary file in the same folder, then rename. Keep the previous version as `<name>.bak`. A file that can't be read is never overwritten. The app shows the error and asks the user what to do.

**`sessions.toml`**

```toml
version = 1

[[folder]]
id = "uuid"
name = "Work"
parent = ""            # Empty means top level

[[session]]
id = "uuid"
name = "Build server"
folder = "uuid"        # Empty means top level
type = "ssh"
host = "build.example.com"
port = 22
user = "alex"
auth = "key"           # password, key or agent
key_path = "~/.ssh/id_ed25519"
jump = ""              # ID of another SSH session, or empty

[[session]]
id = "uuid"
name = "Router console"
folder = ""
type = "serial"
port = "COM3"
baud = 115200
data_bits = 8
parity = "none"        # none, odd or even
stop_bits = 1
flow_control = "none"  # none, hardware or software
local_echo = false
line_ending = "cr"     # cr, lf or crlf

[[session]]
id = "uuid"
name = "NAS"
folder = ""
type = "ftp"
host = "nas.local"
port = 21
user = "alex"
security = "explicit"  # plain, explicit or implicit
trusted_cert_sha256 = "" # Set when the user accepts a certificate (SEC8)
```

Passwords and passphrases are never in this file. A test must save and reload a session and confirm that no secret appears in the text.

**`settings.toml`**

```toml
version = 1
font_family = "monospace"
font_size = 14
theme = "dark"
scrollback = 10000     # Minimum 10000
renderer = "auto"      # auto, dom or webgl
webkit_workaround = "off" # off, disable_dmabuf or disable_compositing. Linux only
default_shell = ""
restore_layout = true
```

**`layout.toml`** holds the tabs and pane tree. It stores what each pane was running, but restoring never reconnects by itself (P6).

```ts
type PaneNode =
  | { kind: "leaf"; pane_id: string; spec: SessionSpec | null; title: string }
  | { kind: "split"; direction: "row" | "column"; sizes: number[]; children: PaneNode[] };
```

## 10. Keychain

- Entries use service `SSHToolbox`, and account `session:<session-id>:<kind>`, where kind is `password`, `passphrase` or `ftp_password`.
- Set the store once at startup: `windows-native-keyring-store` on Windows and `zbus-secret-service-keyring-store` on Linux. If that fails, the keychain is unavailable.
- **Timeouts (SEC4):** the keychain can hang as well as fail, and this has been seen on the Ubuntu machine. Run every call on `spawn_blocking`, wrapped in a 3-second timeout. After a timeout or a store failure, treat the keychain as unavailable until the app restarts, and show one quiet notice saying passwords will be asked for each time.
- With the keychain unavailable, nothing else fails. Prompts appear instead, and "save password" is hidden. **Never fall back to writing secrets to a file.**
- Deleting a session deletes its secrets.

## 11. Terminal rendering

| Setting | Behavior |
|---|---|
| `auto` on Linux | DOM (T8) |
| `auto` on Windows | WebGL. Falls back to DOM if the addon fails to load, or on `onContextLoss` |
| `dom`, `webgl` | As chosen, with a notice if WebGL fails |

Tauri's docs warn that WebGL creation can succeed on Linux even when it's slow, so the app can't detect this itself. That's why the setting exists.

**xterm options:** `scrollback` from settings, `allowProposedApi: true` (for Unicode 11), `unicode.activeVersion = "11"` after loading the addon. On Windows, set `windowsPty: { backend: "conpty", buildNumber }` from `platform_info`.

**`webkit_workaround`** is off by default. Tauri's docs warn against turning these on for everyone. If set on Linux, set the matching variable at the very start of `main`, before any threads or the webview exist: `WEBKIT_DISABLE_DMABUF_RENDERER=1` or `WEBKIT_DISABLE_COMPOSITING_MODE=1`. It takes effect after a restart.

## 12. Keyboard shortcuts (P3)

| Shortcut | Action |
|---|---|
| Ctrl+Shift+C / Ctrl+Shift+V | Copy / paste |
| Ctrl+Shift+T | New tab |
| Ctrl+Shift+W | Close pane, and the tab if it's the last one |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab |
| Ctrl+Shift+E | Split side by side |
| Ctrl+Shift+O | Split top and bottom |
| Ctrl+Shift+Arrow | Move between panes |
| Ctrl+Shift+F | Search scrollback |
| Ctrl+Shift+B | Type into all panes on/off |

Intercept these with xterm's `attachCustomKeyEventHandler` so they don't reach the program in the pane. All other keys reach the program.

**Paste (T2):** if the text contains a line break, ask first and show the first few lines. Then use `term.paste()`, which handles bracketed paste and line endings.

## 13. Links (SEC9)

Use `@xterm/addon-web-links` with a custom handler. A link opens only on Ctrl+click. It goes to the `open_link` command, and the backend checks the scheme is exactly `http` or `https` before calling the opener plugin from Rust. The interface doesn't get the opener plugin's permission. Unit-test that `file:`, `javascript:` and custom schemes are rejected.

## 14. Tauri security settings (SEC10)

In `tauri.conf.json`:

```json
"security": {
  "csp": "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src ipc: http://ipc.localhost",
  "dangerousDisableAssetCspModification": ["style-src"]
}
```

xterm adds inline styles, and Tauri normally adds a nonce to `style-src`, which makes browsers ignore `'unsafe-inline'`. That's the only reason for the setting, which applies to `style-src` only. Never apply it to scripts. Keep `withGlobalTauri` false.

The capability file grants only `core:default`, plus individual dialog permissions when needed. Never add shell or filesystem plugin permissions: all file and process work goes through our own Rust commands. Developer tools stay off in release builds, which is Tauri's default.

## 15. Key setup (K1–K7)

The steps run in this order, and stop at the first failure with a clear message:

1. **Choose a key (K1, K2).** Either an existing key, or generate ed25519 with `russh::keys`. A new key goes in `~/.ssh/` as `id_ed25519_sshtoolbox`, then `_2`, `_3` if taken. Open with create-new mode, which fails instead of overwriting. Restrict the private key file to the user: mode `600` on Linux, and on Windows the same `icacls` pattern as below, applied locally.
2. **Log in with the password** over a normal connection, including all host key checks.
3. **Detect the server OS.** Run `uname -s`. If it succeeds, the server is Unix-like. Otherwise run the PowerShell check below. If that also fails, stop and explain.
4. **Install the public key** with the matching script. Send only `<type> <base64>`, plus a comment of `sshtoolbox`. Check the key text against `^ssh-ed25519 [A-Za-z0-9+/=]+$` (or the existing key's type) first.
5. **Test (K6).** Open a separate new connection, using only the key. Report success only if that login works.
6. **Offer to delete the saved password.** Never delete it without asking.

**Linux and macOS (K4).** Run this on an exec channel, sending the key on standard input, so it's never in a shell command:

```sh
umask 077; key=$(cat); blob=$(printf '%s' "$key" | cut -d' ' -f1,2)
mkdir -p ~/.ssh && chmod 700 ~/.ssh && touch ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys || exit 1
grep -qF -- "$blob" ~/.ssh/authorized_keys && { echo already-present; exit 0; }
if [ -s ~/.ssh/authorized_keys ] && [ -n "$(tail -c1 ~/.ssh/authorized_keys)" ]; then echo >> ~/.ssh/authorized_keys; fi
printf '%s\n' "$key" >> ~/.ssh/authorized_keys && echo installed
```

The `tail` line avoids joining the new key onto a last line with no line ending. Integration-test: a fresh user, an existing file, a file with no final line ending, and running twice (no duplicate).

**Windows (K5).** Run with `powershell -NoProfile -NonInteractive -EncodedCommand <base64 of UTF-16LE script>`, which works whether the server's default shell is cmd or PowerShell. Put the validated key directly into the script.

Detect: `[Environment]::OSVersion.Platform` prints `Win32NT`.

Administrator check, by membership and not by the current token: `(whoami /groups) -match 'S-1-5-32-544'`. This matches how `sshd` picks the file.

```powershell
$key = '<validated key>'
$blob = ($key -split ' ')[0..1] -join ' '
$isAdmin = [bool]((whoami /groups) -match 'S-1-5-32-544')
if ($isAdmin) {
  $file = Join-Path $env:ProgramData 'ssh\administrators_authorized_keys'
} else {
  $dir = Join-Path $env:USERPROFILE '.ssh'
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  $file = Join-Path $dir 'authorized_keys'
}
if ((Test-Path $file) -and (Select-String -Path $file -SimpleMatch -Quiet -Pattern $blob)) { 'already-present'; exit 0 }
Add-Content -Force -Path $file -Encoding ascii -Value $key
if ($isAdmin) {
  icacls.exe $file /inheritance:r /grant '*S-1-5-32-544:F' /grant '*S-1-5-18:F' | Out-Null
} else {
  $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
  icacls.exe $file /inheritance:r /grant "*${sid}:F" /grant '*S-1-5-18:F' /grant '*S-1-5-32-544:F' | Out-Null
}
if ($LASTEXITCODE -ne 0) { 'acl-failed'; exit 1 }
'installed'
```

The paths and the administrator file permissions follow [Microsoft's guide](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_keymanagement). Group SIDs are used because group names change on non-English Windows. If writing the administrator file fails, explain that it needs an elevated session. This can't be tested in CI, so it's on the owner's list, and it's part of the security review.
</content>
