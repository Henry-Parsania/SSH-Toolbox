# SSH Toolbox: Requirements

A lightweight, personal desktop app in the style of MobaXterm. "SSH Toolbox" is a working title.

**Priorities:** **Must** = needed for that phase to be finished. **Should** = build it in that phase unless it turns out to be disproportionately hard. **Could** = only if it's cheap, otherwise later.

## 1. Scope

| Item | Decision |
|---|---|
| **User** | One person, on their own machines |
| **Required platforms** | Windows 11 and Ubuntu 26.04 (amd64) |
| **Optional platform** | macOS, later. Don't make choices that rule it out, but don't test on it yet |
| **Interface language** | English |
| **Distribution** | Personal builds. No code signing, app store or auto-update |

### Out of scope
- X11 server, RDP and VNC
- Team features, cloud sync and accounts
- AI features and a plugin system
- Telemetry of any kind

## 2. Quality targets

These are checked on Windows first and on the Ubuntu machine later, using the methods in [TEST_CHECKLIST.md](TEST_CHECKLIST.md). The numbers are targets to measure against, and we can adjust them once we have real measurements.

| ID | Requirement | Target | Priority |
|---|---|---|---|
| Q1 | Download size | Under 30 MB | Must |
| Q2 | Memory with one terminal open, including the webview | Under 250 MB | Should |
| Q3 | Time from launch to a usable window | Under 2 seconds | Should |
| Q4 | Typing | No noticeable lag | Must |
| Q5 | Heavy output: `seq 1 1000000` | Finishes without freezing the app; other panes stay usable | Must |
| Q6 | Full-screen programs: `htop`, `vim` | No flicker or visual errors | Must |
| Q7 | Window resizing | No blank window or crash | Must |
| Q8 | A failed connection or dropped session | Shows a clear message, and only that tab is affected | Must |

## 3. Security

| ID | Requirement | Priority |
|---|---|---|
| SEC1 | Check SSH host keys against `known_hosts`. Ask on first connection, and block with a clear warning if a key changes | Must |
| SEC2 | Never write passwords or passphrases to session files, logs or plain text fallback files | Must |
| SEC3 | Keep saved secrets in the OS keychain (Windows Credential Manager or Secret Service on Linux) | Must |
| SEC4 | If the keychain fails or is missing, keep running and ask for the password each time. This has been seen in practice: `gnome-keyring-daemon` crashed on the Ubuntu test machine | Must |
| SEC5 | Refer to private keys by file path, and never copy them into app data | Must |
| SEC6 | Warn before connecting with plain, unencrypted FTP | Must |
| SEC7 | Run a security review of the code, by a stronger model than the builder, before first use on real servers. Until then, test only against throwaway servers | Must |
| SEC8 | Verify FTPS certificates. If verification fails, show the reason and fingerprint and connect only if the user accepts. Save the accepted fingerprint with the session, and block with a clear warning if it later changes | Must |
| SEC9 | Open terminal links only on Ctrl+click, and only `http` and `https` links. The backend enforces this | Must |
| SEC10 | Use a strict content security policy and minimal Tauri permissions, as set out in INTERFACES.md section 14 | Must |
| SEC11 | Treat remote names as untrusted. A folder download can't write outside its chosen destination, and remote text is never rendered as HTML | Must |

## 4. Terminal (phase 1)

| ID | Requirement | Priority |
|---|---|---|
| T1 | Local shell: default shell on Linux; PowerShell, cmd and Git Bash if present on Windows | Must |
| T2 | Copy and paste (Ctrl+Shift+C/V), with a warning before pasting multiple lines | Must |
| T3 | Scrollback of at least 10,000 lines | Must |
| T4 | Correct Unicode, colors and terminal resizing | Must |
| T5 | Search within scrollback | Should |
| T6 | Clickable links | Should |
| T7 | Font, size and color theme settings | Should |
| T8 | Choice of DOM or WebGL renderer. Linux uses DOM by default, and Windows uses WebGL by default | Must |
| T9 | Save session output to a file | Could |

## 5. Tabs and panes (phase 1)

| ID | Requirement | Priority |
|---|---|---|
| P1 | Multiple tabs: new, close, rename and reorder | Must |
| P2 | Split panes horizontally and vertically, and resize them by dragging | Must |
| P3 | Keyboard shortcuts for splitting and moving between panes | Must |
| P4 | Any pane can hold any session type. Phase 1 delivers local and WSL, and its pane design must accept SSH, serial and Docker later without changes. Those types arrive in their own phases | Must |
| P5 | Type into all panes at once | Should |
| P6 | Reopen the previous layout on launch, and reconnect only when asked. Phase 1 covers local and WSL panes, and phase 2 adds saved SSH sessions | Should |
| P7 | Move a tab into its own window | Could |

