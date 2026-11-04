// When to start a dropped SSH session again by itself (task 7.4).

export const MAX_RETRIES = 5;

export interface RetryInput {
  /** The setting. */
  enabled: boolean;
  isSsh: boolean;
  /** The session had worked before it dropped. */
  wasConnected: boolean;
  /** The reason was a lost connection, not a refusal or a wrong password. */
  lost: boolean;
  /** How many automatic tries have been made since it last worked. */
  retries: number;
}

/** Seconds to wait before trying again (2, 4, 8, 16, 30), or null to stop and let the user decide. */
export function nextRetryDelay(input: RetryInput): number | null {
  if (!input.enabled || !input.isSsh || !input.wasConnected || !input.lost) return null;
  if (input.retries >= MAX_RETRIES) return null;
  return Math.min(30, 2 ** (input.retries + 1));
}
