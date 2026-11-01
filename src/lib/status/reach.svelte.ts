// Which saved servers answer, kept up to date in the background.

import type { SavedSession } from "../backend";
import { reachCheck, type Reach } from "../backend/status";

/** Whether any saved session is one that gets checked (the backend picks them itself). */
export function hasTargets(sessions: SavedSession[]): boolean {
  return sessions.some((s) => (s.type === "ssh" && !s.jump) || s.type === "ftp");
}

export class Reachability {
  /** The latest answer for each session. A session with no answer yet isn't in here. */
  results = $state<Record<string, Reach>>({});
  checking = $state(false);
  private running = false;

  /** Checks the given sessions now. A check already under way is not doubled up. */
  async run(sessions: SavedSession[]): Promise<void> {
    if (this.running || !hasTargets(sessions)) return;
    this.running = true;
    this.checking = true;
    try {
      const found = await reachCheck();
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
