# Decisions and open items

Decisions the builder made on its own, and anything blocked. Newest last.

- 1.8: Dividers and tab reordering use pointer events, not HTML drag and drop, because Tauri's file-drop handling (needed in 3.5) switches HTML drag and drop off on Windows.
- 1.8: A new split pane runs the same kind of session as the pane it came from.
- 1.9: Hidden tabs stay mounted (display: none), so their terminals keep running. Closing a tab closes its sessions.
- 1.10: Shortcuts are caught in the capture phase on `window`, so they never reach the program. Search (Ctrl+Shift+F) and type-into-all-panes (Ctrl+Shift+B) are wired in tasks 1.13 and 1.15.
- Until the shell menu (1.11), "new tab" opens the first shell in the list.
- 1.12: Settings changes (font, size, theme, scrollback) apply to open terminals at once. The renderer applies to terminals opened afterwards. The light theme changes the terminal only; the tab bar and dialogs stay dark.
- 1.12: A `settings.toml` that can't be read is moved to `settings.toml.bad` and defaults are used, so the user's text is kept. Out-of-range numbers in a hand-edited file are pulled into range.
- 1.13: Links go through the backend command open_link, which checks http/https with the url crate before calling tauri-plugin-opener from Rust. The plugin itself is not registered with the window, so the interface has no permission to it.
- 1.14: The layout is saved 0.5 s after any change and read once at start, only if `restore_layout` is on. A layout that can't be read or fails the checks is moved to `layout.toml.bad`. Restored panes open new sessions of the saved kind.
- 1.15: Type-into-all-panes (P5) is done: Ctrl+Shift+B, with a red banner in the tab bar while it is on. It sends to the other panes of the same tab only.
- 1.15: T9 (save session output) is left for later: saving needs a file-save dialog, and the dialog plugin isn't in the approved list. P7 (move a tab to its own window) is left for later too.
- 1.16: Measured on the owner's Windows PC with a release build (`npx tauri build --no-bundle`), one PowerShell tab: memory 150 MB (Q2, target 250), usable after 1.3 s on a first launch (Q3, target 2 s), program file 10 MB (Q1 counts the installer, measured in 6.2). Linux numbers come from the owner.
- 2.1: `sessions.toml` has no field that can hold a secret, and unknown keys in a hand-edited file are dropped on the next save. A file that can't be read is never overwritten: saving is refused until the user chooses "start again", which keeps the old file as `sessions.toml.bad`. Every save keeps the previous version as `<name>.bak` (this also applies to settings and layout).
- 2.2: There is deliberately no command that sends a saved secret to the interface. Only the backend reads secrets, when connecting. The Linux keychain code compiles in CI but is untested until the owner tries it on Ubuntu.
- 2.3: The host key rules are tested end to end against a small SSH server that runs inside the test (`ssh/testing.rs`), so no container is needed. Its two keys are throwaway keys made for the tests and open nothing. A key read from a private key file carries a comment that a server's key doesn't, so tests strip it before comparing.
- 2.3: Prompts reach the interface as a `prompt` status and are answered with the `prompt_respond` command. The prompt dialogs themselves are built with the SSH pane in 2.5.
