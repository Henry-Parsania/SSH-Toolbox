// A tab, or a single session, shown in a window of its own. The new window is told what to show
// through its address, as URL-safe base64 of this description.

import type { PaneNode } from "./panes";

export interface WindowPayload {
  title: string;
  root: PaneNode;
  /** For each pane, the open session it should show instead of starting a new one. */
  attach: Record<string, string>;
  /** The pane that has the keyboard. */
  active: string;
}

/** Sessions that are being handed to another window, so closing their tab here must not end them. */
export const keepAlive = new Set<string>();

function validNode(node: unknown, depth = 0): node is PaneNode {
  if (depth > 20 || typeof node !== "object" || node === null) return false;
  const n = node as Record<string, unknown>;
  if (n.kind === "leaf") return typeof n.pane_id === "string" && typeof n.title === "string";
  if (n.kind === "split") {
    return (
      (n.direction === "row" || n.direction === "column") &&
      Array.isArray(n.sizes) &&
      Array.isArray(n.children) &&
      n.children.length > 0 &&
      n.children.length === n.sizes.length &&
      n.children.every((c) => validNode(c, depth + 1))
    );
  }
  return false;
}

export function encodePayload(payload: WindowPayload): string {
  const bytes = new TextEncoder().encode(JSON.stringify(payload));
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/** The description in an address, or null if it isn't one this app made. */
export function decodePayload(text: string): WindowPayload | null {
  try {
    const padded = text.replace(/-/g, "+").replace(/_/g, "/");
    const binary = atob(padded + "=".repeat((4 - (padded.length % 4)) % 4));
    const value = JSON.parse(new TextDecoder().decode(Uint8Array.from(binary, (c) => c.charCodeAt(0)))) as unknown;
    if (typeof value !== "object" || value === null) return null;
    const v = value as Record<string, unknown>;
    if (typeof v.title !== "string" || typeof v.active !== "string" || !validNode(v.root)) return null;
    const attach: Record<string, string> = {};
    if (typeof v.attach === "object" && v.attach !== null) {
      for (const [pane, id] of Object.entries(v.attach)) if (typeof id === "string") attach[pane] = id;
    }
    return { title: v.title, root: v.root, attach, active: v.active };
  } catch {
    return null;
  }
}
