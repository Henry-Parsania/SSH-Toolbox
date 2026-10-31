# SSH Toolbox: Task List

Do these in order, following the rules in [PLAN.md](PLAN.md) section 6. Each task lists what to build and when it's done. Exact details are in [INTERFACES.md](INTERFACES.md), referred to here as **IF**.

Every task also requires the automated checks in PLAN.md section 3 to pass, and a commit and push. Each requirement ID, such as T1, is in [REQUIREMENTS.md](REQUIREMENTS.md).

Mark tasks finished by changing `[ ]` to `[x]`.

Tasks from 2.1 onward are shorter, because IF holds their detail. Each is done when what it describes works and its named tests pass.

The builder may not be able to see or operate the app window. If a "done when" needs that, don't claim it passed. Add it to a short list for the owner, and ask them to check that list at the next owner check.

## Phase 0: Setup

- [x] **0.1 Install tools.** Run these one at a time, because Windows installers can block each other:
  ```
  winget install --id OpenJS.NodeJS.LTS --exact --source winget --silent --accept-package-agreements --accept-source-agreements
  winget install --id Rustlang.Rustup --exact --source winget --silent --accept-package-agreements --accept-source-agreements
  winget install --id Microsoft.VisualStudio.2022.BuildTools --exact --source winget --accept-package-agreements --accept-source-agreements --override "--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
  rustup default stable-msvc
  ```
  The owner has approved these. The build tools take a while and show a Windows administrator prompt, so tell the owner to expect it. Open a new shell afterward, so the new `PATH` applies.
  *Done when:* `node -v`, `npm -v`, `rustc -V` and `cargo -V` print versions, and `rustup show` lists `stable-x86_64-pc-windows-msvc` as default.

- [x] **0.2 Create the git repository.** In the project folder run `git init -b main`. Add a `.gitignore` for `node_modules/`, `dist/`, `target/`, `*.log` and `.env`. Add the remote `https://github.com/Henry-Parsania/SSH-Toolbox.git`, commit the docs and `CLAUDE.md`, and push. Don't commit `.claude/` local settings.
  *Done when:* the docs are visible on GitHub.

- [x] **0.3 Create the skeleton.**
  1. Create the Svelte project in a temporary folder with `npm create vite@latest <temp> -- --template svelte-ts`, then copy it into the project root. Don't use `create-tauri-app`, whose Svelte template may bring SvelteKit.
  2. `npm install`, then `npm install -D @tauri-apps/cli` and `npm install @tauri-apps/api`.
  3. `npx tauri init` with app name `SSH Toolbox`, window title `SSH Toolbox`, frontend dist `../dist`, dev URL `http://localhost:5173`, before-dev command `npm run dev` and before-build command `npm run build`.
  4. Set the identifier from IF section 1. Add a root `Cargo.toml` workspace with members `src-tauri` and `crates/core`. Create `crates/core` as `toolbox-core`, with no Tauri dependency. Add `npm test` using Vitest.
  5. Use the folder layout in PLAN.md section 1. Commit `Cargo.lock` and `package-lock.json`.
  *Done when:* `npx tauri dev` opens a window on this PC.

- [x] **0.4 Security settings (SEC10).** Apply IF section 14. Don't add any plugin yet.
  *Done when:* the window still opens, with no CSP errors in the dev console.

- [x] **0.5 Connect the layers.** Add `AppError` and `Secret` (IF section 2) with tests, the `platform_info` command, and `src/lib/backend/index.ts`. Show the OS name in the window, proving the path from interface to `crates/core`. Add `scripts/check-boundaries.mjs`, which fails if a file outside `src/lib/backend/` imports `@tauri-apps/*`, and add it to `npm run check`.
  *Done when:* the window shows the OS. Adding such an import temporarily makes the check fail.

- [x] **0.6 CI.** Add `.github/workflows/ci.yml`, running on pushes to `main`, with `windows-latest` and `ubuntu-26.04`. Run every check from PLAN.md section 3, then `npx tauri build`, and upload the Windows installer and Linux `.deb` and AppImage as artifacts. On Linux, first install:
  ```
  sudo apt-get update
  sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libudev-dev
  ```
  Build on 26.04 so the build matches the owner's Ubuntu. If that build machine is unavailable, run the Linux job on `ubuntu-latest` with `container: ubuntu:26.04`, and tell the owner.
  *Done when:* both jobs are green and the artifacts can be downloaded.

