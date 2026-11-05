// The macOS menu bar at the top of the screen. On Windows and Linux the app draws its own menu bar
// inside the window; on a Mac the menus belong to the system, with an app menu first and the usual
// Edit and Window menus (Edit is also what makes Cmd+C and Cmd+V work in text boxes).

import { Menu } from "@tauri-apps/api/menu";

export interface NativeItem {
  label: string;
  checked?: boolean;
  disabled?: boolean;
  /** For example "CmdOrCtrl+T". Shown in the menu and handled by the system. */
  accelerator?: string;
  onselect: () => void;
}
export type NativeMenu = { label: string; items: (NativeItem | "separator")[] };

type Entry = NonNullable<NonNullable<Parameters<typeof Menu.new>[0]>["items"]>[number];

const SEP: Entry = { item: "Separator" };

function entries(all: (NativeItem | "separator")[]): Entry[] {
  // Taking items out can leave a separator at the end, or two in a row.
  const items = all.filter((it, i) => it !== "separator" || (i > 0 && i < all.length - 1 && all[i - 1] !== "separator"));
  return items.map((it): Entry => {
    if (it === "separator") return SEP;
    const base = { text: it.label, enabled: !it.disabled, accelerator: it.accelerator, action: () => it.onselect() };
    return it.checked === undefined ? base : { ...base, checked: it.checked };
  });
}

let current: Menu | null = null;

/**
 * Puts the menu bar in place. `appItems` go in the app's own menu (About, Settings), between the
 * system's Services and Hide items and Quit. Call again to change a tick or a greyed-out item.
 */
export async function installNativeMenu(appName: string, appItems: NativeItem[], menus: NativeMenu[]): Promise<void> {
  const app: Entry = {
    text: appName,
    items: [
      ...entries(appItems),
      SEP,
      { item: "Services" },
      SEP,
      { item: "Hide" },
      { item: "HideOthers" },
      { item: "ShowAll" },
      SEP,
      { item: "Quit" },
    ],
  };
  const edit: Entry = {
    text: "Edit",
    items: [{ item: "Undo" }, { item: "Redo" }, SEP, { item: "Cut" }, { item: "Copy" }, { item: "Paste" }, { item: "SelectAll" }],
  };
  const window: Entry = {
    text: "Window",
    items: [{ item: "Minimize" }, { item: "Maximize" }, SEP, { item: "BringAllToFront" }],
  };
  const sub = (m: NativeMenu): Entry => ({ text: m.label, items: entries(m.items) });
  const help = menus.filter((m) => m.label === "Help").map(sub);
  const rest = menus.filter((m) => m.label !== "Help");
  // File first, then Edit, then the others, then Window and Help, as a Mac user expects.
  const ordered: Entry[] = [app, ...rest.slice(0, 1).map(sub), edit, ...rest.slice(1).map(sub), window, ...help];

  const menu = await Menu.new({ items: ordered });
  await menu.setAsAppMenu();
  const old = current;
  current = menu;
  if (old) void old.close().catch(() => {});
}
