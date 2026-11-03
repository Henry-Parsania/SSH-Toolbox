// Server files that are open on this computer: when one is saved, it is sent back, either
// straight away or after the user says yes.

import { editOpen, editReopen, editStop, editUpload, type EditEvent, type EditInfo } from "../backend/files";

export type EditState = "watching" | "waiting" | "sending" | "sent" | "failed";

export interface EditItem {
  id: string;
  name: string;
  remotePath: string;
  state: EditState;
  /** A sentence about how it went, if there is one. */
  message: string;
  /** When it was last sent, in milliseconds since 1970. */
  sentAt: number | null;
}

/** What is being asked: whether to send a saved file, or whether to replace a server file that changed. */
export interface Question {
  id: string;
  kind: "send" | "overwrite";
}

export interface EditDeps {
  open(fileId: string, path: string, onEvent: (event: EditEvent) => void, choose?: boolean): Promise<EditInfo>;
  upload(id: string, force: boolean): Promise<"uploaded" | "remote_changed">;
  reopen(id: string, choose: boolean): Promise<void>;
  stop(id: string): Promise<void>;
}

export const realEditDeps: EditDeps = { open: editOpen, upload: editUpload, reopen: editReopen, stop: editStop };

const MODE_KEY = "edit-upload-mode";

function savedMode(): "ask" | "auto" {
  try {
    return localStorage.getItem(MODE_KEY) === "auto" ? "auto" : "ask";
  } catch {
    return "ask";
  }
}

const sayError = (e: unknown, fallback: string) => (e as { message?: string })?.message ?? fallback;

export class EditSync {
  items = $state<EditItem[]>([]);
  /** Whether a saved file is sent back by itself, or only after asking. */
  mode = $state<"ask" | "auto">(savedMode());
  questions = $state<Question[]>([]);
  /** Saves that arrived while the file was being sent, so it is sent once more afterwards. */
  #again = new Set<string>();
  #deps: EditDeps;

  constructor(deps: EditDeps = realEditDeps) {
    this.#deps = deps;
  }

  setMode(mode: "ask" | "auto") {
    this.mode = mode;
    try {
      localStorage.setItem(MODE_KEY, mode);
    } catch {
      // The choice then lasts until the app closes.
    }
  }

  #item(id: string): EditItem | undefined {
    return this.items.find((i) => i.id === id);
  }

  #ask(id: string, kind: Question["kind"]) {
    this.questions = [...this.questions.filter((q) => q.id !== id), { id, kind }];
  }

  /** Downloads the file and opens it on this computer. Returns its edit ID. */
  async open(fileId: string, path: string, choose = false): Promise<string> {
    let id: string | null = null;
    const early: EditEvent[] = [];
    const info = await this.#deps.open(fileId, path, (event) => {
      if (id) void this.#onEvent(id, event);
      else early.push(event);
    }, choose);
    id = info.id;
    this.items.push({ id, name: info.name, remotePath: path, state: "watching", message: "", sentAt: null });
    for (const event of early) void this.#onEvent(id, event);
    return id;
  }

  async #onEvent(id: string, event: EditEvent) {
    const item = this.#item(id);
    if (!item) return;
    if (event.type === "gone") {
      item.state = "failed";
      item.message = "The copy on this computer was deleted, so changes are not watched any more.";
      this.questions = this.questions.filter((q) => q.id !== id);
      return;
    }
    if (item.state === "sending") {
      this.#again.add(id);
      return;
    }
    if (this.mode === "auto") await this.#send(id, false);
    else {
      item.state = "waiting";
      item.message = "Saved here. Not sent to the server yet.";
      this.#ask(id, "send");
    }
  }

  async #send(id: string, force: boolean) {
    const item = this.#item(id);
    if (!item) return;
    item.state = "sending";
    item.message = "";
    try {
      const result = await this.#deps.upload(id, force);
      if (result === "remote_changed") {
        item.state = "waiting";
        item.message = "The file on the server changed after you opened it. Nothing was sent.";
        this.#ask(id, "overwrite");
        return;
      }
      item.state = "sent";
      item.sentAt = Date.now();
      item.message = "";
    } catch (e) {
      item.state = "failed";
      item.message = sayError(e, "Could not send the file.");
      return;
    }
    if (this.#again.delete(id)) await this.#send(id, false);
  }

  /** Sends the file now, whatever the mode. */
  async sendNow(id: string) {
    this.questions = this.questions.filter((q) => q.id !== id);
    await this.#send(id, false);
  }

  /** The user's answer to a question. */
  async answer(id: string, choice: "send" | "later" | "always") {
    const question = this.questions.find((q) => q.id === id);
    this.questions = this.questions.filter((q) => q.id !== id);
    const item = this.#item(id);
    if (!question || !item) return;
    if (choice === "later") {
      item.state = "watching";
      item.message = question.kind === "overwrite" ? "Not sent: the server's file is different." : "Not sent.";
      return;
    }
    if (choice === "always" && question.kind === "send") this.setMode("auto");
    await this.#send(id, question.kind === "overwrite");
  }

  async reopen(id: string, choose: boolean) {
    const item = this.#item(id);
    try {
      await this.#deps.reopen(id, choose);
    } catch (e) {
      if (item) item.message = sayError(e, "Could not open the file.");
    }
  }

  /** Stops watching one file and deletes the copy on this computer. */
  async stop(id: string) {
    this.items = this.items.filter((i) => i.id !== id);
    this.questions = this.questions.filter((q) => q.id !== id);
    this.#again.delete(id);
    await this.#deps.stop(id).catch(() => {});
  }
}

/** The files being edited, for the whole app: they stay watched when the file panel is closed. */
export const edits = new EditSync();