- [x] **0.7 Check the libraries.** Add small programs under `crates/core/examples/`, then record results in a new `docs/LIBRARIES.md`, with one row per library: version, what was tried, on which OS, and the result. Add a Dockerfile in `tests/containers/sshd/` based on `debian:stable-slim` with `openssh-server`. In CI on Linux, start that container, and start `ssh-agent` with a throwaway key.

  | Library | Check | Where |
  |---|---|---|
  | `portable-pty` | Start a shell, run `echo`, read the output, resize | Windows and Linux CI |
  | `russh` with `ring` | Log in with a password, a key, and an agent, to the test container | Linux CI. Windows too, if Docker Desktop is running |
  | `russh` Windows agent | Connect to `\\.\pipe\openssh-ssh-agent` and list keys | Windows, if the owner has turned on `ssh-agent`. If not, record "untested" and it stays on the phase 2 checklist |
  | `russh` host keys | Confirm the behaviors in IF section 7's table, using a temporary file | Both |
  | `russh-sftp` | List a folder on the same connection | Linux CI |
  | `suppaftp` | Builds with `tokio-rustls-ring` and `deprecated` | Both |
  | `tokio-serial` | Build and list ports | Both |
  | `bollard` | Ping and list containers | Whichever has Docker. Linux CI has it |
  | `keyring-core` and stores | On Windows: set, read and delete a test entry named `SSHToolbox-test`, then confirm it's gone. On Linux: builds only | Both |
  | Tauri raw channel | Send bytes with `InvokeResponseBody::Raw` and confirm the interface gets an `ArrayBuffer` | Windows |

  *Done when:* every row has a result. If a check fails, follow the stop rules.

- [x] **0.8 Finish.** Ask the owner to run the phase 0 Windows item in [TEST_CHECKLIST.md](TEST_CHECKLIST.md). Update `CLAUDE.md`'s status.
  *Done when:* the owner confirms the window opens on Windows.

## Phase 1: Terminal, tabs and panes

- [x] **1.1 Output pump.** Build `TerminalBackend`, `BackendEvent`, `OutputSink` and the pump (IF sections 3 and 4), with a session registry holding open sessions by ID. Include all the pump's unit tests.
  *Done when:* tests pass using a fake backend and sink.

- [ ] **1.2 Local shell backend (T1).** IF section 3. Test with `echo`, resize and exit code.
  *Done when:* a test starts a real shell on both OSes in CI and reads its output.

- [ ] **1.3 Terminal commands.** Add the phase 1 terminal commands (IF section 5), `list_shells`, and their `src/lib/backend/` wrappers.
  *Done when:* it compiles and a temporary button can list shells.

- [ ] **1.4 One terminal in the window (T3, T4, T8).** Add `src/lib/terminal/`, including xterm, fit, Unicode 11, the renderer choice (IF section 11) and flow control (IF section 4). Temporarily fill the window with one local shell.
  *Done when:* the app builds and opens to a shell. Typing, `seq 1 1000000` and `yes` with Ctrl+C are confirmed in task 1.5.

- [ ] **1.5 Early owner check.** Ask the owner to run the rendering section of the checklist on Windows. Fix what's found. Mention that Ubuntu can be tried from this point with the CI build.
  *Done when:* the owner reports it works on Windows.

- [ ] **1.6 Clipboard (T2).** Copy and paste, with the multi-line warning (IF section 12).
  *Done when:* tests cover the "should we warn" logic, and manual checks work.

- [ ] **1.7 Pane tree (P2).** Build the `PaneNode` tree as plain TypeScript with no UI: split, close, resize, and finding the next pane in a direction. Unit-test thoroughly, including closing the last child of a split.
  *Done when:* tests pass.

- [ ] **1.8 Panes on screen (P2, P4).** Draw the tree with draggable dividers. Each pane takes a `SessionSpec`, so later session types need no pane changes. Closing a pane closes its session. A pane whose session ended shows a message and a close button, and only that pane is affected.
  *Done when:* panes split in both directions, resize by dragging, and each runs its own shell.

- [ ] **1.9 Tabs (P1).** New, close, rename and drag to reorder. Hidden tabs keep their terminals alive (IF section 4).
  *Done when:* switching tabs keeps scrollback and running programs.

