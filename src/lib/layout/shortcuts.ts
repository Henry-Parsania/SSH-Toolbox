// Keyboard shortcuts (INTERFACES.md section 12). Copy and paste are handled in the terminal.

export type ShortcutAction =
  | { type: "new-tab" }
  | { type: "close-pane" }
  | { type: "next-tab" }
  | { type: "previous-tab" }
  | { type: "split"; direction: "row" | "column" }
  | { type: "move-focus"; move: "left" | "right" | "up" | "down" }
  | { type: "search" }
  | { type: "broadcast" }
  | { type: "palette" }
  | { type: "zoom"; change: "in" | "out" | "reset" };

type KeyInfo = Pick<KeyboardEvent, "type" | "ctrlKey" | "shiftKey" | "altKey" | "metaKey" | "code">;

const ARROWS = {
  ArrowLeft: "left",
  ArrowRight: "right",
  ArrowUp: "up",
  ArrowDown: "down",
} as const;

/** The action for a key press, or `null` if it belongs to the program in the pane. */
export function shortcutFor(e: KeyInfo): ShortcutAction | null {
  if (e.type !== "keydown" || !e.ctrlKey || e.altKey || e.metaKey) return null;

  // Ctrl and plus, minus or zero change the terminal text size, as in a browser.
  if (!e.shiftKey || e.code === "Equal" || e.code === "NumpadAdd") {
    if (e.code === "Equal" || e.code === "NumpadAdd") return { type: "zoom", change: "in" };
    if (e.code === "Minus" || e.code === "NumpadSubtract") return { type: "zoom", change: "out" };
    if (e.code === "Digit0" || e.code === "Numpad0") return { type: "zoom", change: "reset" };
  }

  if (e.code === "Tab") return { type: e.shiftKey ? "previous-tab" : "next-tab" };
  if (!e.shiftKey) return null;

  switch (e.code) {
    case "KeyT":
      return { type: "new-tab" };
    case "KeyW":
      return { type: "close-pane" };
    case "KeyE":
      return { type: "split", direction: "row" };
    case "KeyO":
      return { type: "split", direction: "column" };
    case "KeyF":
      return { type: "search" };
    case "KeyB":
      return { type: "broadcast" };
    case "KeyP":
      return { type: "palette" };
  }
  if (e.code in ARROWS) return { type: "move-focus", move: ARROWS[e.code as keyof typeof ARROWS] };
  return null;
}
