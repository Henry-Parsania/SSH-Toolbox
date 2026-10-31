# SSH Toolbox: Owner Test Checklist

This is a reminder of what's worth trying on real machines, not a sign-off form. Try what you care about, when there's a build worth trying, and skip what you don't use. The one thing that really needs doing is marked **Important** below. A screenshot and what you did is plenty when something's wrong.

To get a build, ask Claude to make one. Then download it from the repository's Actions tab on GitHub. On Ubuntu, install the `.deb` with `sudo apt install ./<file>.deb`.

## How targets are measured

| ID | Target | How to check it |
|---|---|---|
| Q1 | Download under 30 MB | File size of the Windows installer `.exe` and the Linux `.deb` |
| Q2 | Memory under 250 MB with one terminal | Run `scripts/measure-memory.ps1` on Windows or `scripts/measure-memory.sh` on Ubuntu. They add up the app and all its webview processes. On Linux, they use shared-aware numbers (PSS), so nothing is counted twice |
| Q3 | Usable in under 2 seconds | Time from launch until you can type. The app also logs this number. Measure on the second launch |
| Q4 | No noticeable typing lag | Your judgment: type quickly in the shell and in `vim` |
| Q5 | Heavy output doesn't freeze the app | Split into two panes, run `seq 1 1000000` in one, and type in the other while it runs. The run finishes with 1000000 as the last line. On Windows, use a Git Bash pane |
| Q6 | No flicker or errors in full-screen programs | Use `htop` and `vim` for a minute, including scrolling and resizing. On Windows, run them in a WSL pane |
| Q7 | Resizing is safe | Drag the window edge around quickly for 10 seconds, and maximize and restore |
| Q8 | Failures stay in one tab | See phase 2 |

## Phase 0
**Windows**
- [x] The app opens a window, showing the OS name. (Owner screenshot, 2026-10-31, from a dev build. The CI installer hasn't been tried yet.)

**Ubuntu (later)**
- [ ] The app from CI opens a window on Ubuntu 26.04.

## Phase 1
**Rendering, on Windows (renderer: auto)**
- [ ] Q4 typing, Q5 heavy output, Q6 `htop` and `vim`, and Q7 resizing all pass.
- [ ] `yes` stops within about a second of pressing Ctrl+C.
- [ ] Colors and non-English characters or emoji look right.

**Everything else, on Windows**
- [ ] PowerShell, cmd and Git Bash all open from the menu.
- [ ] Ctrl+Shift+C and V work, and pasting several lines asks first.
- [ ] Scrolling back 10,000 lines works. Search finds text.
- [ ] Tabs: new, close, rename, reorder. Switching tabs keeps what's on screen.
- [ ] Splits both ways, dragging dividers, and every shortcut in the shortcut list.
- [ ] Typing in all panes works when turned on.
- [ ] A link opens in your browser on Ctrl+click only.
- [ ] Font, size, theme and renderer settings survive a restart.
- [ ] Layout returns after a restart, with no surprises.
- [ ] Your WSL distro opens. `docker-desktop` isn't offered.
- [ ] Q2 and Q3 numbers recorded: ______

**Important: Ubuntu rendering, before phase 2 (also send your GPU model from `lspci | grep -i vga`)**
- [ ] Q4, Q5, Q6 and Q7 pass with renderer `auto` (DOM).
- [ ] If they don't: try renderer `webgl`, then each `webkit_workaround` setting. Report which helped.
- [ ] Scrolling is smooth enough for daily use. This is your call.
- [ ] Q2 and Q3 numbers recorded: ______

**Ubuntu (later)**
- [ ] Your default shell opens. No WSL options appear anywhere.
- [ ] Splits, tabs and shortcuts work, including that GNOME doesn't take any of them.

## Phase 2
Claude will say when the login code has had its review. Try key setup first on a server you can still reach another way.

**Windows**
- [ ] Connect to a real server with a password, a key, a passphrase-protected key and an agent.
- [ ] A first connection asks about the host key. A second doesn't.
- [ ] A changed host key is blocked. To test: edit that host's line in `~/.ssh/known_hosts`, using a copy to restore afterward.
- [ ] Run "Set up passwordless login" against a Linux server. Run it twice: the key isn't added twice.
- [ ] Do the same against a Windows server with an administrator account and a normal account, if you have one.
- [ ] Offer to delete the saved password appears only after the key login worked.
- [ ] Pull the network cable mid-session: that tab shows a clear message, other tabs keep working, and Reconnect works once the cable is back (Q8).
- [ ] Jump host and port forwarding work, if you use them.
- [ ] `~/.ssh/config` import finds your hosts.
- [ ] Sessions file contains no password: open it in a text editor and look.

**Ubuntu (later)**
- [ ] The same logins work, including your agent.
- [ ] Saving a password works with the keychain.
- [ ] Run `pkill gnome-keyring-daemon` and connect: the app keeps working and asks for the password (SEC4).
- [ ] Note whether `gnome-keyring-daemon` crashes when using the app.

## Phase 3
**Windows**
- [ ] The file panel opens beside an SSH session with no second login.
- [ ] Upload and download a large file and a folder.
- [ ] Cancel a transfer: the old file at the destination is untouched.
- [ ] Overwrite prompt appears. Delete asks first.
- [ ] Drag and drop works.
- [ ] Browse your WSL distro's files.
- [ ] Connect to your FTP server, if you have one. Plain FTP shows its warning.
- [ ] FTPS with a self-signed certificate asks first.

**Ubuntu (later)**
- [ ] The same transfers work.

## Phase 4
**Windows, with a real device**
- [ ] The port appears, with refresh.
- [ ] Connect at the device's baud rate, send and receive.
- [ ] Unplug: a clear message appears. Plug in and reconnect.

**Ubuntu (later)**
- [ ] The same tests. Without `dialout` membership, the app explains the fix.

## Phase 5
**Windows**
- [ ] Local Docker: list, start, stop, restart, remove (asks first).
- [ ] Docker on a remote server over a saved SSH session.
- [ ] Live logs follow and search. Config details are correct.
- [ ] A shell inside a container opens as a normal pane.
- [ ] A remote user without Docker access sees a clear explanation.

**Ubuntu (later)**
- [ ] The same tests, using the local Docker socket.

## Phase 6
- [ ] Installers work on a clean install on Windows and Ubuntu.
- [ ] Q1â€“Q8 measured on both, and recorded here: ______
- [ ] The "Ubuntu (later)" items you care about are done.

## Deferred on purpose
- T9, P7 and F8 are Could items. Tasks 1.15 and 3.10 record what was decided.
</content>
