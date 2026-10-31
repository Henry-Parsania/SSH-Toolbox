# Decisions and open items

Decisions the builder made on its own, and anything blocked. Newest last.

- 1.8: Dividers and tab reordering use pointer events, not HTML drag and drop, because Tauri's file-drop handling (needed in 3.5) switches HTML drag and drop off on Windows.
- 1.8: A new split pane runs the same kind of session as the pane it came from.
- 1.9: Hidden tabs stay mounted (display: none), so their terminals keep running. Closing a tab closes its sessions.
- 1.10: Shortcuts are caught in the capture phase on `window`, so they never reach the program. Search (Ctrl+Shift+F) and type-into-all-panes (Ctrl+Shift+B) are wired in tasks 1.13 and 1.15.
- Until the shell menu (1.11), "new tab" opens the first shell in the list.
