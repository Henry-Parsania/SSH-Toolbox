import { describe, expect, it } from "vitest";
import { findLeaf, leaves } from "./panes";
import { shortcutFor } from "./shortcuts";
import { Workspace } from "./workspace.svelte";

const key = (code: string, extra: Record<string, unknown> = {}) => ({
  type: "keydown",
  ctrlKey: true,
  shiftKey: true,
  altKey: false,
  metaKey: false,
  code,
  ...extra,
});

describe("shortcutFor", () => {
  it("maps the documented shortcuts", () => {
    expect(shortcutFor(key("KeyT"))).toEqual({ type: "new-tab" });
    expect(shortcutFor(key("KeyW"))).toEqual({ type: "close-pane" });
    expect(shortcutFor(key("KeyE"))).toEqual({ type: "split", direction: "row" });
    expect(shortcutFor(key("KeyO"))).toEqual({ type: "split", direction: "column" });
    expect(shortcutFor(key("KeyF"))).toEqual({ type: "search" });
    expect(shortcutFor(key("KeyB"))).toEqual({ type: "broadcast" });
    expect(shortcutFor(key("ArrowLeft"))).toEqual({ type: "move-focus", move: "left" });
    expect(shortcutFor(key("ArrowDown"))).toEqual({ type: "move-focus", move: "down" });
    expect(shortcutFor(key("Tab", { shiftKey: false }))).toEqual({ type: "next-tab" });
    expect(shortcutFor(key("Tab"))).toEqual({ type: "previous-tab" });
  });

  it("lets every other key through to the program", () => {
    expect(shortcutFor(key("KeyT", { shiftKey: false }))).toBeNull(); // Ctrl+T
    expect(shortcutFor(key("KeyW", { shiftKey: false }))).toBeNull(); // Ctrl+W: used by editors
    expect(shortcutFor(key("KeyC"))).toBeNull(); // copy is the terminal's
    expect(shortcutFor(key("ArrowLeft", { shiftKey: false }))).toBeNull();
    expect(shortcutFor(key("KeyT", { ctrlKey: false }))).toBeNull();
    expect(shortcutFor(key("KeyT", { altKey: true }))).toBeNull();
    expect(shortcutFor(key("KeyT", { type: "keyup" }))).toBeNull();
  });
});

describe("Workspace", () => {
  const spec = { type: "local", shell_id: "x" } as const;

  function setup() {
    const w = new Workspace();
    w.newTab(spec, "one");
    return w;
  }

  it("a new tab becomes the active one", () => {
    const w = setup();
    const second = w.newTab(spec, "two");
    expect(w.activeTabId).toBe(second.id);
    expect(w.tabs.map((t) => t.title)).toEqual(["one", "two"]);
  });

  it("closing a tab selects its neighbour, and the last tab leaves none", () => {
    const w = setup();
    w.newTab(spec, "two");
    w.newTab(spec, "three");
    w.closeTab(w.activeTabId);
    expect(w.activeTab?.title).toBe("two");
    w.closeTab(w.tabs[0].id);
    w.closeTab(w.tabs[0].id);
    expect(w.tabs).toEqual([]);
    expect(w.activeTabId).toBe("");
  });

  it("renames, ignoring an empty name", () => {
    const w = setup();
    const id = w.tabs[0].id;
    w.renameTab(id, "  work  ");
    expect(w.tabs[0].title).toBe("work");
    w.renameTab(id, "   ");
    expect(w.tabs[0].title).toBe("work");
  });

  it("reorders tabs and steps through them with wrap-around", () => {
    const w = setup();
    w.newTab(spec, "two");
    w.newTab(spec, "three");
    w.moveTab(2, 0);
    expect(w.tabs.map((t) => t.title)).toEqual(["three", "one", "two"]);
    w.selectTab(w.tabs[2].id);
    w.stepTab(1);
    expect(w.activeTab?.title).toBe("three");
    w.stepTab(-1);
    expect(w.activeTab?.title).toBe("two");
  });

  it("splitting copies the session and focuses the new pane; closing goes back", () => {
    const w = setup();
    const first = w.activeTab!.active;
    w.split("row");
    const tab = w.activeTab!;
    expect(w.paneCount(tab)).toBe(2);
    expect(tab.active).not.toBe(first);
    w.moveFocus("left");
    expect(tab.active).toBe(first);
    w.moveFocus("left"); // at the edge: stays
    expect(tab.active).toBe(first);
    w.closeActivePane();
    expect(w.paneCount(tab)).toBe(1);
  });

  it("closing the last pane closes the tab", () => {
    const w = setup();
    w.newTab(spec, "two");
    w.closeActivePane();
    expect(w.tabs.map((t) => t.title)).toEqual(["one"]);
  });

  it("dragging a divider changes the sizes", () => {
    const w = setup();
    w.split("row");
    const tab = w.activeTab!;
    w.resizeDivider(tab.id, [], 0, 0.2);
    expect(tab.root.kind === "split" && tab.root.sizes[0]).toBeCloseTo(0.7, 9);
  });
});

