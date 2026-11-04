// Tabs and their pane trees. All the rules for the tree live in panes.ts; this holds the state
// the screen draws.

import type { SavedLayout, SavedPane, SessionSpec } from "../backend";
import { keepAlive, type WindowPayload } from "./detach";
import {
  closePane,
  findLeaf,
  focusAfterClose,
  leaves,
  moveDivider,
  neighbour,
  newLeaf,
  splitPane,
  updateLeaf,
  type Direction,
  type Move,
  type PaneNode,
  type SplitPath,
} from "./panes";

export interface Tab {
  id: string;
  title: string;
  root: PaneNode;
  /** The pane that has the keyboard. */
  active: string;
}

export class Workspace {
  tabs = $state<Tab[]>([]);
  /**
   * The window is divided into panes ("groups"), in a tree like the one inside a tab. Each group
   * shows one tab, chosen from the tab bar. A group's id is the id of its leaf.
   */
  groups = $state<PaneNode>(newLeaf(null, ""));
  /** The tab each group shows. A group with no entry is empty. */
  groupTab = $state<Record<string, string>>({});
  /** The group that was clicked last: new tabs open in it and the tab bar chooses for it. */
  activeGroup = $state("");
  /** The last tab that was in front, for the side panels that follow the terminal. */
  lastTerminalTabId = $state("");
  /** Text to type into a pane once its session connects, then forgotten (such as a compose command). */
  pendingInput = $state<Record<string, string>>({});
  /** Tabs where a long command seems to have finished while they weren't showing. */
  finished = $state<Record<string, boolean>>({});

  /** Marks a tab whose long-running output has stopped, if it isn't showing in any pane. */
  markFinished(tabId: string) {
    if (this.tabs.some((t) => t.id === tabId) && this.groupOf(tabId) === null) this.finished[tabId] = true;
  }

  constructor() {
    this.activeGroup = (this.groups as Extract<PaneNode, { kind: "leaf" }>).pane_id;
  }

  /** The tab the active group shows. Setting it shows that tab in the active group. */
  get activeTabId(): string {
    return this.groupTab[this.activeGroup] ?? "";
  }

  set activeTabId(id: string) {
    this.assign(this.activeGroup, id);
  }

  /** The group showing a tab, if any. */
  groupOf(tabId: string): string | null {
    return Object.entries(this.groupTab).find(([, t]) => t === tabId)?.[0] ?? null;
  }

  /** Shows a tab in a group. A tab is only in one group: if another shows it, they swap. */
  assign(groupId: string, tabId: string) {
    if (tabId && !this.tabs.some((t) => t.id === tabId)) return;
    const before = this.groupTab[groupId];
    if (tabId) {
      const other = this.groupOf(tabId);
      if (other && other !== groupId) {
        if (before) this.groupTab[other] = before;
        else delete this.groupTab[other];
      }
      this.groupTab[groupId] = tabId;
      delete this.finished[tabId];
    } else {
      delete this.groupTab[groupId];
    }
    this.noteContext();
  }

  /** Remembers the tab in front, for the side panels that follow the terminal. */
  noteContext() {
    const tab = this.tabs.find((t) => t.id === this.activeTabId);
    if (tab) this.lastTerminalTabId = tab.id;
  }

  /** The tab that files, Docker and the agent follow: the one in front, or the last one that was (while an empty pane is chosen). */
  get contextTab(): Tab | undefined {
    return this.activeTab ?? this.tabs.find((t) => t.id === this.lastTerminalTabId);
  }

  /** Tabs that are not shown in any group. */
  get hiddenTabs(): Tab[] {
    const shown = new Set(Object.values(this.groupTab));
    return this.tabs.filter((t) => !shown.has(t.id));
  }

  focusGroup(id: string) {
    if (this.activeGroup === id || !findLeaf(this.groups, id)) return;
    this.activeGroup = id;
    this.noteContext();
  }

  /** Splits the active group. The new group is empty, and offers a choice of what to show. */
  splitGroup(direction: Direction): string {
    const added = newLeaf(null, "");
    this.groups = splitPane(this.groups, this.activeGroup, direction, added);
    this.activeGroup = added.pane_id;
    return added.pane_id;
  }

  /** Closes a group. Its tab stays open, in the tab bar. The last group can't be closed. */
  closeGroup(id: string) {
    const next = focusAfterClose(this.groups, id);
    const root = closePane(this.groups, id);
    if (!root) return;
    this.groups = root;
    delete this.groupTab[id];
    if (this.activeGroup === id && next) this.activeGroup = next;
    this.noteContext();
  }

  moveGroupFocus(move: Move): boolean {
    const target = neighbour(this.groups, this.activeGroup, move);
    if (!target) return false;
    this.activeGroup = target;
    this.noteContext();
    return true;
  }

  resizeGroupDivider(path: SplitPath, divider: number, delta: number) {
    this.groups = moveDivider(this.groups, path, divider, delta);
  }

