// Noticing that a long-running command has finished in a tab you aren't looking at (task 7.9).
// There is no way to know a command ended, so this watches the output: a long stretch of output
// followed by quiet is treated as "it finished".

export class FinishDetector {
  private burstStart = 0;
  private last = 0;
  private armed = false;

  constructor(
    /** How long output must have been going before quiet counts as finished. */
    private readonly burstMs = 8000,
    /** How long the output must then stay quiet. */
    private readonly quietMs = 3000,
    /** A pause longer than this starts a new burst. */
    private readonly gapMs = 3000,
  ) {}

  /** Call for every chunk of output. */
  chunk(now: number): void {
    if (now - this.last > this.gapMs) this.burstStart = now;
    this.last = now;
    this.armed = now - this.burstStart >= this.burstMs;
  }

  /** Call now and then. True once per long burst of output, after it has gone quiet. */
  check(now: number): boolean {
    if (!this.armed || now - this.last < this.quietMs) return false;
    this.armed = false;
    return true;
  }
}
