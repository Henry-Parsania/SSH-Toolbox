# SSH Toolbox: Build Plan

This covers how the app in [REQUIREMENTS.md](REQUIREMENTS.md) gets built. It's a personal tool with one owner, built by Claude, possibly several agents at once. The owner tests on real machines and reports what's wrong.

**Keep the process light.** These documents are there to help, not to be obeyed for their own sake. If one is wrong or out of date, fix it and carry on.

| Document | Use it for |
|---|---|
| [REQUIREMENTS.md](REQUIREMENTS.md) | What the app does. This wins if documents disagree |
| This plan | Architecture, libraries and how we work |
| [INTERFACES.md](INTERFACES.md) | The design details, and things that are easy to get wrong |
| [TASKS.md](TASKS.md) | What's done and what's left |
| [LIBRARIES.md](LIBRARIES.md) | What we learned testing the libraries |
| [TEST_CHECKLIST.md](TEST_CHECKLIST.md) | Things for the owner to try, and how targets are measured |

## 1. Architecture

| Layer | Choice |
|---|---|
| App shell | Tauri 2 |
| Backend | Rust, async with Tokio |
| Interface | Plain Svelte 5 with Vite and TypeScript. Not SvelteKit |
| Terminal | xterm.js 6 |
| Package manager | npm |

### Libraries
These are the choices so far. `Cargo.lock` and `package-lock.json` are committed, so versions stay put until someone upgrades them on purpose. Upgrade or swap a library when there's a good reason, and note it in `docs/LIBRARIES.md`. The warnings below come from real checks, so read them before changing anything nearby.

| Area | Library | Notes |
|---|---|---|
| App shell | `tauri`, `@tauri-apps/cli`, `@tauri-apps/api` 2.12 | |
| Local shell and WSL | `portable-pty` 0.9.0 | Its reads and writes block, so run them on dedicated threads |
| SSH | `russh` `=0.65.0` | `default-features = false, features = ["ring", "flate2", "rsa"]`. Fixed exactly because versions below 1.0 can break between releases |
| SFTP | `russh-sftp` 3.0.1 | |
| FTP and FTPS | `suppaftp` 12.2.1 | Features `tokio-rustls-ring` and `deprecated`. Implicit TLS is only available with `deprecated` |
| Serial | `tokio-serial` 5.5.0 | Feature `libudev` on Linux only, for USB device names |
| Docker | `bollard` 0.21.1 | Don't use its `ssh` feature: it runs the system `ssh` program and wouldn't use the app's own login |
| Keys | `russh::keys` | Don't add `ssh-key` separately: `russh` needs its own exact version, and a second one causes type conflicts |
| Saved secrets | `keyring-core`, `windows-native-keyring-store`, `zbus-secret-service-keyring-store` | Not the `keyring` crate, whose version 4 docs tell apps to use these directly |
| Config paths | `directories` 6.0.0 | |
| Secret wiping | `zeroize` 1.9 | Inside the `Secret` type |
| Terminal addons | `@xterm/addon-fit`, `-search`, `-web-links`, `-unicode11`, `-webgl` | No `addon-canvas`: xterm 6 removed that renderer. No `addon-clipboard`: it lets remote programs touch the clipboard |
| Opening links | `tauri-plugin-opener` 2.7 | Called only from the Rust backend (SEC9) |
| File dialogs | `tauri-plugin-dialog` 2.8 | |

### Design rules
These keep later phases small and protect credentials. They're about the shape of the code, and they cost nothing to follow day to day:

1. **One terminal interface.** Local shell, WSL, SSH, serial and Docker exec all implement `TerminalBackend`. Panes don't know which one they're showing.
2. **One file interface.** Local, SFTP and FTP all implement `FileBackend`, and the file browser uses only that.
3. **Interface kept separate from backend.** Backend logic lives in `crates/core`, which has no `tauri` dependency, and `src-tauri` holds only thin commands. Only `src/lib/backend/` imports `@tauri-apps/*`. This keeps the Electron fallback affordable, and the compiler and `scripts/check-boundaries.mjs` enforce it automatically.
4. **Flow control on terminal output,** so a huge `cat` can't freeze the app.
5. **Secrets stay in the backend.** The interface never receives saved passwords. A password the user types goes straight to the backend and isn't kept in interface state.
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
  terminal/           TerminalBackend, output pump, local shell
  wsl/                WSL distro list and launch
  ssh/                Connections, login, host keys, key setup, tunnels
  files/              FileBackend, transfer queue, local, SFTP, FTP
  serial/             Serial sessions
  docker/             Docker client and connection handling
  store/              sessions.toml, settings.toml, keychain