  /** How many groups the window is divided into. */
  get groupCount(): number {
    return leaves(this.groups).length;
  }
  /** Which pane should show its search box, and how many times it has been asked. */
  searchTarget = $state("");
  searchCount = $state(0);
  saveTarget = $state("");
  saveCount = $state(0);
  /**
   * Panes brought back from the saved layout that must not connect by themselves (P6): SSH
   * panes wait for a click on Connect. Local and WSL panes just start fresh.
   */
  waiting = $state<Record<string, boolean>>({});

  /** Marks a pane as connected by the user, so it can open. */
  connect(paneId: string) {
    delete this.waiting[paneId];
  }

  /** Panes that show a session that is already open (a tab moved here from another window). */
  attach = $state<Record<string, string>>({});

  /**
   * Takes a tab out of this window and returns what another window needs to show it. Its
   * sessions are not ended: they carry on and the other window picks them up.
   */
  detachTab(id: string): WindowPayload | null {
    const tab = this.tabs.find((t) => t.id === id);
    if (!tab) return null;
    const attach: Record<string, string> = {};
    for (const leaf of leaves(tab.root)) {
      const session = this.sessionIds[leaf.pane_id];
      if (session) {
        attach[leaf.pane_id] = session;
        keepAlive.add(session);
      }
    }
    const payload: WindowPayload = { title: tab.title, root: $state.snapshot(tab.root) as PaneNode, attach, active: tab.active };
    this.closeTab(id);
    return payload;
  }

  /** The backend session behind each pane, for controls that act on it (serial lines). */
  sessionIds = $state<Record<string, string>>({});

  /** The folder each pane's shell last reported (OSC 7). Not saved: it is only a hint. */
  cwd = $state<Record<string, string>>({});

  /** When on, what is typed in one pane goes to every pane in the tab (P5). */
  broadcast = $state(false);

  requestSearch() {
    const tab = this.activeTab;
    if (!tab) return;
    this.searchTarget = tab.active;
    this.searchCount++;
  }

  /** Asks the active pane to save its output to a file. */
  requestSave() {
    const tab = this.activeTab;
    if (!tab) return;
    this.saveTarget = tab.active;
    this.saveCount++;
  }

  get activeTab(): Tab | undefined {
    return this.tabs.find((t) => t.id === this.activeTabId);
  }

  newTab(spec: SessionSpec | null, title: string): Tab {
    const leaf = newLeaf(spec, title);
    const tab: Tab = { id: crypto.randomUUID(), title, root: leaf, active: leaf.pane_id };
    this.tabs.push(tab);
    this.activeTabId = tab.id;
    return tab;
  }

  selectTab(id: string) {
    if (this.tabs.some((t) => t.id === id)) this.activeTabId = id;
  }

  /** Closes a tab. Its terminals go with it, because they leave the screen. */
  closeTab(id: string) {
    const at = this.tabs.findIndex((t) => t.id === id);
    if (at < 0) return;
    this.tabs.splice(at, 1);
    const freed = Object.entries(this.groupTab)
      .filter(([, t]) => t === id)
      .map(([g]) => g);
    for (const group of freed) delete this.groupTab[group];
    // A pane left empty shows the neighbouring tab, or another that isn't on screen, so it isn't
    // left blank.
    for (const group of freed) {
      const hidden = this.hiddenTabs;
      const neighbour = this.tabs[at] ?? this.tabs[at - 1];
      const next = (neighbour && hidden.some((t) => t.id === neighbour.id) ? neighbour : undefined) ?? hidden[0];
      if (next) this.assign(group, next.id);
    }
    if (this.lastTerminalTabId === id) this.lastTerminalTabId = "";
    this.noteContext();
  }

  renameTab(id: string, title: string) {
    const tab = this.tabs.find((t) => t.id === id);
    const clean = title.trim();
    if (tab && clean) tab.title = clean;
  }

  moveTab(from: number, to: number) {
    if (from === to || from < 0 || to < 0 || from >= this.tabs.length || to >= this.tabs.length) return;
    const [tab] = this.tabs.splice(from, 1);
    this.tabs.splice(to, 0, tab);
  }

  /** Switches by `step` tabs, wrapping round at either end. */
  stepTab(step: number) {
    if (this.tabs.length === 0) return;
    const at = this.tabs.findIndex((t) => t.id === this.activeTabId);
    this.activeTabId = this.tabs[(at + step + this.tabs.length) % this.tabs.length].id;
  }

  focusPane(tabId: string, paneId: string) {
    const tab = this.tabs.find((t) => t.id === tabId);
    if (tab && findLeaf(tab.root, paneId)) tab.active = paneId;
    const group = this.groupOf(tabId);
    if (group) this.focusGroup(group);
  }

