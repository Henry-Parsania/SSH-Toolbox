// Which saved servers answer, kept up to date in the background.

import type { SavedSession } from "../backend";
import { reachCheck, type Reach, type ReachTarget } from "../backend/status";

/** The sessions worth checking: those with a host and port that are reached directly. */
export function targetsOf(sessions: SavedSession[]): ReachTarget[] {
  const out: ReachTarget[] = [];
  for (const s of sessions) {
    if (s.type === "ssh" && !s.jump) out.push({ id: s.id, host: s.host, port: s.port });
    else if (s.type === "ftp") out.push({ id: s.id, host: s.host, port: s.port });
  }
  return out;
}

export class Reachability {
  /** The latest answer for each session. A session with no answer yet isn't in here. */
  results = $state<Record<string, Reach>>({});
  checking = $state(false);
  private running = false;

  /** Checks the given sessions now. A check already under way is not doubled up. */
  async run(sessions: SavedSession[]): Promise<void> {
    const targets = targetsOf(sessions);
    if (this.running || targets.length === 0) return;
    this.running = true;
    this.checking = true;
    try {
      const found = await reachCheck(targets);
      const next: Record<string, Reach> = {};
      for (const r of found) next[r.id] = r;
      this.results = next;
    } catch {
      // A failed check leaves the old answers; it is not worth an error.
    } finally {
      this.running = false;
      this.checking = false;
    }
  }

  clear() {
    this.results = {};
  }
}
