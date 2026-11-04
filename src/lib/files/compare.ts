// Comparing a folder on this computer with one on a server (task 7.11). Nothing is ever deleted:
// the result only says what is missing or newer on each side, and the user chooses what to copy.

export interface Item {
  size: number | null;
  /** Seconds since 1970. */
  modified: number | null;
}

export interface Comparison {
  /** Only on this computer. */
  onlyLocal: string[];
  /** Only on the server. */
  onlyRemote: string[];
  /** On both, and the copy on this computer is newer. */
  localNewer: string[];
  /** On both, and the copy on the server is newer. */
  remoteNewer: string[];
  /** On both but different in size, with no way to tell which is newer. */
  differs: string[];
  same: number;
}

/** Clocks and file systems round times differently, so a difference this small is ignored. */
const TIME_SLACK = 2;

export function compareFolders(local: Map<string, Item>, remote: Map<string, Item>): Comparison {
  const out: Comparison = { onlyLocal: [], onlyRemote: [], localNewer: [], remoteNewer: [], differs: [], same: 0 };
  for (const [path, l] of local) {
    const r = remote.get(path);
    if (!r) {
      out.onlyLocal.push(path);
      continue;
    }
    const sameSize = l.size === r.size;
    if (l.modified === null || r.modified === null) {
      if (sameSize) out.same++;
      else out.differs.push(path);
      continue;
    }
    const gap = l.modified - r.modified;
    if (sameSize && Math.abs(gap) <= TIME_SLACK) out.same++;
    // A different size with the same time can't be told apart by age.
    else if (Math.abs(gap) <= TIME_SLACK) out.differs.push(path);
    else if (gap > 0) out.localNewer.push(path);
    else out.remoteNewer.push(path);
  }
  for (const path of remote.keys()) if (!local.has(path)) out.onlyRemote.push(path);
  for (const list of [out.onlyLocal, out.onlyRemote, out.localNewer, out.remoteNewer, out.differs]) list.sort();
  return out;
}

/** The part of a path after the folder it was found in, always with "/" between names. */
export function relativeTo(root: string, full: string): string {
  const trimmed = root.replace(/[\\/]+$/, "");
  return full.slice(trimmed.length).replace(/^[\\/]+/, "").replaceAll("\\", "/");
}