  /**
   * Opens a copy of a tab's connection in a new pane beside it (`row`) or below it (`column`).
   * It is a new session of the same kind, such as a second shell on the same server, and starts
   * empty: nothing is copied from the first.
   */
  duplicateTab(tabId: string, direction: Direction): Tab | null {
    const tab = this.tabs.find((t) => t.id === tabId);
    const leaf = tab ? (findLeaf(tab.root, tab.active) ?? leaves(tab.root)[0]) : undefined;
    if (!tab || !leaf) return null;
    // The copy goes next to the tab that was chosen, wherever that is showing.
    const group = this.groupOf(tabId);
    if (group) this.focusGroup(group);
    this.splitGroup(direction);
    return this.newTab(leaf.spec, leaf.title);
  }

  /** Divides the window: a new pane beside (or below) the current one. */
  split(direction: Direction) {
    this.splitGroup(direction);
  }

  /** Closes a pane, and its tab if it was the last one. */
  closePane(tabId: string, paneId: string) {
    const tab = this.tabs.find((t) => t.id === tabId);
    if (!tab) return;
    const next = focusAfterClose(tab.root, paneId);
    const root = closePane(tab.root, paneId);
    if (!root) {
      this.closeTab(tabId);
      return;
    }
    tab.root = root;
    if (tab.active === paneId && next) tab.active = next;
  }

  closeActivePane() {
    const tab = this.activeTab;
    if (tab) this.closePane(tab.id, tab.active);
  }

  moveFocus(move: Move) {
    const tab = this.activeTab;
    const target = tab ? neighbour(tab.root, tab.active, move) : null;
    if (tab && target) tab.active = target;
    else this.moveGroupFocus(move);
  }

  resizeDivider(tabId: string, path: SplitPath, divider: number, delta: number) {
    const tab = this.tabs.find((t) => t.id === tabId);
    if (tab) tab.root = moveDivider(tab.root, path, divider, delta);
  }

  setPaneTitle(tabId: string, paneId: string, title: string) {
    const tab = this.tabs.find((t) => t.id === tabId);
    if (tab) tab.root = updateLeaf(tab.root, paneId, { title });
  }

  /** The tabs and panes in the form that is saved to `layout.toml`. */
  toLayout(): SavedLayout {
    return {
      version: 1,
      active_tab: this.activeTabId,
      tabs: $state.snapshot(this.tabs).map((t) => ({
        id: t.id,
        title: t.title,
        active: t.active,
        root: t.root as SavedPane,
      })),
      groups: $state.snapshot(this.groups) as SavedPane,
      group_tabs: $state.snapshot(this.groupTab),
      active_group: this.activeGroup,
    };
  }

  /** Replaces everything with a saved layout. Panes start fresh sessions of the saved kind. */
  restore(layout: SavedLayout) {
    this.waiting = {};
    for (const tab of layout.tabs) {
      for (const leaf of leavesOf(tab.root)) {
        // Neither SSH nor serial panes connect by themselves.
        if (leaf.spec?.type === "ssh" || leaf.spec?.type === "serial") this.waiting[leaf.pane_id] = true;
      }
    }
    // Files, Docker and the agent were tabs for a while. They are side panels again, so those tabs
    // from a saved layout are dropped.
    this.tabs = layout.tabs
      .filter((t) => !t.view)
      .map((t) => ({
        id: t.id,
        title: t.title,
        active: t.active,
        root: fromSaved(t.root),
      }));
    const front = this.tabs.some((t) => t.id === layout.active_tab) ? layout.active_tab : (this.tabs[0]?.id ?? "");
    const known = new Set(this.tabs.map((t) => t.id));
    if (layout.groups && layout.group_tabs) {
      this.groups = fromSaved(layout.groups);
      const ids = leaves(this.groups).map((l) => l.pane_id);
      this.groupTab = {};
      for (const [group, tab] of Object.entries(layout.group_tabs)) {
        if (ids.includes(group) && known.has(tab)) this.groupTab[group] = tab;
      }
      this.activeGroup = ids.includes(layout.active_group ?? "") ? (layout.active_group as string) : ids[0];
    } else {
      // A layout from before panes: one pane showing the tab that was in front.
      this.groups = newLeaf(null, "");
      this.activeGroup = (this.groups as Extract<PaneNode, { kind: "leaf" }>).pane_id;
      this.groupTab = {};
      if (front) this.groupTab[this.activeGroup] = front;
    }
    this.noteContext();
  }

  paneCount(tab: Tab): number {
    return leaves(tab.root).length;
  }
}

/** A saved pane leaves out `spec` when there is none. */
function fromSaved(node: SavedPane): PaneNode {
  if (node.kind === "leaf") {
    // A container's log or shell belonged to a Docker connection that ended with the last run.
    if (node.spec?.type === "docker_logs" || node.spec?.type === "docker_shell") {
      return { kind: "leaf", pane_id: node.pane_id, spec: null, title: "Terminal" };
    }
    return { kind: "leaf", pane_id: node.pane_id, spec: node.spec ?? null, title: node.title };
  }
  return { ...node, children: node.children.map(fromSaved) };
}

function leavesOf(node: SavedPane): Extract<SavedPane, { kind: "leaf" }>[] {
  return node.kind === "leaf" ? [node] : node.children.flatMap(leavesOf);
}
