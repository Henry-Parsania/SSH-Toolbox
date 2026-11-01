// The pane tree (INTERFACES.md section 9). Plain data and pure functions, no UI.
// Every function returns a new tree and leaves the one it was given untouched.

import type { SessionSpec } from "../backend";

export type Direction = "row" | "column";
export type Move = "left" | "right" | "up" | "down";

export type PaneNode =
  | { kind: "leaf"; pane_id: string; spec: SessionSpec | null; title: string }
  | { kind: "split"; direction: Direction; sizes: number[]; children: PaneNode[] };

export type Leaf = Extract<PaneNode, { kind: "leaf" }>;

/** The smallest share of a split that a child can be dragged down to. */
export const MIN_SIZE = 0.05;

export function newLeaf(spec: SessionSpec | null, title: string, paneId?: string): Leaf {
  return { kind: "leaf", pane_id: paneId ?? crypto.randomUUID(), spec, title };
}

/** All panes, in reading order (left to right, top to bottom). */
export function leaves(node: PaneNode): Leaf[] {
  return node.kind === "leaf" ? [node] : node.children.flatMap(leaves);
}

export function findLeaf(node: PaneNode, paneId: string): Leaf | null {
  return leaves(node).find((l) => l.pane_id === paneId) ?? null;
}

/** Scales sizes so they add up to 1. Falls back to equal shares if they can't. */
function normalise(sizes: number[]): number[] {
  const total = sizes.reduce((a, b) => a + b, 0);
  if (!(total > 0) || sizes.some((s) => !Number.isFinite(s) || s < 0)) {
    return sizes.map(() => 1 / sizes.length);
  }
  return sizes.map((s) => s / total);
}

/** Changes one pane (its title or session) and leaves the rest alone. */
export function updateLeaf(
  root: PaneNode,
  paneId: string,
  change: Partial<Pick<Leaf, "spec" | "title">>,
): PaneNode {
  return mapLeaf(root, paneId, (leaf) => ({ ...leaf, ...change }));
}

function mapLeaf(node: PaneNode, paneId: string, fn: (leaf: Leaf) => PaneNode): PaneNode {
  if (node.kind === "leaf") return node.pane_id === paneId ? fn(node) : node;
  return { ...node, children: node.children.map((c) => mapLeaf(c, paneId, fn)) };
}

/**
 * Splits a pane in two. The new pane goes after the old one (right or below). If the old pane
 * already sits in a split going the same way, the new pane joins that split instead of nesting,
 * and takes half of the old pane's share.
 */
export function splitPane(root: PaneNode, paneId: string, direction: Direction, added: Leaf): PaneNode {
  if (!findLeaf(root, paneId)) return root;

  if (root.kind === "leaf") {
    return { kind: "split", direction, sizes: [0.5, 0.5], children: [root, added] };
  }

  const at = root.children.findIndex((c) => c.kind === "leaf" && c.pane_id === paneId);
  if (at >= 0) {
    if (root.direction === direction) {
      const sizes = [...root.sizes];
      const half = sizes[at] / 2;
      sizes.splice(at, 1, half, half);
      const children = [...root.children];
      children.splice(at + 1, 0, added);
      return { ...root, sizes, children };
    }
    const children = [...root.children];
    children[at] = {
      kind: "split",
      direction,
      sizes: [0.5, 0.5],
      children: [root.children[at], added],
    };
    return { ...root, children };
  }

  return { ...root, children: root.children.map((c) => splitPane(c, paneId, direction, added)) };
}

/**
 * Removes a pane. The space goes to its neighbours in proportion. A split left with one child
 * is replaced by that child, and the closed last pane gives `null` (no panes left).
 */
export function closePane(root: PaneNode, paneId: string): PaneNode | null {
  if (!findLeaf(root, paneId)) return root;
  return remove(root, paneId);
}

function remove(node: PaneNode, paneId: string): PaneNode | null {
  if (node.kind === "leaf") return node.pane_id === paneId ? null : node;

  const kept: { child: PaneNode; size: number }[] = [];
  node.children.forEach((child, i) => {
    const result = remove(child, paneId);
    if (result) kept.push({ child: result, size: node.sizes[i] });
  });

  if (kept.length === 0) return null;
  if (kept.length === 1) return kept[0].child;

  // A child that collapsed into a split going the same way as this one is merged in, so the
  // tree never has a split directly inside a split of the same direction.
  const children: PaneNode[] = [];
  const sizes: number[] = [];
  for (const { child, size } of kept) {
    if (child.kind === "split" && child.direction === node.direction) {
      child.children.forEach((c, i) => {
        children.push(c);
        sizes.push(size * child.sizes[i]);
      });
    } else {
      children.push(child);
      sizes.push(size);
    }
  }
  return { ...node, children, sizes: normalise(sizes) };
}

/** Where a split is in the tree: the child index to follow at each step from the root. */
export type SplitPath = number[];

function nodeAt(root: PaneNode, path: SplitPath): PaneNode | null {
  let node: PaneNode = root;
  for (const i of path) {
    if (node.kind !== "split" || i < 0 || i >= node.children.length) return null;
    node = node.children[i];
  }
  return node;
}

