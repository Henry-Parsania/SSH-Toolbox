import { describe, expect, it } from "vitest";
import { newLeaf, splitPane } from "./panes";
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

  it("splitting divides the window and the new pane shows a tab that was hidden", () => {
    const w = setup();
    w.newTab(spec, "two");
    expect(w.groupCount).toBe(1);
    const first = w.activeGroup;
    w.split("row");
    expect(w.groupCount).toBe(2);
    expect(w.activeGroup).not.toBe(first);
    // "one" was hidden behind "two", so the new pane shows it.
    expect(w.activeTab?.title).toBe("one");
    expect(w.groupTab[first]).toBe(w.tabs[1].id);
    expect(w.hiddenTabs).toEqual([]);
    expect(w.moveGroupFocus("left")).toBe(true);
    expect(w.activeGroup).toBe(first);
    expect(w.moveGroupFocus("left")).toBe(false); // at the edge: stays
  });

  it("choosing a tab shows it in the active pane, and a tab shown elsewhere swaps places", () => {
    const w = setup();
    const [one] = w.tabs;
    const two = w.newTab(spec, "two");
    w.split("row");
    const right = w.activeGroup;
    const left = w.groupOf(two.id)!;
    expect(w.groupOf(one.id)).toBe(right);
    // Pick "two" for the right pane: it comes from the left pane, which gets "one".
    w.selectTab(two.id);
    expect(w.groupTab[right]).toBe(two.id);
    expect(w.groupTab[left]).toBe(one.id);
    expect(w.activeTab?.title).toBe("two");
  });

  it("closing a pane keeps its tab, and the last pane can't be closed", () => {
    const w = setup();
    w.newTab(spec, "two");
    w.split("column");
    const extra = w.activeGroup;
    w.closeGroup(extra);
    expect(w.groupCount).toBe(1);
    expect(w.tabs).toHaveLength(2);
    expect(w.activeGroup).not.toBe(extra);
    w.closeGroup(w.activeGroup);
    expect(w.groupCount).toBe(1);
  });

  it("a panel opens in a pane beside the current one, and the button closes it again", () => {
    const w = setup();
    w.toggleView("files", "Files");
    expect(w.groupCount).toBe(2);
    expect(w.viewShown("files")).toBe(true);
    expect(w.activeTab?.view).toBe("files");
    // The panel follows the terminal that was in front.
    expect(w.contextTab?.title).toBe("one");
    w.toggleView("files", "Files");
    expect(w.viewShown("files")).toBe(false);
    expect(w.tabs.map((t) => t.title)).toEqual(["one"]);
    // The pane it filled goes too, so no empty pane is left.
    expect(w.groupCount).toBe(1);
  });

  it("closing the last pane closes the tab", () => {
    const w = setup();
    w.newTab(spec, "two");
    w.closeActivePane();
    expect(w.tabs.map((t) => t.title)).toEqual(["one"]);
  });

  it("dragging a divider changes the sizes", () => {
    const w = setup();
    w.newTab(spec, "two");
    w.split("row");
    w.resizeGroupDivider([], 0, 0.2);
    expect(w.groups.kind === "split" && w.groups.sizes[0]).toBeCloseTo(0.7, 9);
  });

  it("closing the last pane of a tab with several panes only closes that pane", () => {
    const w = setup();
    const tab = w.activeTab!;
    const added = newLeaf(spec, "inner");
    tab.root = splitPane(tab.root, tab.active, "row", added);
    tab.active = added.pane_id;
    w.closeActivePane();
    expect(w.paneCount(tab)).toBe(1);
    expect(w.tabs).toHaveLength(1);
  });
});

describe("Workspace layout saving", () => {
  const spec = { type: "local", shell_id: "x" } as const;

  it("saves and restores tabs, panes, sizes and what each pane shows", () => {
    const w = new Workspace();
    w.newTab(spec, "one");
    w.newTab({ type: "wsl", distro: "Ubuntu" }, "two");
    w.split("row");
    w.resizeGroupDivider([], 0, 0.2);
    w.toggleView("files", "Files");
    w.selectTab(w.tabs[0].id);

    const saved = JSON.parse(JSON.stringify(w.toLayout()));
    const again = new Workspace();
    again.restore(saved);

    expect(again.tabs.map((t) => t.title)).toEqual(["one", "two", "Files"]);
    expect(again.tabs[2].view).toBe("files");
    expect(again.groupCount).toBe(w.groupCount);
    expect(again.groupTab).toEqual(w.groupTab);
    expect(again.activeGroup).toBe(w.activeGroup);
    expect(again.activeTabId).toBe(w.tabs[0].id);
    expect(again.groups).toEqual(w.groups);
  });

  it("a layout from before panes comes back as one pane showing the tab that was in front", () => {
    const w = new Workspace();
    w.restore({
      version: 1,
      active_tab: "b",
      tabs: [
        { id: "a", title: "a", active: "pa", root: { kind: "leaf", pane_id: "pa", title: "a" } },
        { id: "b", title: "b", active: "pb", root: { kind: "leaf", pane_id: "pb", title: "b" } },
      ],
    });
    expect(w.groupCount).toBe(1);
    expect(w.activeTabId).toBe("b");
    expect(w.hiddenTabs.map((t) => t.id)).toEqual(["a"]);
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
  it("opens the chosen session as a new tab in a new pane beside the current one", () => {
    const w = new Workspace();
    w.newTab({ type: "local", shell_id: "a" }, "A");
    w.splitWith("row", { type: "ssh", saved_session_id: "s1" }, "Server");
    w.splitWith("column", { type: "local", shell_id: "b" }, "B");
    expect(w.groupCount).toBe(3);
    expect(w.tabs.map((t) => t.title)).toEqual(["A", "Server", "B"]);
    expect(w.hiddenTabs).toEqual([]);
    expect(w.activeTab?.title).toBe("B");
  });

  it("works with nothing open yet", () => {
    const w = new Workspace();
    w.splitWith("row", { type: "local", shell_id: "a" }, "A");
    expect(w.tabs).toHaveLength(1);
    expect(w.activeTab?.title).toBe("A");
  });
});
