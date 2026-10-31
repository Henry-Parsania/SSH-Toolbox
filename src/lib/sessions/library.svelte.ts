// The saved sessions the sidebar shows, and the changes made to them.

import {
  keychainAvailable,
  forwardStopSession,
  keychainDeleteSession,
  sessionsGet,
  sessionsSave,
  sessionsSetAsideUnreadable,
  type SessionsFile,
} from "../backend";
import { removeFolder, removeSession } from "./tree";

export class Library {
  file = $state<SessionsFile>({ version: 1, folder: [], session: [] });
  /** Set when the saved file can't be used. Nothing is overwritten until the user chooses. */
  problem = $state<string | null>(null);
  /** Whether saved passwords can be used. Hides "save password" when false. */
  keychain = $state(false);
  loaded = $state(false);

  async load(): Promise<void> {
    const result = await sessionsGet();
    this.file = result.file;
    this.problem = result.problem;
    this.keychain = await keychainAvailable().catch(() => false);
    this.loaded = true;
  }

  /** Saves, then shows the change. If saving fails, nothing changes and the error goes up. */
  async save(next: SessionsFile): Promise<void> {
    await sessionsSave(next);
    this.file = next;
  }

  async deleteSession(id: string): Promise<void> {
    // A deleted session can't keep a port open.
    await forwardStopSession(id).catch(() => {});
    await this.save(removeSession(this.file, id));
    // Deleting a session deletes its saved secrets. A keychain that isn't there is no problem.
    await keychainDeleteSession(id).catch(() => {});
  }

  async deleteFolder(id: string): Promise<void> {
    await this.save(removeFolder(this.file, id));
  }

  /** The user chose to start again: the old file is kept as `sessions.toml.bad`. */
  async startAgain(): Promise<void> {
    await sessionsSetAsideUnreadable();
    await this.load();
  }
}