- [ ] **1.10 Shortcuts (P3).** IF section 12.
  *Done when:* each works, and `vim` still gets ordinary keys such as Ctrl+W.

- [ ] **1.11 WSL (W1ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã…â€œW3).** IF section 3. Add `list_wsl_distros` and a menu, hidden when `wsl_available` is false.
  *Done when:* a test decodes a captured UTF-16LE sample and removes Docker's distros. On this PC, `Ubuntu` is listed and opens, and `docker-desktop` isn't listed.

- [ ] **1.12 Settings (T7, T8).** `settings.toml` and a simple screen for font, size, theme, scrollback and renderer. Add `webkit_workaround` too (IF section 11).
  *Done when:* changes save and survive a restart.

- [ ] **1.13 Search and links (T5, T6, SEC9).** Add search, and links per IF section 13, using `tauri-plugin-opener` from Rust.
  *Done when:* the unsafe-scheme tests pass, and Ctrl+click opens an `https` link.

- [ ] **1.14 Restore layout (P6).** `layout.toml`, restoring tabs and splits. Local and WSL panes start fresh. Reconnecting other types arrives with their phases.
  *Done when:* layout returns after restart.

- [ ] **1.15 Should and Could items.** Type into all panes (P5). Save session output (T9), only if cheap. Skip moving tabs to their own window (P7), and record that on the checklist as "later".
  *Done when:* P5 works, and the T9 and P7 decisions are recorded.

- [ ] **1.16 Measurements.** Add `scripts/measure-memory.sh` and `scripts/measure-memory.ps1` for Q2 (see the checklist), and log startup time for Q3.
  *Done when:* numbers are recorded in the checklist.

- [ ] **1.17 Finish.** Owner runs the phase 1 Windows list, then the Ubuntu rendering decision (PLAN.md section 2). Update `CLAUDE.md`.
  *Done when:* both are reported.

## Phase 2: SSH and key setup

**Until task 2.13 is finished, connect only to the local test container.** This phase also adds SSH to `SessionSpec`, and the owner's real-server tests come after the review.

- [ ] **2.1 Saved sessions (S2).** `sessions.toml` per IF section 9. Test save and reload, a corrupt file (not overwritten) and that no secret appears in the file.
- [ ] **2.2 Keychain (SEC3, SEC4).** IF section 10. Test with a fake store for: normal use, failure, and a call that hangs past the timeout.
- [ ] **2.3 Connection and host keys (SEC1).** IF section 7. Add the prompt mechanism. Test every row of the host key table.
- [ ] **2.4 Login (S1, SEC2, SEC5).** Password, key, encrypted key and agent. Integration-test each against the container. Add a test that logs from a failed login contain no password.
- [ ] **2.5 SSH in a pane (S5, P4, Q8).** `TerminalBackend` for SSH, keepalive and the Reconnect button. Integration-test killing the container mid-session: only that pane shows the failure.
- [ ] **2.6 Sidebar (S3).** Folders, search, and a session editor. Add the "save password" option, hidden when the keychain is unavailable.
- [ ] **2.7 Layout restore for SSH (P6).** Restored SSH panes show a Connect button, and don't connect by themselves.
- [ ] **2.8 Key generation (K1, K2).** IF section 15, step 1. Test that an existing file is never overwritten.
- [ ] **2.9 Key install on Linux (K3, K4, K6).** The full flow, including all the integration tests in IF section 15.
- [ ] **2.10 Key install on Windows servers (K5, K7).** The script in IF section 15. Unit-test the generated script and its encoding. Real testing is on the owner's list.
- [ ] **2.11 Import and jump hosts (S4, S6).** IF section 7. Test parsing with sample config files. Integration-test a jump host with two containers.
- [ ] **2.12 Port forwarding (S7).** IF section 7. Integration-test local, remote and SOCKS.
- [ ] **2.13 Security review (SEC7).** Stop. Tell the owner a stronger model needs to review. Fix everything rated high or medium before continuing.
- [ ] **2.14 Finish.** Owner runs the phase 2 Windows list, against real servers. Update `CLAUDE.md`.

## Phase 3: File transfer