src-tauri/src/        App startup, and thin commands in commands/
tests/containers/     Dockerfiles for test SSH and FTP servers
scripts/              Boundary check and measurement scripts
docs/                 These documents
```

## 2. Phases

Phases are the order of work, not gates. Nothing has to be signed off to move on. If something's broken, fix it when it's found. Work can be reordered if that makes more sense, as long as what a task depends on exists.

| Phase | Covers |
|---|---|
| 0: Setup | Finished |
| 1: Terminal, tabs, panes | T1–T9, P1–P7, W1–W3 |
| 2: SSH and key setup | S1–S7, K1–K7, SEC1–SEC5, SEC7 |
| 3: File transfer | F1–F8, W4, SEC6, SEC8, SEC11 |
| 4: Serial | R1–R7 |
| 5: Docker | D1–D9 |
| 6: Polish | Settings, leftovers, installers, measurements |

SEC9 and SEC10 start in phases 0 and 1. macOS can be tried after phase 6.

### The two checkpoints that matter
Everything else is flexible. These two remain because they're cheap to do and costly to skip:

1. **Try terminal rendering on Ubuntu before starting phase 2.** It's the project's biggest unknown, and everything later assumes it works. Do this as soon as the terminal is usable. If it fails with every renderer, see section 4 before building more.
2. **Do one security review of the login code before relying on it** (SEC7). Build and test against the local test container, since that's easiest. Try key setup on a server you can still reach another way, because a bug there could lock you out.

## 3. Testing

**Test where a bug would really hurt:** credentials, host keys, key setup, anything that could lose data or files, and tricky logic such as parsing and flow control. Screens and layout don't need automated tests, since the owner sees those directly.

| Who | What |
|---|---|
| **CI, every push** | Format, lint, tests and the library checks on Windows and Linux, so Linux problems show up straight away |
| **CI, when a build is wanted** | Full installers for both systems. Run it by pushing a tag named `build-<anything>`, or from the Actions tab. These are slow, so they don't run on every push |
| **Claude on the Windows PC** | Builds and tests. Claude can't judge visual quality or typing feel |
| **Owner** | Tries builds on real machines when there's something worth trying. A screenshot and what you did is plenty |

If a test is wrong, change it and say why in the commit. Don't weaken a security test just to make it pass.

## 4. Risks

| Risk | Response |
|---|---|
| Terminal is slow or glitchy on Ubuntu's webview. This is the main risk | xterm 6 has only DOM and WebGL renderers. Try in order: DOM, WebGL, the WebKit settings in INTERFACES.md section 11, then xterm 5 with its canvas addon. If all fail, move the interface to Electron and run the Rust backend as a separate process. The app gets heavier, but most of the work carries over |
| `gnome-keyring-daemon` keeps crashing on Ubuntu | The app keeps working and asks for passwords (SEC4). Check whether our app triggers it during Ubuntu testing |
| A library falls short | Fix or replace it behind the design-rule interfaces |
| Docker over SSH through `bollard`'s custom transport doesn't work | Task 5.1 tries it before any Docker screens get built. If both methods in INTERFACES.md section 8 fail, tell the owner the options |
| Target memory use isn't reachable with a webview | Measure, and adjust the target, since Q2 is a Should |
| Credential bugs that nobody reads the code to catch | Established libraries, tests for host key handling, and the review in section 5 |
| AppImage doesn't run without FUSE on newer Ubuntu | Make `.deb` the main Linux build |

**What the owner will need to provide:** the Ubuntu GPU model (`lspci | grep -i vga`) for the rendering check, a test SSH server for phase 2, Windows and FTP servers if you use them, and a serial device for phase 4.

## 5. How we work

**Decide and keep moving.** Make normal technical decisions yourself: libraries, file formats, code structure, the order of work, and small changes to scope. Note anything surprising in the commit or the relevant document.

**Ask the owner first only for:**
- Weakening a security requirement, or dropping a Must requirement.
- Moving to the Electron fallback.
- Installing something on the owner's computer, spending money, or deleting the owner's data.

**Security level.** This is a personal app on the owner's own computer, with no other users, so there's no need for hardening beyond REQUIREMENTS.md. It does hold logins to real servers, so the basics stay: check host keys, keep passwords out of files and logs, and don't trust what a server sends.

**Never:**
- Turn off certificate or host key checks to get something working.
- Log, print, or put in an error message any password, passphrase, private key or keychain value.
- Commit real secrets or private keys. Throwaway keys generated during tests are fine.

**Git.** Work on `main`, and commit and push at sensible points: a finished piece of work, not necessarily every small step. Never force-push.

**Several agents at once.** Give each a separate area, such as the terminal in one and the sidebar in another, and its own branch or git worktree. Merge to `main` when its tests pass. Pull before starting. If two agents would touch the same files, run them one after the other instead.

**Records.** Tick off `TASKS.md` when work is done, and keep `CLAUDE.md`'s status to facts that future work needs. Don't update documents just for the sake of it.

**Code habits that prevent real bugs:**
- Use Svelte 5 syntax only: `$state`, `$derived`, `$effect`, `$props` and `onclick`. Don't use `export let`, `$:` or `on:click`.
- Make Tauri commands `async`, so they never block the window.
- When a library doesn't match these documents, read its source instead of guessing, then update the document.

**Talking to the owner.** Use plain language. The owner doesn't read code.

**Security review.** Once, in task 2.13, a separate session reads the login, host key, keychain and key setup code, because nobody catches their own mistakes. Use a stronger model if the builder is a less capable one. The `/security-review` command is an easy way to do it. Fix what's serious and move on. Task 6.3 is a quick repeat over what was added afterward.
</content>
