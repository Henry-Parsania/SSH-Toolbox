// Pure helpers for the file panel: paths, sorting and how sizes, times and permissions read.

import type { FileEntry } from "../backend/files";

/** Puts a name on the end of a folder path, with the separator that system uses. */
export function joinPath(dir: string, name: string, sep: string): string {
  if (dir.endsWith("/") || dir.endsWith("\\")) return dir + name;
  return `${dir}${sep}${name}`;
}

/** The folder above this one. A root stays a root. */
export function parentPath(path: string, sep: string): string {
  const trimmed = path.length > 1 ? path.replace(/[\\/]+$/, "") : path;
  const cut = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  if (cut < 0) return path;
  if (cut === 0) return sep === "/" ? "/" : trimmed.slice(0, 1);
  const parent = trimmed.slice(0, cut);
  // "C:" alone is not a folder: its root is "C:\".
  return /^[A-Za-z]:$/.test(parent) ? parent + sep : parent;
}

/** The last part of a path. */
export function baseName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const cut = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  return cut < 0 ? trimmed : trimmed.slice(cut + 1);
}

/** The folders leading to a path, each with what to open when it is clicked. */
export function crumbs(path: string, sep: string): { label: string; path: string }[] {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  const out: { label: string; path: string }[] = [];
  const windows = /^[A-Za-z]:/.test(path);
  let at = windows ? "" : "/";
  if (!windows) out.push({ label: "/", path: "/" });
  parts.forEach((part, i) => {
    at = windows ? (i === 0 ? part + sep : joinPath(at, part, sep)) : joinPath(at, part, sep);
    out.push({ label: part, path: at });
  });
  return out;
}

/** Folders first, then by name, ignoring case. */
export function sortEntries(entries: FileEntry[]): FileEntry[] {
  const rank = (e: FileEntry) => (e.kind === "dir" ? 0 : 1);
  return [...entries].sort(
    (a, b) =>
      rank(a) - rank(b) ||
      a.name.localeCompare(b.name, undefined, { sensitivity: "base", numeric: true }),
  );
}

export function formatSize(bytes: number | null): string {
  if (bytes === null) return "";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

export function formatTime(seconds: number | null): string {
  if (seconds === null || seconds === 0) return "";
  const d = new Date(seconds * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** 0o755 as "rwxr-xr-x". */
export function modeToText(mode: number): string {
  const bit = (shift: number, ch: string) => ((mode >> shift) & 1 ? ch : "-");
  return [6, 3, 0].map((s) => bit(s + 2, "r") + bit(s + 1, "w") + bit(s, "x")).join("");
}

/** Reads "755" or "0755" as permission bits, or null if it isn't one. */
export function parseMode(text: string): number | null {
  const t = text.trim();
  if (!/^[0-7]{3,4}$/.test(t)) return null;
  return parseInt(t, 8);
}

/** The percentage of a transfer that is done, or null if the size isn't known. */
export function percent(done: number, total: number | null): number | null {
  if (!total || total <= 0) return null;
  return Math.min(100, Math.floor((done / total) * 100));
}