- [ ] **3.1 File interface and local backend.** IF section 6. Test on a temporary folder. Include the remote name checks (SEC11).
- [ ] **3.2 Transfer queue (F4).** Queue, progress, cancel, safe writes and overwrite prompts, tested with fake backends. Test that a cancelled transfer leaves the old file untouched.
- [ ] **3.3 SFTP (F1, F2, F6).** On the existing connection. Integration-test against the container, including that no second login happens.
- [ ] **3.4 File browser (F2, F4).** Panel beside the session, with the transfer queue and delete confirmation.
- [ ] **3.5 Drag and drop (F3).** Between the desktop and the panel. Use Tauri's drag-drop event for files coming in. Dragging out to the desktop may not be possible, so if it isn't, ask the owner whether to drop that direction.
- [ ] **3.6 WSL files (W4).** Browse a distro using the local backend.
- [ ] **3.7 FTP (F5, SEC6).** Plain FTP with the warning before every connection. Add a `vsftpd` test container.
- [ ] **3.8 FTPS (F5, SEC8).** Explicit and implicit TLS, and the certificate rules in REQUIREMENTS.md. Integration-test: a trusted certificate, a self-signed one (asks), an accepted one (connects) and a changed one (blocks).
- [ ] **3.9 Security review.** Stop for the stronger-model review in PLAN.md section 6.
- [ ] **3.10 Follow the terminal's folder (F7).** Use xterm's `parser.registerOscHandler(7, ...)`. Treat the value as untrusted, and have it change only what the panel shows. Skip opening files in a local editor (F8), and record that as "later".
- [ ] **3.11 Finish.** Owner runs the phase 3 Windows list. Update `CLAUDE.md`.

## Phase 4: Serial

- [ ] **4.1 Port listing (R1).** With a refresh button. Linux adds USB names through `libudev`.
- [ ] **4.2 Serial backend (R2, R5, P4).** `TerminalBackend`, where `resize` does nothing. A read error or end of stream becomes a `failed` status with a reconnect offer. Test on Linux CI with virtual ports from `socat`.
- [ ] **4.3 Saved setups (R3).** Add serial sessions to the editor, with the fields in IF section 9.
- [ ] **4.4 Permission help (R4).** On Linux, a permission error explains this fix: `sudo usermod -aG dialout $USER`, then log out and back in. The app never runs it itself.
- [ ] **4.5 Should items (R6, R7).** Local echo, line endings, send break, and DTR and RTS toggles.
- [ ] **4.6 Finish.** Owner runs the phase 4 list with a real device. Update `CLAUDE.md`.

## Phase 5: Docker

- [ ] **5.1 Prove Docker over SSH first (D2).** IF section 8. Before any Docker UI, build this and test it in Linux CI, using the SSH test container with the Docker command line installed and the host's Docker socket mounted. If neither method works, stop and ask.
- [ ] **5.2 Local Docker (D1).** Add a connection screen with a clear message when Docker isn't running.
- [ ] **5.3 Containers (D3, D7).** List, group by Compose project, start, stop, restart, and remove with confirmation.
- [ ] **5.4 Logs (D4).** Live, with follow and search. Uses the same flow control as the terminal.
- [ ] **5.5 Details (D5).** Environment, ports, mounts, networks and raw inspect output. Display it as plain text only.
- [ ] **5.6 Shell in a container (D6).** Implements `TerminalBackend`, as a normal pane. Prefer `bash`, falling back to `sh`.
- [ ] **5.7 Should and Could items (D8, D9).** Live stats. Images, volumes and networks only if cheap. Add the "permission denied" explanation for remote Docker.
- [ ] **5.8 Security review.** Stop for the stronger-model review in PLAN.md section 6.
- [ ] **5.9 Finish.** Owner runs the phase 5 Windows list. Update `CLAUDE.md`.

## Phase 6: Polish

- [ ] **6.1 Leftovers.** Finish the settings screen and themes, and any Should items that were put off. List anything left, for the owner to decide.
- [ ] **6.2 Installers.** `.deb` and AppImage for Linux, and `.exe` for Windows, with an app icon. Measure Q1 on each.
- [ ] **6.3 Final security review.** Stop for the stronger-model review.
- [ ] **6.4 Ubuntu pass.** Owner runs every Ubuntu item on the checklist, including keychain behavior when the service is stopped. Fix what's found.
- [ ] **6.5 Finish.** Record all quality target measurements. Update `CLAUDE.md`.
</content>
