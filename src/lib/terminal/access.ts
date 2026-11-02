// Lets other parts of the app (the agent) read what a terminal is showing. Each terminal
// registers a reader under its backend session ID while it is open.

type Reader = (maxLines: number) => string;

const readers = new Map<string, Reader>();

/** Registers a terminal's reader. Returns a function that removes it. */
export function registerTerminalReader(sessionId: string, read: Reader): () => void {
  readers.set(sessionId, read);
  return () => {
    if (readers.get(sessionId) === read) readers.delete(sessionId);
  };
}

/** The last lines the terminal shows or has scrolled past, or null if it isn't open. */
export function readTerminalText(sessionId: string, maxLines = 80): string | null {
  return readers.get(sessionId)?.(maxLines) ?? null;
}
