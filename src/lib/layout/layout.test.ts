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

describe("zoom shortcuts", () => {
  const ctrl = (code: string, shift = false) => ({ type: "keydown", ctrlKey: true, shiftKey: shift, altKey: false, metaKey: false, code }) as Parameters<typeof shortcutFor>[0];

  it("maps Ctrl with plus, minus and zero", () => {
    expect(shortcutFor(ctrl("Equal"))).toEqual({ type: "zoom", change: "in" });
    expect(shortcutFor(ctrl("Equal", true))).toEqual({ type: "zoom", change: "in" });
    expect(shortcutFor(ctrl("Minus"))).toEqual({ type: "zoom", change: "out" });
    expect(shortcutFor(ctrl("Digit0"))).toEqual({ type: "zoom", change: "reset" });
    expect(shortcutFor(ctrl("Numpad0"))).toEqual({ type: "zoom", change: "reset" });
  });
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

  it("splitting divides the window and the new pane starts empty", () => {
    const w = setup();
    w.newTab(spec, "two");
    expect(w.groupCount).toBe(1);
    const first = w.activeGroup;
    w.split("row");
    expect(w.groupCount).toBe(2);
    expect(w.activeGroup).not.toBe(first);
    expect(w.activeTab).toBeUndefined();
    expect(w.groupTab[first]).toBe(w.tabs[1].id);
    expect(w.hiddenTabs.map((t) => t.title)).toEqual(["one"]);
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
    w.selectTab(one.id);
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
    w.selectTab(w.tabs[0].id);

    const saved = JSON.parse(JSON.stringify(w.toLayout()));
    const again = new Workspace();
    again.restore(saved);

    expect(again.tabs.map((t) => t.title)).toEqual(["one", "two"]);
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

describe("panes from the empty-pane chooser", () => {
  const spec = { type: "local", shell_id: "x" } as const;

  it("a split opens an empty pane when every tab is already showing, and a tab opened there fills it", () => {
    const w = new Workspace();
    w.newTab(spec, "A");
    w.split("row");
    const empty = w.activeGroup;
    expect(w.groupTab[empty]).toBeUndefined();
    expect(w.activeTab).toBeUndefined();
    // The panel beside it still has something to follow.
    expect(w.contextTab?.title).toBe("A");
    // Choosing "new terminal" in the empty pane opens a tab in it.
    w.newTab(spec, "B");
    expect(w.groupTab[empty]).toBe(w.tabs[1].id);
    expect(w.hiddenTabs).toEqual([]);
    expect(w.groupCount).toBe(2);
  });

  it("an empty pane can be given a tab that is open but hidden", () => {
    const w = new Workspace();
    w.newTab(spec, "A");
    w.newTab(spec, "B");
    w.split("column");
    w.closeGroup(w.activeGroup);
    w.split("column");
    const hidden = w.hiddenTabs;
    expect(hidden.length + Object.keys(w.groupTab).length).toBe(2);
  });

  it("tabs saved as panels by an earlier version are dropped on restore", () => {
    const w = new Workspace();
    w.restore({
      version: 1,
      active_tab: "f",
      tabs: [
        { id: "a", title: "a", active: "pa", root: { kind: "leaf", pane_id: "pa", title: "a" } },
        { id: "f", title: "Files", active: "pf", root: { kind: "leaf", pane_id: "pf", title: "Files" }, view: "files" },
      ],
      groups: { kind: "split", direction: "row", sizes: [0.5, 0.5], children: [
        { kind: "leaf", pane_id: "g1", title: "" },
        { kind: "leaf", pane_id: "g2", title: "" },
      ] },
      group_tabs: { g1: "a", g2: "f" },
      active_group: "g2",
    });
    expect(w.tabs.map((t) => t.title)).toEqual(["a"]);
    expect(w.groupTab).toEqual({ g1: "a" });
    expect(w.groupCount).toBe(2);
  });
});

describe("Workspace.duplicateTab", () => {
  it("opens a copy of the connection in a new pane beside the tab, or below it", () => {
    const w = new Workspace();
    const ssh = w.newTab({ type: "ssh", saved_session_id: "s1" }, "server");
    w.newTab({ type: "local", shell_id: "x" }, "shell"); // in front now
    const copy = w.duplicateTab(ssh.id, "row");
    expect(copy?.title).toBe("server");
    expect(copy?.root).toMatchObject({ spec: { type: "ssh", saved_session_id: "s1" } });
    expect(copy?.id).not.toBe(ssh.id);
    expect(w.groupCount).toBe(2);
    expect(w.activeTabId).toBe(copy?.id);
    expect(w.tabs.map((t) => t.title)).toEqual(["server", "shell", "server"]);

    // A second copy, below, splits the pane the chosen tab is in.
    w.duplicateTab(copy!.id, "column");
    expect(w.groupCount).toBe(3);
    expect(w.tabs).toHaveLength(4);
    expect(w.groups.kind === "split" && w.groups.children.some((c) => c.kind === "split" && c.direction === "column")).toBe(true);
  });

  it("does nothing for a tab that does not exist", () => {
    const w = new Workspace();
    expect(w.duplicateTab("nope", "row")).toBeNull();
    expect(w.groupCount).toBe(1);
  });
});

describe("Workspace finished marks", () => {
  it("marks a tab that isn't showing and clears the mark when it is shown", () => {
    const w = new Workspace();
    const a = w.newTab(null, "a");
    const b = w.newTab(null, "b"); // b is showing; a is not
    w.markFinished(b.id);
    expect(w.finished[b.id]).toBeUndefined();
    w.markFinished(a.id);
    expect(w.finished[a.id]).toBe(true);
    w.selectTab(a.id);
    expect(w.finished[a.id]).toBeUndefined();
  });
});

describe("Mac shortcuts (the Command key)", () => {
  const cmd = (code: string, extra: { shift?: boolean; alt?: boolean } = {}) =>
    ({ type: "keydown", ctrlKey: false, shiftKey: !!extra.shift, altKey: !!extra.alt, metaKey: true, code }) as Parameters<typeof shortcutFor>[0];
  it("zooms with Cmd and plus, minus and zero", () => {
    expect(shortcutFor(cmd("Equal", { shift: true }))).toEqual({ type: "zoom", change: "in" });
    expect(shortcutFor(cmd("Minus"))).toEqual({ type: "zoom", change: "out" });
    expect(shortcutFor(cmd("Digit0"))).toEqual({ type: "zoom", change: "reset" });
  });
  it("opens the palette with Cmd+Shift+P", () => {
    expect(shortcutFor(cmd("KeyP", { shift: true }))).toEqual({ type: "palette" });
  });
  it("moves between panes with Cmd+Option and an arrow", () => {
    expect(shortcutFor(cmd("ArrowLeft", { alt: true }))).toEqual({ type: "move-focus", move: "left" });
  });
  it("leaves the menu shortcuts and everything else to the system and the program", () => {
    expect(shortcutFor(cmd("KeyT"))).toBeNull();
    expect(shortcutFor(cmd("KeyC"))).toBeNull();
    expect(shortcutFor(cmd("KeyD", { shift: true }))).toBeNull();
  });
});
