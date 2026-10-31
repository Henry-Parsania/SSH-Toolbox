// Tabs and their pane trees. All the rules for the tree live in panes.ts; this holds the state
// the screen draws.

import type { SessionSpec } from "../backend";
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

  requestSearch() {
    const tab = this.activeTab;
    if (!tab) return;
    this.searchTarget = tab.active;
    this.searchCount++;
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

  paneCount(tab: Tab): number {
    return leaves(tab.root).length;
  }
}
