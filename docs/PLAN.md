# SSH Toolbox: Build Plan

This covers how the app in [REQUIREMENTS.md](REQUIREMENTS.md) gets built. Claude writes the code, and the owner tests builds on real machines and reports what's wrong.

**Documents, and which one wins if they disagree:**

| Order | Document | Answers |
|---|---|---|
| 1 | [REQUIREMENTS.md](REQUIREMENTS.md) | What the app must do |
| 2 | This plan | Architecture, libraries, rules for the builder |
| 3 | [INTERFACES.md](INTERFACES.md) | Exact interfaces, file formats, numbers and server commands |
| 4 | [TASKS.md](TASKS.md) | The ordered work for each phase |
| 5 | [TEST_CHECKLIST.md](TEST_CHECKLIST.md) | What the owner tests, and how targets are measured |

If two documents disagree, stop and ask the owner. Don't pick one quietly.

## 1. Architecture

| Layer | Choice |
|---|---|
| App shell | Tauri 2 |
| Backend | Rust, async with Tokio |
| Interface | Plain Svelte 5 with Vite and TypeScript. Not SvelteKit |
| Terminal | xterm.js 6 |
| Package manager | npm |

### Libraries
Versions are what was current and checked on 2026-10-31. Use these or newer when a task first adds the library, then keep them fixed through `Cargo.lock` and `package-lock.json`, which are both committed. Don't upgrade unless a task says to.

| Area | Library | Version | How to use it |
|---|---|---|---|
| App shell | `tauri`, `@tauri-apps/cli`, `@tauri-apps/api` | 2.12 | |
| Local shell and WSL | `portable-pty` | 0.9.0 | Its reads and writes block, so run them on dedicated threads. Last released February 2025 but widely used |
| SSH | `russh` | `=0.65.0` | `default-features = false, features = ["ring", "flate2", "rsa"]`. Using `ring` avoids the extra Windows build tools that `aws-lc-rs` may need. Fixed exactly because versions below 1.0 can break between releases |
| SFTP | `russh-sftp` | 3.0.1 | |
| FTP and FTPS | `suppaftp` | 12.2.1 | Features `tokio-rustls-ring` and `deprecated`. Implicit TLS is only available with `deprecated` |
| Serial | `tokio-serial` | 5.5.0 | Add feature `libudev` on Linux only, for USB device names |
| Docker | `bollard` | 0.21.1 | Default features. **Don't use its `ssh` feature:** it runs the system `ssh` program and wouldn't use the app's own login |
| Keys | `russh::keys` | with `russh` | **Don't add `ssh-key` separately.** `russh` depends on its own exact version, and a second version would cause type conflicts |
| Saved secrets | `keyring-core` 1.0.1, `windows-native-keyring-store` 1.1.0 on Windows, `zbus-secret-service-keyring-store` 1.0.1 on Linux | | **Don't use the `keyring` crate.** Its version 4 docs tell apps to use these directly |
| Config paths | `directories` | 6.0.0 | |
| Secret wiping | `zeroize` | 1.9 | Inside the `Secret` type in INTERFACES.md section 2 |
| Terminal addons | `@xterm/addon-fit`, `-search`, `-web-links`, `-unicode11`, `-webgl` | Latest | **Don't use `@xterm/addon-canvas`:** xterm 6 removed that renderer. Don't add `@xterm/addon-clipboard`: it lets remote programs touch the clipboard |
| Opening links | `tauri-plugin-opener` | 2.7 | Called only from the Rust backend (SEC9) |
| File dialogs | `tauri-plugin-dialog` | 2.8 | |

Phase 0 task 0.7 tests these on real behavior. If one fails, follow the stop rules in section 6.

### Design rules
The first three rules keep later phases small, and the rest protect responsiveness and credentials:

1. **One terminal interface.** Local shell, WSL, SSH, serial and Docker exec all implement `TerminalBackend` (INTERFACES.md section 3). Panes don't know which one they're showing.
2. **One file interface.** Local, SFTP and FTP all implement `FileBackend` (INTERFACES.md section 6), and the file browser uses only that.
3. **Interface kept separate from backend**, enforced in two places:
   - In Rust, everything lives in `crates/core`, which has no `tauri` dependency, so the compiler rejects any mix. `src-tauri` holds only thin Tauri commands.
   - In TypeScript, only `src/lib/backend/` may import `@tauri-apps/*`. `scripts/check-boundaries.mjs` enforces this in CI.
   This is what makes the Electron fallback affordable.
