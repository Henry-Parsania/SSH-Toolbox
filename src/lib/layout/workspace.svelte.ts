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
  activeTabId = $state("");
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
    if (this.activeTabId === id) this.activeTabId = (this.tabs[at] ?? this.tabs[at - 1])?.id ?? "";
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
  }

  /** Splits the active pane of the active tab. The new pane runs the same kind of session. */
  split(direction: Direction) {
    const tab = this.activeTab;
    if (!tab) return;
    const current = findLeaf(tab.root, tab.active);
    if (!current) return;
    const added = newLeaf(current.spec, current.title);
    tab.root = splitPane(tab.root, current.pane_id, direction, added);
    tab.active = added.pane_id;
  }

  /** Splits the active pane and opens `spec` in the new one, so a split can hold any session. */
  splitWith(direction: Direction, spec: SessionSpec, title: string) {
    const tab = this.activeTab;
    const current = tab ? findLeaf(tab.root, tab.active) : undefined;
    if (!tab || !current) {
      this.newTab(spec, title);
      return;
    }
    const added = newLeaf(spec, title);
    tab.root = splitPane(tab.root, current.pane_id, direction, added);
    tab.active = added.pane_id;
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
    if (!tab) return;
    const target = neighbour(tab.root, tab.active, move);
    if (target) tab.active = target;
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
    this.tabs = layout.tabs.map((t) => ({
      id: t.id,
      title: t.title,
      active: t.active,
      root: fromSaved(t.root),
    }));
    this.activeTabId = this.tabs.some((t) => t.id === layout.active_tab)
      ? layout.active_tab
      : (this.tabs[0]?.id ?? "");
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
