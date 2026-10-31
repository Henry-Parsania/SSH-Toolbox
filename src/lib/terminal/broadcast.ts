// Typing into all panes of a tab at once (P5). Each terminal registers a way to receive input
// here; when broadcasting is on, what is typed in one pane is also sent to the others in the
// same tab.

type Receiver = (data: Uint8Array) => void;

const receivers = new Map<string, Map<string, Receiver>>();

/** Registers a pane. Returns a function that removes it. */
export function registerReceiver(tabId: string, paneId: string, receive: Receiver): () => void {
  let tab = receivers.get(tabId);
  if (!tab) receivers.set(tabId, (tab = new Map()));
  tab.set(paneId, receive);
  return () => {
    const current = receivers.get(tabId);
    current?.delete(paneId);
    if (current?.size === 0) receivers.delete(tabId);
  };
}

/** Sends input typed in `fromPane` to every other pane in the tab. */
export function broadcastInput(tabId: string, fromPane: string, data: Uint8Array): void {
  for (const [paneId, receive] of receivers.get(tabId) ?? []) {
    if (paneId !== fromPane) receive(data);
  }
}
