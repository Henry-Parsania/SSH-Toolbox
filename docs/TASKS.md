# SSH Toolbox: Task List

This is what's done and what's left. Treat it as a guide: reorder, combine or rewrite tasks when that makes sense, and tick them off as they're finished. Requirement IDs such as T1 are in [REQUIREMENTS.md](REQUIREMENTS.md). **IF** means [INTERFACES.md](INTERFACES.md).

If you can't see or operate the app window, don't claim a screen works. Mention it to the owner to try in the next build.

## Phase 0: Setup

- [x] Finished: tools, git, CI, skeleton, security settings and library checks. Results are in [LIBRARIES.md](LIBRARIES.md).

## Phase 1: Terminal, tabs and panes

- [x] **1.1 Output pump.** Build `TerminalBackend`, `BackendEvent`, `OutputSink` and the pump (IF sections 3 and 4), with a session registry holding open sessions by ID. Include all the pump's unit tests.
  *Done when:* tests pass using a fake backend and sink.

- [x] **1.2 Local shell backend (T1).** IF section 3. Test with `echo`, resize and exit code.
  *Done when:* a test starts a real shell on both OSes in CI and reads its output.

- [x] **1.3 Terminal commands.** Add the phase 1 terminal commands (IF section 5), `list_shells`, and their `src/lib/backend/` wrappers.
  *Done when:* it compiles and a temporary button can list shells.

- [x] **1.4 One terminal in the window (T3, T4, T8).** Add `src/lib/terminal/`, including xterm, fit, Unicode 11, the renderer choice (IF section 11) and flow control (IF section 4). Temporarily fill the window with one local shell.
  *Done when:* the app builds and opens to a shell. Typing, `seq 1 1000000` and `yes` with Ctrl+C are confirmed in task 1.5.

- [x] **1.5 Owner tries the terminal.** Make a build and ask the owner to try the rendering section of the checklist, on Windows and on Ubuntu. Ubuntu matters most: see the first checkpoint in PLAN.md section 2.
  *Done when:* the owner reports how it went.

- [x] **1.6 Clipboard (T2).** Copy and paste, with the multi-line warning (IF section 12).
  *Done when:* tests cover the "should we warn" logic, and manual checks work.

- [x] **1.7 Pane tree (P2).** Build the `PaneNode` tree as plain TypeScript with no UI: split, close, resize, and finding the next pane in a direction. Unit-test thoroughly, including closing the last child of a split.
  *Done when:* tests pass.

- [x] **1.8 Panes on screen (P2, P4).** Draw the tree with draggable dividers. Each pane takes a `SessionSpec`, so later session types need no pane changes. Closing a pane closes its session. A pane whose session ended shows a message and a close button, and only that pane is affected.
  *Done when:* panes split in both directions, resize by dragging, and each runs its own shell.

- [x] **1.9 Tabs (P1).** New, close, rename and drag to reorder. Hidden tabs keep their terminals alive (IF section 4).
  *Done when:* switching tabs keeps scrollback and running programs.

- [x] **1.10 Shortcuts (P3).** IF section 12.
  *Done when:* each works, and `vim` still gets ordinary keys such as Ctrl+W.

- [x] **1.11 WSL (W1–W3).** IF section 3. Add `list_wsl_distros` and a menu, hidden when `wsl_available` is false.
  *Done when:* a test decodes a captured UTF-16LE sample and removes Docker's distros. On this PC, `Ubuntu` is listed and opens, and `docker-desktop` isn't listed.

- [x] **1.12 Settings (T7, T8).** `settings.toml` and a simple screen for font, size, theme, scrollback and renderer. Add `webkit_workaround` too (IF section 11).
  *Done when:* changes save and survive a restart.

- [x] **1.13 Search and links (T5, T6, SEC9).** Add search, and links per IF section 13, using `tauri-plugin-opener` from Rust.
  *Done when:* the unsafe-scheme tests pass, and Ctrl+click opens an `https` link.

- [x] **1.14 Restore layout (P6).** `layout.toml`, restoring tabs and splits. Local and WSL panes start fresh. Reconnecting other types arrives with their phases.
  *Done when:* layout returns after restart.

- [x] **1.15 Should and Could items.** Type into all panes (P5). Save session output (T9), only if cheap. Skip moving tabs to their own window (P7), and record that on the checklist as "later".
  *Done when:* P5 works, and the T9 and P7 decisions are recorded.

- [x] **1.16 Measurements.** Add `scripts/measure-memory.sh` and `scripts/measure-memory.ps1` for Q2 (see the checklist), and log startup time for Q3.
  *Done when:* numbers are recorded in the checklist.

- [ ] **1.17 Finish.** Make sure the Ubuntu rendering check has happened (PLAN.md section 2) before phase 2 starts.

## Phase 2: SSH and key setup

Build and test against the local test container. This phase also adds SSH to `SessionSpec`.