Local panes stop when the app closes. Unlike tmux, the app doesn't keep them running in the background. Running tmux on a remote server still works normally.

## 6. WSL (phase 1, except W4 in phase 3; Windows only)

| ID | Requirement | Priority |
|---|---|---|
| W1 | List installed distros, leaving out Docker Desktop's internal ones | Must |
| W2 | Open a shell in a chosen distro | Must |
| W3 | Hide all WSL options on Linux and macOS | Must |
| W4 | Browse a distro's files in the file browser. Phase 3, since that's when the file browser arrives | Should |

## 7. SSH and saved sessions (phase 2)

| ID | Requirement | Priority |
|---|---|---|
| S1 | Log in with a password, private key (including passphrase-protected) or SSH agent | Must |
| S2 | Save sessions with name, host, port, user and login method | Must |
| S3 | Sidebar of sessions, with folders and search | Must |
| S4 | Import hosts from `~/.ssh/config` | Should |
| S5 | Keepalive, plus a reconnect button after a dropped connection | Must |
| S6 | Jump hosts (ProxyJump) | Should |
| S7 | Port forwarding: local, remote and SOCKS | Should |

Sessions are stored as a readable TOML file in the standard per-user config folder, so they're easy to back up.

## 8. Easy key setup (phase 2)

A single "Set up passwordless login" action on any SSH session.

| ID | Requirement | Priority |
|---|---|---|
| K1 | Generate an ed25519 key with an optional passphrase, or pick an existing key | Must |
| K2 | Never overwrite an existing key file | Must |
| K3 | Log in once with the password and install the public key, without adding duplicates | Must |
| K4 | Linux/macOS servers: create `~/.ssh` and `authorized_keys` if needed, and set correct permissions | Must |
| K5 | Windows servers: detect the OS, and use `administrators_authorized_keys` for admin accounts, with correct ACLs | Must |
| K6 | Test the key login before reporting success, then offer to delete the saved password | Must |
| K7 | Works the same whether the app runs on Windows or Linux | Must |

## 9. File transfer (phase 3)

| ID | Requirement | Priority |
|---|---|---|
| F1 | File browser panel beside an SSH session, using the same connection and no second login | Must |
| F2 | List, upload, download, rename, create folder and delete, with confirmation before deleting | Must |
| F3 | Drag and drop between the desktop and the panel | Should |
| F4 | Transfer queue with progress and cancel, and a prompt before overwriting | Must |
| F5 | Standalone FTP and FTPS sessions in the same browser (passive mode, explicit and implicit TLS) | Must |
| F6 | Change permissions over SFTP | Should |
| F7 | Panel follows the terminal's current folder, where the remote shell reports it | Should |
| F8 | Open a remote file in a local editor and upload it again on save | Could |

## 10. Serial (phase 4)

| ID | Requirement | Priority |
|---|---|---|
| R1 | List ports, with a refresh button | Must |
| R2 | Set baud rate, data bits, parity, stop bits and flow control | Must |
| R3 | Save serial setups as sessions | Must |
| R4 | On Linux, explain a permission error and show the `dialout` fix | Must |
| R5 | Show clearly when a device is unplugged, and offer to reconnect | Must |
| R6 | Local echo and line ending options | Should |
| R7 | Send break and toggle DTR/RTS | Should |

## 11. Docker (phase 5)

| ID | Requirement | Priority |
|---|---|---|
| D1 | Connect to local Docker: Unix socket on Linux, named pipe on Windows | Must |
| D2 | Connect to Docker on a saved SSH session, with nothing to install on the server | Must |
| D3 | List containers, then start, stop, restart and remove them, with confirmation before removing | Must |
| D4 | Live logs with follow and search | Must |
| D5 | Show config: environment, ports, mounts, networks and raw inspect output | Must |
| D6 | Open a shell in a container, as a normal pane | Must |
| D7 | Group containers by Compose project | Should |
| D8 | Live CPU and memory stats | Should |
| D9 | List and remove images, volumes and networks | Could |

Remote Docker needs the SSH user to have access to the Docker socket. The app explains this when access is denied and doesn't try to use `sudo`.
