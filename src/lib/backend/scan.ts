// The network and port scanners.

import { Channel, invoke } from "@tauri-apps/api/core";

export interface ScanRequest {
  /** Addresses, networks (192.168.1.0/24), ranges (192.168.1.10-50) or names, separated by commas or spaces. */
  targets: string;
  /** Ports such as 22,80,8000-8100, or the words common and all. */
  ports: string;
  timeout_ms: number;
  /** Listen briefly on each open port for the greeting some services send. */
  banners: boolean;
  /** Report only hosts that answered. */
  only_alive: boolean;
}

export interface OpenPort {
  port: number;
  service: string | null;
  banner: string | null;
}

export interface HostResult {
  ip: string;
  alive: boolean;
  open: OpenPort[];
  /** How many ports answered "closed". */
  closed: number;
}

export type ScanEvent =
  | { type: "started"; hosts: number; ports: number }
  | ({ type: "host" } & HostResult)
  | { type: "progress"; done: number; total: number };

export interface ScanSummary {
  hosts: number;
  up: number;
  open_ports: number;
  cancelled: boolean;
}

export function scanLocalNetworks(): Promise<string[]> {
  return invoke<string[]>("scan_local_networks");
}

/** Runs a scan. Resolves when it is over; onEvent hears about each host and the progress. */
export function scanRun(scanId: string, request: ScanRequest, onEvent: (e: ScanEvent) => void): Promise<ScanSummary> {
  const channel = new Channel<ScanEvent>();
  channel.onmessage = onEvent;
  return invoke<ScanSummary>("scan_run", { scanId, request, onEvent: channel });
}

export function scanCancel(scanId: string): Promise<void> {
  return invoke<void>("scan_cancel", { scanId });
}
