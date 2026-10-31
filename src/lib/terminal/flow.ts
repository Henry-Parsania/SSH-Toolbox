// Flow control and input ordering for a terminal (INTERFACES.md section 4).

/** Tell the backend about processed bytes once this many have built up. */
export const ACK_THRESHOLD = 32 * 1024;
/** Or this long after the first unreported byte, whichever comes first. */
export const ACK_DELAY_MS = 50;

/**
 * Collects "xterm has processed N bytes" reports and passes them on in batches, so the backend
 * isn't sent an acknowledgement for every small chunk. The backend stops sending when too many
 * bytes are unacknowledged, so a report is never held back for more than `delayMs`.
 */
export class AckBatcher {
  private pending = 0;
  private timer: ReturnType<typeof setTimeout> | null = null;

  constructor(
    private readonly send: (bytes: number) => void,
    private readonly threshold = ACK_THRESHOLD,
    private readonly delayMs = ACK_DELAY_MS,
  ) {}

  processed(bytes: number): void {
    if (bytes <= 0) return;
    this.pending += bytes;
    if (this.pending >= this.threshold) {
      this.flush();
    } else if (this.timer === null) {
      this.timer = setTimeout(() => this.flush(), this.delayMs);
    }
  }

  flush(): void {
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    if (this.pending > 0) {
      const bytes = this.pending;
      this.pending = 0;
      this.send(bytes);
    }
  }

  /** Stops the timer without sending anything more. */
  dispose(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    this.pending = 0;
  }
}

/**
 * Sends keystrokes to the backend strictly in order, one request at a time. Separate requests
 * can reach the shell out of order, which would scramble fast typing. Bytes that arrive while
 * a request is in flight, or before `enable` is called, are joined and sent in the next one.
 */
export class InputSender {
  private queue: Uint8Array[] = [];
  private enabled = false;
  private busy = false;

  constructor(private readonly write: (data: Uint8Array) => Promise<void>) {}

  push(data: Uint8Array): void {
    if (data.length === 0) return;
    this.queue.push(data);
    void this.drain();
  }

  /** Starts sending. Call once the session ID is known. */
  enable(): void {
    this.enabled = true;
    void this.drain();
  }

  /** Drops anything still waiting, for when the session has ended. */
  stop(): void {
    this.enabled = false;
    this.queue = [];
  }

  private async drain(): Promise<void> {
    if (!this.enabled || this.busy) return;
    this.busy = true;
    try {
      while (this.enabled && this.queue.length > 0) {
        const parts = this.queue;
        this.queue = [];
        const joined = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
        let offset = 0;
        for (const part of parts) {
          joined.set(part, offset);
          offset += part.length;
        }
        try {
          await this.write(joined);
        } catch {
          // The session is gone. Keys typed now have nowhere to go.
          this.queue = [];
        }
      }
    } finally {
      this.busy = false;
    }
  }
}