4. **Flow control on terminal output.** Use the exact method in INTERFACES.md section 4, so a huge `cat` can't freeze the app.
5. **Secrets stay in the backend.** The interface never receives saved passwords. A password the user types goes straight to the backend once and isn't kept in interface state.
6. **Nothing from a remote machine is trusted.** That includes terminal output, file names, window titles and links. Never use Svelte's `{@html}`, and see SEC9 and SEC11.

### Folder layout
```
src/                  Interface
  lib/backend/        The only code that talks to Tauri
  lib/terminal/       xterm.js wrapper and flow control
  lib/layout/         Tabs and the pane tree
  lib/sessions/       Sidebar and session editor
  lib/files/          File browser and transfer queue
  lib/docker/         Docker views
  lib/settings/       Settings screen
crates/core/src/      All backend logic, with no Tauri dependency
  error.rs            AppError
  secret.rs           Secret type
  terminal/           TerminalBackend, output pump, local shell
  wsl/                WSL distro list and launch
  ssh/                Connections, login, host keys, key setup, tunnels
  files/              FileBackend, transfer queue, local, SFTP, FTP
  serial/             Serial sessions
  docker/             Docker client and connection handling
  store/              sessions.toml, settings.toml, keychain
src-tauri/src/
  main.rs, lib.rs     App startup
  commands/           Thin Tauri commands, one file per area
tests/containers/     Dockerfiles for test SSH and FTP servers
scripts/              Boundary check and measurement scripts
docs/                 These documents, plus LIBRARIES.md from task 0.7
```

## 2. Phases

Each phase ends with a build the owner can use and the phase's list in [TEST_CHECKLIST.md](TEST_CHECKLIST.md). The next phase starts once that list passes. Sizes are relative effort. Task detail is in [TASKS.md](TASKS.md).

**Windows first, Ubuntu later.** The owner tests on Windows for now and tests on Ubuntu later. Every change still builds for Linux in CI, so Linux build problems show up straight away. Ubuntu items wait on the checklist, with one exception: the rendering decision at the end of phase 1.

| Phase | Size | Covers | Done when |
|---|---|---|---|
| 0: Setup | Small | Tools, git, CI, skeleton, library checks | An empty window opens on Windows, CI is green on both OSes, and `docs/LIBRARIES.md` is complete. Opening it on Ubuntu moves to the Ubuntu list |
| 1: Terminal, tabs, panes | Medium | T1–T9, P1–P7, W1–W3 | Windows list passes, and the Ubuntu rendering decision is made |
| 2: SSH and key setup | Large | S1–S7, K1–K7, SEC1–SEC5, SEC7 | Security review done, then the Windows list passes against real servers |
| 3: File transfer | Medium | F1–F8, W4, SEC6, SEC8, SEC11 | Windows list passes |
| 4: Serial | Small | R1–R7 | Windows list passes with a real device |
| 5: Docker | Medium | D1–D9 | Windows list passes |
| 6: Polish | Small | Settings, leftovers, installers, final review, measurements | All lists pass on Windows and Ubuntu |

SEC9 and SEC10 start in phases 0 and 1. macOS can be tried after phase 6.

**Decision point at the end of phase 1:** the owner runs the rendering tests on Ubuntu, using the Linux build from CI. Everything later assumes the terminal works there, so finding a failure after phase 5 would be expensive. If rendering fails with every renderer, follow section 4 before starting phase 2.

## 3. Testing

| Who | What |
|---|---|
| **Automated, every change** | `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test -p toolbox-core`, `npm run check`, `npm test`, the boundary check, and a full app build on Windows and Linux |
| **Automated, Linux CI only** | Integration tests against OpenSSH and FTP servers in containers, and serial tests with virtual ports. Windows CI machines can't run these Linux containers |
| **Claude on the Windows PC** | Builds, unit tests, and integration tests if Docker Desktop is running. Claude can't judge visual quality or typing feel |
| **Owner** | The checklist on real machines. Report problems with a screenshot and what you did |

Don't weaken, skip or delete a test to make it pass. If a test seems wrong, stop and ask.

## 4. Risks

