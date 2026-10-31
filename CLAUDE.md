# SSH Toolbox

A lightweight, personal MobaXterm-style desktop app for Windows 11 and Ubuntu 26.04, with macOS optional later.

**Start here:** [docs/PLAN.md](docs/PLAN.md) for the architecture and how we work, and [docs/TASKS.md](docs/TASKS.md) for what's left. Read the parts of [docs/REQUIREMENTS.md](docs/REQUIREMENTS.md) and [docs/INTERFACES.md](docs/INTERFACES.md) that cover what you're working on. The next job is the first unticked task, unless the owner says otherwise.

## Working with the owner
- Claude writes all the code. The owner doesn't write code, and tests builds on real machines. Explain results and requests in plain language.
- This is a personal project, so keep the process light. Make normal technical decisions yourself. PLAN.md section 5 lists the few things that need asking.
- Decided: we are building this app. Don't suggest switching to Tabby, WindTerm or other existing tools.
- Decided: Tauri 2, Rust, Svelte and xterm.js. Electron is only the rendering fallback described in PLAN.md section 4.
- Linux support is essential. This PC can't judge rendering on the owner's Ubuntu desktop, so those results come from the owner.
- Testing is on Windows for now, and on Ubuntu later, apart from the rendering check in PLAN.md section 2.

## Current status (2026-11-01)
- Phase 0 is finished. Phase 1 is in progress, and `TASKS.md` shows how far it's got.
- The owner hasn't run the terminal rendering checks yet, on Windows or on Ubuntu. Rendering on Ubuntu is still the project's biggest unknown. `npx tauri build` hasn't been tried on the owner's machines either: installers come from CI.
- GitHub repository: https://github.com/Henry-Parsania/SSH-Toolbox (private). Pushing to `main` is approved. Never force-push.
- This Windows PC has Node.js, Rust (`stable-msvc`), the Visual Studio C++ build tools, git, WSL, WebView2 and Docker Desktop. Docker Desktop wasn't running during phase 0. The Windows `ssh-agent` service is on, with one key.
- Library results are in [docs/LIBRARIES.md](docs/LIBRARIES.md). Two matter later: task 2.3 must read `known_hosts` itself, because `russh` treats an unreadable file as "no keys", and Windows terminal tests must answer ConPTY's cursor-position request.
- Windows tools can't see a window's contents, but a window opened with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` can be read over the DevTools protocol, which is how the builder checked the page text and console errors.
- WSL has `Ubuntu` (26.04.1) and `docker-desktop`, both stopped. Installing packages in it needs the owner's `sudo` password. It isn't needed for now, so leave it alone.
- Early test: Termex, a Tauri SSH client, opened and drew its window correctly on the Ubuntu machine. Its terminal rendering wasn't tested.
- `gnome-keyring-daemon` crashed around Termex's first launch on that machine. The cause is unknown. Requirement SEC4 covers this.

Keep this section to facts that future work needs.