describe("Workspace layout saving", () => {
  const spec = { type: "local", shell_id: "x" } as const;

  it("saves and restores tabs, splits, sizes and the active tab", () => {
    const w = new Workspace();
    w.newTab(spec, "one");
    w.newTab({ type: "wsl", distro: "Ubuntu" }, "two");
    w.split("row");
    w.resizeDivider(w.activeTab!.id, [], 0, 0.2);
    w.selectTab(w.tabs[0].id);

    const saved = JSON.parse(JSON.stringify(w.toLayout()));
    const again = new Workspace();
    again.restore(saved);

    expect(again.tabs.map((t) => t.title)).toEqual(["one", "two"]);
    expect(again.activeTabId).toBe(w.tabs[0].id);
    expect(again.tabs[1].root).toEqual(w.tabs[1].root);
    expect(again.paneCount(again.tabs[1])).toBe(2);
  });

  it("a pane saved without a session comes back with none", () => {
    const w = new Workspace();
    w.restore({
      version: 1,
      active_tab: "missing",
      tabs: [{ id: "t", title: "t", active: "p", root: { kind: "leaf", pane_id: "p", title: "p" } }],
    });
    expect(w.tabs[0].root).toMatchObject({ kind: "leaf", spec: null });
    expect(w.activeTabId).toBe("t");
  });
});

describe("restoring SSH panes", () => {
  it("SSH panes wait for Connect, local and WSL panes do not", () => {
    const w = new Workspace();
    w.restore({
      version: 1,
      active_tab: "t",
      tabs: [
        {
          id: "t",
          title: "t",
          active: "a",
          root: {
            kind: "split",
            direction: "row",
            sizes: [0.5, 0.5],
            children: [
              { kind: "leaf", pane_id: "a", title: "ssh", spec: { type: "ssh", saved_session_id: "s1" } },
              { kind: "leaf", pane_id: "b", title: "local", spec: { type: "local", shell_id: "x" } },
            ],
          },
        },
      ],
    });
    expect(w.waiting).toEqual({ a: true });
    w.connect("a");
    expect(w.waiting).toEqual({});
  });
});

describe("Workspace.splitWith", () => {
  it("puts the chosen session in a new pane beside the current one", () => {
    const w = new Workspace();
    w.newTab({ type: "local", shell_id: "a" }, "A");
    w.splitWith("row", { type: "ssh", saved_session_id: "s1" }, "Server");
    w.splitWith("column", { type: "local", shell_id: "b" }, "B");
    const tab = w.activeTab!;
    const specs = leaves(tab.root).map((l) => l.spec);
    expect(specs).toEqual([
      { type: "local", shell_id: "a" },
      { type: "ssh", saved_session_id: "s1" },
      { type: "local", shell_id: "b" },
    ]);
    expect(findLeaf(tab.root, tab.active)?.spec).toEqual({ type: "local", shell_id: "b" });
  });

  it("opens a tab when there is none to split", () => {
    const w = new Workspace();
    w.splitWith("row", { type: "local", shell_id: "a" }, "A");
    expect(w.tabs).toHaveLength(1);
  });
});
