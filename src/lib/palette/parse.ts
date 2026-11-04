// Helpers for the command palette and quick connect (tasks 7.2 and 7.3).

export interface QuickTarget {
  user: string;
  host: string;
  port: number;
}

const HOST = /^(?:([A-Za-z0-9._-]+)@)?(\[[0-9A-Fa-f:]+\]|[A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?)(?::(\d{1,5}))?$/;

/**
 * Reads `user@host:port` (user and port optional). It only claims text that looks like a
 * connection: it has a user or a port, or the host has a dot (an address or a domain name).
 * A plain word such as "split" is not one.
 */
export function parseQuickConnect(text: string): QuickTarget | null {
  const m = HOST.exec(text.trim());
  if (!m) return null;
  const user = m[1] ?? "";
  const host = m[2].replace(/^\[|\]$/g, "");
  const port = m[4] ? Number(m[4]) : 22;
  if (port < 1 || port > 65535) return null;
  if (!user && !m[4] && !host.includes(".") && !host.includes(":")) return null;
  return { user, host, port };
}

/** True when every word of the query appears in the text, ignoring case. */
export function matchesQuery(query: string, text: string): boolean {
  const hay = text.toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => hay.includes(w));
}