| Risk | Likelihood | Response |
|---|---|---|
| Terminal is slow or glitchy on Ubuntu's webview | Medium, and the main risk | xterm 6 has only DOM and WebGL renderers. Try in order: DOM, WebGL, the WebKit settings in INTERFACES.md section 11, then xterm 5 with its canvas addon. If all fail, move the interface to Electron and run the Rust backend as a separate process. The app gets heavier, but most of the work carries over |
| `gnome-keyring-daemon` keeps crashing on Ubuntu | Unknown | The app keeps working and asks for passwords (SEC4). Check whether our app triggers it during Ubuntu testing |
| A library falls short, for example with agents or jump hosts | Medium | Phase 0 checks. Fix or replace it behind the design-rule interfaces, after asking the owner |
| Docker over SSH through `bollard`'s custom transport doesn't work as expected | Medium | Task 5.1 proves this first. If both methods in INTERFACES.md section 8 fail, stop and ask |
| Target memory use isn't reachable with a webview | Low to medium | Measure in phase 1 and adjust the target, since Q2 is a Should |
| Credential bugs that nobody reads the code to catch | Medium | Established libraries, tests for host key and secret handling, and a security review by a stronger model (section 6) |
| A less capable builder makes a subtle mistake | Medium | Exact instructions, stop rules, automated checks and stronger-model reviews at set points |
| AppImage doesn't run without FUSE on newer Ubuntu | Low | Make `.deb` the main Linux build |
| GitHub's Ubuntu 26.04 build machine is unavailable (it was in public preview in August 2026) | Low | Use the container fallback in task 0.6 |
| CI usage on a private repository runs out of free minutes | Low | Windows minutes count double. If it becomes a problem, build Windows only on `main` |

## 5. Open items for the owner

| Item | Needed by |
|---|---|
| Approve the Windows administrator prompt when the C++ build tools install | Phase 0 |
| Turn on the Windows `ssh-agent` service if you want it tested early (optional) | Phase 0 |
| GPU model on the Ubuntu machine, from `lspci \| grep -i vga` | End of phase 1 |
| Results of the Termex test: scrolling, glitches, typing, resizing and whether the keyring popup returned | Helpful, but phase 1 repeats these tests |
| A test server for SSH, plus Windows and FTP servers if you use them | Phases 2 and 3 |
| A serial device | Phase 4 |

**Settled:** the GitHub repository is https://github.com/Henry-Parsania/SSH-Toolbox (private). Build tools on the Windows PC are approved. The Ubuntu installed in WSL (26.04.1) isn't needed for now, because CI builds for Linux.

## 6. Rules for the builder

The builder may be a less capable model. These rules apply to whichever model is building.

**How to work**
- Read this plan, then REQUIREMENTS.md, INTERFACES.md and the current phase in TASKS.md.
- Do tasks in order, one at a time. A task is finished only when its "Done when" is true and the automated checks pass.
- After each task, commit with a message starting with the task number, such as `1.3: local shell backend`, and push to `main`. These pushes are approved for this repository. Never force-push.
- Update `CLAUDE.md`'s status section at the end of each phase.
- Report to the owner in plain language. The owner doesn't read code.
- Don't invent features, and don't add libraries beyond section 1 without asking. A small helper that a listed library needs anyway, such as `async-trait`, `serde`, `toml`, `tracing`, `uuid` or `tokio-util`, is fine.
- Use Svelte 5 syntax only: `$state`, `$derived`, `$effect`, `$props` and `onclick`. Don't use `export let`, `$:` or `on:click`.
- All Tauri commands are `async`, so they never block the window.
- When a library's API isn't what these documents describe, read the source of the locked version instead of guessing. If that only changes a small detail, adapt and note it in `docs/LIBRARIES.md`.

**Stop and ask the owner before:**
- Replacing or adding a library.
- Weakening, skipping or deleting a test or a CI check.
- Changing anything in a security requirement, or dropping a Must requirement.
- Changing a design rule, or a format in INTERFACES.md once it has shipped to the owner.
- Moving to the Electron fallback.
- Installing anything further on the owner's computer.
- Retrying the same failing fix a third time. Say what you tried and what happened.

**Never:**
- Use `--no-verify`, or turn off certificate or host key checks to get something working.
- Log, print, or put in an error message any password, passphrase, private key or keychain value.
- Commit secrets, test passwords that work anywhere real, or private keys, apart from throwaway keys generated when tests run.

**Reviews by a stronger model.** Stop at these points and tell the owner a stronger model should review before continuing, because a builder shouldn't be the only reader of its own security code:

| When | Review |
|---|---|
| Task 2.13 | SEC7: login, host keys, keychain, key setup and tunnels |
| Task 3.9 | FTPS certificates and remote file names |
| Task 5.8 | Docker over SSH |
| Task 6.3 | Final security review of the whole app |
</content>
