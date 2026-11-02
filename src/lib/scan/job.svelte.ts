// One scan as the screen sees it: what it found so far, how far it has got, and how it ended.

import { scanCancel, scanRun, type HostResult, type ScanRequest, type ScanSummary } from "../backend/scan";

/** A sort key that puts IPv4 addresses in numeric order and everything else after them. */
export function ipKey(ip: string): number {
  const parts = ip.split(".").map(Number);
  if (parts.length !== 4 || parts.some((n) => !Number.isInteger(n))) return Number.MAX_SAFE_INTEGER;
  return ((parts[0] * 256 + parts[1]) * 256 + parts[2]) * 256 + parts[3];
}

export class ScanJob {
  running = $state(false);
  hosts = $state<HostResult[]>([]);
  progress = $state({ done: 0, total: 0 });
  summary = $state<ScanSummary | null>(null);
  error = $state("");
  #id = "";

  /** Hosts in address order. */
  get sorted(): HostResult[] {
    return [...this.hosts].sort((a, b) => ipKey(a.ip) - ipKey(b.ip) || a.ip.localeCompare(b.ip));
  }

  async start(request: ScanRequest) {
    if (this.running) return;
    this.running = true;
    this.hosts = [];
    this.summary = null;
    this.error = "";
    this.progress = { done: 0, total: 0 };
    this.#id = crypto.randomUUID();
    try {
      this.summary = await scanRun(this.#id, request, (event) => {
        if (event.type === "host") {
          const { type: _type, ...host } = event;
          this.hosts.push(host);
        } else if (event.type === "progress") {
          this.progress = { done: event.done, total: event.total };
        }
      });
    } catch (e) {
      this.error = (e as { message?: string }).message ?? "The scan didn't work.";
    } finally {
      this.running = false;
    }
  }

  stop() {
    if (this.running) void scanCancel(this.#id).catch(() => {});
  }
}
