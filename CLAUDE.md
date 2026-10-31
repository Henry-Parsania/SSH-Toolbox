# SSH Toolbox

A lightweight, personal MobaXterm-style desktop app for Windows 11 and Ubuntu 26.04, with macOS optional later.

**Before building, read these in order:**
1. [docs/PLAN.md](docs/PLAN.md): architecture, libraries and the rules for the builder (section 6 has the stop rules)
2. [docs/REQUIREMENTS.md](docs/REQUIREMENTS.md): what to build
3. [docs/INTERFACES.md](docs/INTERFACES.md): exact interfaces, formats, numbers and commands
4. [docs/TASKS.md](docs/TASKS.md): the ordered work, with what's finished ticked off
5. [docs/TEST_CHECKLIST.md](docs/TEST_CHECKLIST.md): what the owner tests

## Working with the owner
- Claude writes all the code. The owner doesn't write code, and tests builds on real machines. Explain results and requests in plain language.
- Decided: we are building this app. Don't suggest switching to Tabby, WindTerm or other existing tools.
- Decided: Tauri 2, Rust, Svelte and xterm.js. Electron is only the rendering fallback described in PLAN.md section 4.
- Linux support is essential. This PC can't judge rendering on the owner's Ubuntu desktop, so those results come from the owner.
- Testing is on Windows for now, and on Ubuntu later. CI builds for Linux on every change.
- The builder may be a less capable model. Stronger-model security reviews at the points in PLAN.md section 6 aren't optional.

## Current status (2026-10-31)
- Only the documents exist. There is no code, and this folder isn't a git repository yet.
- Next step: task 0.1 in [docs/TASKS.md](docs/TASKS.md).
- GitHub repository: https://github.com/Henry-Parsania/SSH-Toolbox. It's empty, and it's reachable from this PC.
- This Windows PC has git, WSL, WebView2 and Docker Desktop. Rust, Node.js and the Visual Studio C++ build tools are missing. The owner has approved installing those three, and the first attempt was stopped before anything installed.
- WSL has `Ubuntu` (26.04.1) and `docker-desktop`, both stopped. Installing packages in it needs the owner's `sudo` password. It isn't needed for now, so leave it alone.
- Early test: Termex, a Tauri SSH client, opened and drew its window correctly on the Ubuntu machine. The terminal rendering tests weren't reported, so rendering is still unproven (see PLAN.md section 5).
- `gnome-keyring-daemon` crashed around Termex's first launch on that machine. The cause is unknown. Requirement SEC4 covers this.

Keep this status section up to date at the end of each phase.
