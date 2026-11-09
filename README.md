<div align="center">

# SSH Toolbox

### The free, fast, all-in-one SSH terminal for Windows, Linux and macOS.

**Terminals, SSH, a file explorer, Docker and an AI assistant in one small window.** If you live in a terminal and juggle servers, this is the tool you keep open all day.

[**Download**](https://github.com/Henry-Parsania/SSH-Toolbox/releases/latest) &nbsp;|&nbsp; [What it does](#what-it-does) &nbsp;|&nbsp; [Build it yourself](#running-and-building)

*If it saves you time, please give it a star. It helps other people find it.*

</div>

![SSH Toolbox: sessions list, Docker, Files and the Agent panel](docs/images/screenshot.jpg)

## Why you will want it

You like what an all-in-one tool like MobaXterm gives you: saved servers, a file browser beside the terminal, tabs, splits. But you want it **free, light and on every system you use**. That is this app.

| | |
|---|---|
| **Light and fast** | Low memory use, a 7 MB installer and a quick start. Built with Rust and Tauri, so there is no bundled browser to download. |
| **Your servers, organised** | Saved sessions in folders, with a live dot showing which ones answer. Jump servers (up to 3 hops), port forwarding, auto-reconnect when a connection drops. |
| **Key exchange in two clicks** | Make or pick a key and install it on the server, like `ssh-copy-id`, from the same window where you add the host. Passwords and passphrases live in the system keychain, never in a file. |
| **File explorer built in** | Browse your computer and the server side by side (SFTP, FTP, FTPS). Drag to copy, edit a remote file in the built-in editor, compare two folders and copy what is missing. |
| **Docker, local or over SSH** | See containers, logs, images, volumes and networks, open a shell in a container, and run Compose, on your machine or on any server you can reach. |
| **AI assistant that asks first** | Use Claude, OpenAI, DeepSeek, Qwen, OpenRouter or a local Ollama model with your own API key. It can look at the active server, and anything that changes something is shown to you before it runs. |
| **Themes you can make** | Eight built-in themes plus a theme builder with live preview. Colour a server's tab red for production, so you never type into the wrong one. |
| **Safe by default** | Host keys are checked, secrets never go in files or logs, and pasting several lines asks you first. Open source, Apache-2.0. |

Also: split panes, a command palette (`Ctrl+Shift+P`), snippets, session logs, serial ports, an X11 server option, network and port scanners, a process monitor, and a live CPU, memory and network footer for the server you are on.

## What it does

- **Terminals and tabs.** Local shells (PowerShell, cmd, Git Bash, WSL, or the default shell on Linux) in tabs. The window can be split into panes; click a pane and pick which open tab it shows, or start a new terminal or saved session in it. Right-click a tab to duplicate its connection to the right or below, rename it, or move it to its own window. The layout comes back on the next start.
- **Themes.** Eight built-in colour themes and a theme builder: pick six colours, see a preview, save your own and choose it from the list. The terminal follows the theme.
- **SSH sessions.** Saved in folders, with password, key or agent login, jump hosts (up to 3 hops) and a status dot that shows which servers answer. Host keys are checked against `known_hosts`. Give a session a colour and its tab and terminal background use it instead of the theme.
- **Key setup.** Make or pick a key and install it on a server (like `ssh-copy-id`), from the new-session window or the session's menu. Saved secrets go in the OS keychain, never in files.
- **Reconnect.** A dropped SSH connection reconnects by itself (up to 5 tries). When a session has ended, press `R` to reconnect or `C` to close it.
- **Command palette.** `Ctrl+Shift+P` jumps to any session, tab or command; type `user@host:port` to connect to a new address.
- **Snippets, startup commands and logs.** Saved commands you can send to one pane, a tab or every pane; commands a session runs after connecting; optional plain-text logs of every session.
- **Files.** Browse and copy between this computer and a server over SFTP, FTP or FTPS. Edit a text file in the built-in editor (with a diff view) or in your own program, and compare a local folder with a server folder.
- **Port forwarding.** Local, remote and SOCKS forwards per session.
- **X11.** Show windows of programs run on the server (Windows uses VcXsrv, installed separately).
- **Docker.** Containers, images, volumes, networks, logs and shells, on this computer or over SSH, plus Compose up/down/pull/restart/logs, build, tag, push, registry login, attach, file copy and network connect.
- **Process monitor.** Opens btop, htop or top for a session.
- **Network tools.** Network scanner and port scanner.
- **Serial ports.** Open a device with its settings.
- **Agent.** An optional assistant for the active SSH server; anything that changes something is shown first.
- **Also:** search and save terminal output, type into all panes at once, import from `~/.ssh/config`, back up and restore sessions, a trusted-servers (known hosts) manager, an optional tray icon, a check mark on a background tab when a long command goes quiet, a footer with CPU, memory and disk, and a Help window listing every feature and shortcut.

## Keyboard shortcuts

| Keys | Action |
|---|---|
| Ctrl+Shift+T | New tab |
| Ctrl+Shift+W | Close the active pane |
| Ctrl+Tab / Ctrl+Shift+Tab | Next or previous tab |
| Ctrl+Shift+E / Ctrl+Shift+O | Split side by side / top and bottom |
| Ctrl+Shift+arrows | Move between panes |
| Ctrl+Shift+F | Search the output |
| Ctrl+Shift+S | Save the output to Downloads |
| Ctrl+Shift+B | Type into all panes |
| Ctrl+Shift+P | Command palette |
| Ctrl+Shift+C / V | Copy and paste |

## Running and building

You need Node.js, Rust (stable) and, on Windows, the Visual Studio C++ build tools and WebView2.

```
npm install
npx tauri dev                    # run for development
npx tauri build --no-bundle      # standalone build: target/release/app.exe
npm test                         # frontend tests
cargo test -p toolbox-core       # backend tests
```

Installers are built by CI.

## Project layout

- `crates/core`: the backend logic (SSH, files, Docker, serial, X11), with no Tauri dependency
- `src-tauri`: the Tauri app and its commands
- `src`: the Svelte interface
- `docs`: [plan](docs/PLAN.md), [requirements](docs/REQUIREMENTS.md), [interfaces](docs/INTERFACES.md), [tasks](docs/TASKS.md); decisions are in [NOTES.md](NOTES.md)

## Contributing

This is a personal project and I decide what goes into it. Feature requests and bug reports are welcome as GitHub issues. If you want to send a change, open an issue first so we can agree on it before you write code. Pull requests are merged at my discretion, and by sending one you agree it can be released under the licence below. Please don't include secrets, passwords or keys in issues or code.

Security problems: please report them privately (GitHub's "Report a vulnerability" on the Security tab) rather than in a public issue.

## Licence

[Apache License 2.0](LICENSE). You may use, copy and modify it, including commercially, under the terms of that licence. The software comes with no warranty.

## Download and status

Get the Windows installer, the Linux `.deb` or AppImage, or the macOS disk image (Apple Silicon) from the [Releases page](https://github.com/Henry-Parsania/SSH-Toolbox/releases/latest).

The installers are not code-signed yet, so Windows may show SmartScreen (More info, Run anyway) and macOS asks you to allow the app once (System Settings, Privacy & Security, Open Anyway). Windows is the most tested platform. The Linux and macOS builds run and work, but have had less testing: bug reports are very welcome.
