// Server status: which saved servers can be reached, and the footer's readings.

import { invoke } from "@tauri-apps/api/core";

export interface ReachTarget {
  id: string;
  host: string;
  port: number;
}

export interface Reach {
  id: string;
  reachable: boolean;
  /** How long the connection took, when it worked. */
  millis: number | null;
}

export interface Stats {
  /** 0 to 100. Missing until two readings exist. */
  cpu_percent: number | null;
  mem_used: number;
  mem_total: number;
  disk_used: number;
  disk_total: number;
  load: string | null;
  uptime_secs: number | null;
}

/** Tries a quick connection to each server's port. */
export function reachCheck(targets: ReachTarget[]): Promise<Reach[]> {
  return invoke<Reach[]>("reach_check", { targets });
}

export function statsLocal(): Promise<Stats> {
  return invoke<Stats>("stats_local");
}

/** The readings of a connected SSH session's server, or null if it isn't connected or isn't Linux. */
export async function statsRemote(savedSessionId: string): Promise<Stats | null> {
  const result = await invoke<{ stats: Stats | null }>("stats_remote", { savedSessionId });
  return result.stats;
}