- [x] **2.1 Saved sessions (S2).** `sessions.toml` per IF section 9. Test save and reload, a corrupt file (not overwritten) and that no secret appears in the file.
- [x] **2.2 Keychain (SEC3, SEC4).** IF section 10. Test with a fake store for: normal use, failure, and a call that hangs past the timeout.
- [x] **2.3 Connection and host keys (SEC1).** IF section 7. Add the prompt mechanism. Test every row of the host key table. Read `known_hosts` yourself first: see note 2 in LIBRARIES.md.
- [x] **2.4 Login (S1, SEC2, SEC5).** Password, key, encrypted key and agent. Integration-test each against the container.
- [x] **2.5 SSH in a pane (S5, P4, Q8).** `TerminalBackend` for SSH, keepalive and the Reconnect button. Integration-test killing the container mid-session: only that pane shows the failure.
- [x] **2.6 Sidebar (S3).** Folders, search, and a session editor. Add the "save password" option, hidden when the keychain is unavailable.
- [x] **2.7 Layout restore for SSH (P6).** Restored SSH panes show a Connect button, and don't connect by themselves.
- [x] **2.8 Key generation (K1, K2).** IF section 15, step 1. Test that an existing file is never overwritten.
- [x] **2.9 Key install on Linux (K3, K4, K6).** The full flow, including all the integration tests in IF section 15.
- [x] **2.10 Key install on Windows servers (K5, K7).** The script in IF section 15. Unit-test the generated script and its encoding. Real testing is on the owner's list.
- [x] **2.11 Import and jump hosts (S4, S6).** IF section 7. Test parsing with sample config files. Integration-test a jump host with two containers.
- [x] **2.12 Port forwarding (S7).** IF section 7. Integration-test local, remote and SOCKS.
- [x] **2.13 Security review (SEC7).** Run the review in PLAN.md section 5 and fix what's serious.
- [ ] **2.14 Owner tries it** on real servers.

## Phase 3: File transfer

- [x] **3.1 File interface and local backend.** IF section 6. Test on a temporary folder. Include the remote name checks (SEC11).
- [x] **3.2 Transfer queue (F4).** Queue, progress, cancel, safe writes and overwrite prompts, tested with fake backends. Test that a cancelled transfer leaves the old file untouched.
- [x] **3.3 SFTP (F1, F2, F6).** On the existing connection. Integration-test against the container, including that no second login happens.
- [x] **3.4 File browser (F2, F4).** Panel beside the session, with the transfer queue and delete confirmation.
- [x] **3.5 Drag and drop (F3).** Between the desktop and the panel. Use Tauri's drag-drop event for files coming in. Dragging out to the desktop may not be possible. If it isn't, leave that direction out and tell the owner.
- [x] **3.6 WSL files (W4).** Browse a distro using the local backend.
- [x] **3.7 FTP (F5, SEC6).** Plain FTP with the warning before every connection. Add a `vsftpd` test container.
- [x] **3.8 FTPS (F5, SEC8).** Explicit and implicit TLS, and the certificate rules in REQUIREMENTS.md. Test with the `vsftpd` container's self-signed certificate: it asks, then connects once accepted.
- [x] **3.10 Follow the terminal's folder (F7).** Use xterm's `parser.registerOscHandler(7, ...)`. Treat the value as untrusted, and have it change only what the panel shows. Skip opening files in a local editor (F8), and record that as "later".
- [ ] **3.11 Owner tries it.**

## Phase 4: Serial

- [x] **4.1 Port listing (R1).** With a refresh button. Linux adds USB names through `libudev`.
- [x] **4.2 Serial backend (R2, R5, P4).** `TerminalBackend`, where `resize` does nothing. A read error or end of stream becomes a `failed` status with a reconnect offer. Test on Linux CI with virtual ports from `socat`.
- [x] **4.3 Saved setups (R3).** Add serial sessions to the editor, with the fields in IF section 9.
- [x] **4.4 Permission help (R4).** On Linux, a permission error explains this fix: `sudo usermod -aG dialout $USER`, then log out and back in. The app never runs it itself.
- [x] **4.5 Should items (R6, R7).** Local echo, line endings, send break, and DTR and RTS toggles.
- [ ] **4.6 Owner tries it** with a real device.

## Phase 5: Docker

- [x] **5.1 Prove Docker over SSH first (D2).** IF section 8. Before any Docker UI, build this and test it in Linux CI, using the SSH test container with the Docker command line installed and the host's Docker socket mounted. If neither method works, tell the owner the options.
- [x] **5.2 Local Docker (D1).** Add a connection screen with a clear message when Docker isn't running.
- [x] **5.3 Containers (D3, D7).** List, group by Compose project, start, stop, restart, and remove with confirmation.
- [x] **5.4 Logs (D4).** Live, with follow and search. Uses the same flow control as the terminal.
- [x] **5.5 Details (D5).** Environment, ports, mounts, networks and raw inspect output. Display it as plain text only.
- [x] **5.6 Shell in a container (D6).** Implements `TerminalBackend`, as a normal pane. Prefer `bash`, falling back to `sh`.
- [ ] **5.7 Should and Could items (D8, D9).** Live stats. Images, volumes and networks only if cheap. Add the "permission denied" explanation for remote Docker.
- [ ] **5.9 Owner tries it.**

## Phase 6: Polish

- [ ] **6.1 Leftovers.** Finish the settings screen and themes, and any Should items that were put off. List anything left, for the owner to decide.
- [ ] **6.2 Installers.** `.deb` and AppImage for Linux, and `.exe` for Windows, with an app icon. Measure Q1 on each.
- [ ] **6.3 Quick security pass.** Run `/security-review` over what was added since task 2.13, and fix what's serious.
- [ ] **6.4 Ubuntu pass.** Owner tries the Ubuntu items on the checklist, including keychain behavior when the service is stopped. Fix what's found.
- [ ] **6.5 Finish.** Record the quality target measurements.
</content>
