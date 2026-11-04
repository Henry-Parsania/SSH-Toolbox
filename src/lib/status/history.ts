// Recent readings for the footer's small graphs.

/** How many readings each graph keeps. At one every 5 seconds that is five minutes. */
export const HISTORY_LENGTH = 60;

/** Adds a reading to the end of a list, dropping the oldest ones beyond the limit. */
export function pushReading(list: number[], value: number, limit = HISTORY_LENGTH): number[] {
  const next = [...list, value];
  return next.length > limit ? next.slice(next.length - limit) : next;
}

export interface NetSample {
  /** Milliseconds, from any clock that only goes forward. */
  at: number;
  rx: number;
  tx: number;
}

/**
 * The receive and send speed in bytes per second between two readings of the counters. Null when
 * the time between them is too short or the counters went backwards (a restart, or an interface
 * that was removed), because then the difference means nothing.
 */
export function netSpeed(before: NetSample | null, now: NetSample): { rx: number; tx: number } | null {
  if (!before) return null;
  const seconds = (now.at - before.at) / 1000;
  if (seconds < 0.5 || now.rx < before.rx || now.tx < before.tx) return null;
  return { rx: (now.rx - before.rx) / seconds, tx: (now.tx - before.tx) / seconds };
}
