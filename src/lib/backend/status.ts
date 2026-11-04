// Server status: which saved servers can be reached, and the footer's readings.

import { invoke } from "@tauri-apps/api/core";

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
  /** Bytes received and sent since the computer started (all connections but the loopback). */
  net_rx_bytes: number | null;
  net_tx_bytes: number | null;
  /** The busiest graphics card's share in use, and the memory of all cards. NVIDIA only. */
  gpu_percent: number | null;
  gpu_mem_used: number | null;
  gpu_mem_total: number | null;
}

/** Tries a quick connection to the port of each saved server. */
export function reachCheck(): Promise<Reach[]> {
  return invoke<Reach[]>("reach_check");
}

export function statsLocal(): Promise<Stats> {
  return invoke<Stats>("stats_local");
}

/** The readings of a connected SSH session's server, or null if it isn't connected or isn't Linux. */
export async function statsRemote(savedSessionId: string): Promise<Stats | null> {
  const result = await invoke<{ stats: Stats | null }>("stats_remote", { savedSessionId });
  return result.stats;
}
