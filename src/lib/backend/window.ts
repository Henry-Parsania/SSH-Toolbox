// The app window's own controls (the title bar is drawn by the page).
import { emitTo, listen } from "@tauri-apps/api/event";
import { getAllWindows, getCurrentWindow } from "@tauri-apps/api/window";

export function appWindow() {
  return getCurrentWindow();
}

/** The labels of the other app windows that the screen point (in the page's own pixels) is over. */
export async function otherWindowsAt(screenX: number, screenY: number): Promise<string[]> {
  const me = getCurrentWindow().label;
  const scale = window.devicePixelRatio || 1;
  const x = screenX * scale;
  const y = screenY * scale;
  const found: string[] = [];
  for (const w of await getAllWindows()) {
    if (w.label === me) continue;
    try {
      if (!(await w.isVisible()) || (await w.isMinimized())) continue;
      const [pos, size] = await Promise.all([w.outerPosition(), w.outerSize()]);
      if (x >= pos.x && x < pos.x + size.width && y >= pos.y && y < pos.y + size.height) found.push(w.label);
    } catch {
      // a window that can't be measured is not a target
    }
  }
  return found;
}

/** Hands a tab (as an encoded payload, see layout/detach.ts) to another window and brings it forward. */
export async function sendTabTo(label: string, payload: string): Promise<void> {
  await emitTo(label, "attach-tab", payload);
  const target = (await getAllWindows()).find((w) => w.label === label);
  await target?.setFocus().catch(() => {});
}

/** Calls `handler` whenever another window hands this one a tab. Returns a function that stops listening. */
export async function onTabReceived(handler: (payload: string) => void): Promise<() => void> {
  return listen<string>("attach-tab", (e) => handler(e.payload));
}