/**
 * Moves the divider between child `divider` and the next one by `delta` (a share of the whole
 * split, so 0.1 is a tenth). Neither side goes below `min`.
 */
export function moveDivider(
  root: PaneNode,
  path: SplitPath,
  divider: number,
  delta: number,
  min: number = MIN_SIZE,
): PaneNode {
  const target = nodeAt(root, path);
  if (!target || target.kind !== "split") return root;
  if (divider < 0 || divider >= target.sizes.length - 1 || !Number.isFinite(delta)) return root;

  const sizes = [...target.sizes];
  const pair = sizes[divider] + sizes[divider + 1];
  const first = Math.min(Math.max(sizes[divider] + delta, min), pair - min);
  sizes[divider] = first;
  sizes[divider + 1] = pair - first;
  return replaceAt(root, path, { ...target, sizes });
}

function replaceAt(node: PaneNode, path: SplitPath, replacement: PaneNode): PaneNode {
  if (path.length === 0) return replacement;
  if (node.kind !== "split") return node;
  const [head, ...rest] = path;
  const children = [...node.children];
  children[head] = replaceAt(children[head], rest, replacement);
  return { ...node, children };
}

export interface Rect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

function layout(node: PaneNode, rect: Rect, out: Map<string, Rect>): void {
  if (node.kind === "leaf") {
    out.set(node.pane_id, rect);
    return;
  }
  let offset = 0;
  node.children.forEach((child, i) => {
    const share = node.sizes[i];
    if (node.direction === "row") {
      const width = rect.right - rect.left;
      layout(
        child,
        { ...rect, left: rect.left + offset * width, right: rect.left + (offset + share) * width },
        out,
      );
    } else {
      const height = rect.bottom - rect.top;
      layout(
        child,
        { ...rect, top: rect.top + offset * height, bottom: rect.top + (offset + share) * height },
        out,
      );
    }
    offset += share;
  });
}

/** A divider between two children of a split, as a line across the split's rectangle. */
export interface DividerPlace {
  path: SplitPath;
  index: number;
  direction: Direction;
  /** The split it belongs to, so a drag can turn pixels into a share of it. */
  split: Rect;
  /** Where the line sits along the split's axis, as a share of the whole window. */
  at: number;
}

/**
 * Where every pane and divider goes, as shares of the tab (0 to 1). Panes are drawn in one flat
 * list, so splitting never moves a running terminal to a different place in the page.
 */
export function placeAll(root: PaneNode): { panes: { leaf: Leaf; rect: Rect }[]; dividers: DividerPlace[] } {
  const panes: { leaf: Leaf; rect: Rect }[] = [];
  const dividers: DividerPlace[] = [];
  const walk = (node: PaneNode, rect: Rect, path: SplitPath) => {
    if (node.kind === "leaf") {
      panes.push({ leaf: node, rect });
      return;
    }
    const row = node.direction === "row";
    const size = row ? rect.right - rect.left : rect.bottom - rect.top;
    const start = row ? rect.left : rect.top;
    let offset = 0;
    node.children.forEach((child, i) => {
      const share = node.sizes[i];
      const a = start + offset * size;
      const b = start + (offset + share) * size;
      walk(child, row ? { ...rect, left: a, right: b } : { ...rect, top: a, bottom: b }, [...path, i]);
      if (i < node.children.length - 1) {
        dividers.push({ path, index: i, direction: node.direction, split: rect, at: b });
      }
      offset += share;
    });
  };
  walk(root, { left: 0, top: 0, right: 1, bottom: 1 }, []);
  return { panes, dividers };
}

const EDGE = 1e-6;

/**
 * The pane next to `paneId` in the given direction, or `null` if it is at that edge. When
 * several panes touch, the one overlapping most with the current pane wins.
 */
export function neighbour(root: PaneNode, paneId: string, move: Move): string | null {
  const rects = new Map<string, Rect>();
  layout(root, { left: 0, top: 0, right: 1, bottom: 1 }, rects);
  const from = rects.get(paneId);
  if (!from) return null;

  const horizontal = move === "left" || move === "right";
  let best: { id: string; gap: number; overlap: number } | null = null;

  for (const [id, r] of rects) {
    if (id === paneId) continue;
    const gap =
      move === "right" ? r.left - from.right
      : move === "left" ? from.left - r.right
      : move === "down" ? r.top - from.bottom
      : from.top - r.bottom;
    if (gap < -EDGE) continue;
    const overlap = horizontal
      ? Math.min(from.bottom, r.bottom) - Math.max(from.top, r.top)
      : Math.min(from.right, r.right) - Math.max(from.left, r.left);
    if (overlap <= EDGE) continue;
    if (!best || gap < best.gap - EDGE || (Math.abs(gap - best.gap) <= EDGE && overlap > best.overlap)) {
      best = { id, gap, overlap };
    }
  }
  return best?.id ?? null;
}

/** The pane to focus after `paneId` is closed: the one that was before it, else the next. */
export function focusAfterClose(root: PaneNode, paneId: string): string | null {
  const all = leaves(root).map((l) => l.pane_id);
  const at = all.indexOf(paneId);
  if (at < 0) return all[0] ?? null;
  return all[at - 1] ?? all[at + 1] ?? null;
}
