// The sidebar's tree of saved sessions: building it, searching it and checking the editor's form.

import type { SavedFolder, SavedSession, SessionsFile } from "../backend";

export type SshSession = Extract<SavedSession, { type: "ssh" }>;
export type FtpSession = Extract<SavedSession, { type: "ftp" }>;

export type TreeItem =
  | { kind: "folder"; folder: SavedFolder; children: TreeItem[] }
  | { kind: "session"; session: SavedSession };

/** What the search box matches: the name and, for connections, the host. */
function haystack(session: SavedSession): string {
  const place = session.type === "serial" ? session.port : session.host;
  return `${session.name} ${place}`.toLowerCase();
}

function byName(a: TreeItem, b: TreeItem): number {
  const nameOf = (i: TreeItem) => (i.kind === "folder" ? i.folder.name : i.session.name);
  return nameOf(a).localeCompare(nameOf(b), undefined, { sensitivity: "base" });
}

/**
 * Builds the tree, folders first, each level sorted by name. With a search, only matching
 * sessions are shown, together with the folders that hold them. A session whose folder is missing
 * is shown at the top level rather than lost.
 */
export function buildTree(file: Pick<SessionsFile, "folder" | "session">, search = ""): TreeItem[] {
  const query = search.trim().toLowerCase();
  const folderIds = new Set(file.folder.map((f) => f.id));

  const make = (parent: string): TreeItem[] => {
    const folders: TreeItem[] = file.folder
      .filter((f) => (folderIds.has(f.parent) ? f.parent : "") === parent)
      .map((folder) => ({ kind: "folder" as const, folder, children: make(folder.id) }))
      .sort(byName);
    const sessions: TreeItem[] = file.session
      .filter((s) => (folderIds.has(s.folder) ? s.folder : "") === parent)
      .filter((s) => !query || haystack(s).includes(query))
      .map((session) => ({ kind: "session" as const, session }))
      .sort(byName);
    const shownFolders = query
      ? folders.filter((f) => f.kind === "folder" && f.children.length > 0)
      : folders;
    return [...shownFolders, ...sessions];
  };
  return make("");
}

/** The folders a session can be put in, with their path as the label, for a menu. */
export function folderChoices(folders: SavedFolder[]): { id: string; label: string }[] {
  const byId = new Map(folders.map((f) => [f.id, f]));
  const label = (f: SavedFolder, seen = new Set<string>()): string => {
    const parent = byId.get(f.parent);
    if (!parent || seen.has(f.id)) return f.name;
    seen.add(f.id);
    return `${label(parent, seen)} / ${f.name}`;
  };
  return folders
    .map((f) => ({ id: f.id, label: label(f) }))
    .sort((a, b) => a.label.localeCompare(b.label, undefined, { sensitivity: "base" }));
}

export interface SshForm {
  name: string;
  host: string;
  port: string | number;
  user: string;
  auth: "password" | "key" | "agent";
  key_path: string;
}

/** The message for the first thing wrong with the form, or null if it is fine. */
export function checkSshForm(form: SshForm): string | null {
  if (!form.name.trim()) return "Give the session a name.";
  const host = form.host.trim();
  if (!host) return "Enter the host.";
  if (/\s/.test(host)) return "The host can't contain spaces.";
  const port = Number(form.port);
  if (!Number.isInteger(port) || port < 1 || port > 65535) return "The port must be between 1 and 65535.";
  if (!form.user.trim()) return "Enter the user name.";
  if (form.auth === "key" && !form.key_path.trim()) return "Choose the private key file.";
  return null;
}

/** The message for the first thing wrong with an FTP form, or null if it is fine. */
export function checkFtpForm(form: { name: string; host: string; port: string | number }): string | null {
  if (!form.name.trim()) return "Give the session a name.";
  const host = form.host.trim();
  if (!host) return "Enter the host.";
  if (/\s/.test(host)) return "The host can't contain spaces.";
  const port = Number(form.port);
  if (!Number.isInteger(port) || port < 1 || port > 65535) return "The port must be between 1 and 65535.";
  return null;
}

/** A new folder or session ID. */
export function newId(): string {
  return crypto.randomUUID();
}

/** Removes a folder. Everything inside moves up to the folder's own parent, so nothing is lost. */
export function removeFolder(file: SessionsFile, folderId: string): SessionsFile {
  const gone = file.folder.find((f) => f.id === folderId);
  if (!gone) return file;
  return {
    ...file,
    folder: file.folder
      .filter((f) => f.id !== folderId)
      .map((f) => (f.parent === folderId ? { ...f, parent: gone.parent } : f)),
    session: file.session.map((s) => (s.folder === folderId ? { ...s, folder: gone.parent } : s)),
  };
}

/**
 * Removes a session. Any session that used it as a jump host loses that setting, because the
 * file would otherwise point at something that isn't there.
 */
export function removeSession(file: SessionsFile, sessionId: string): SessionsFile {
  return {
    ...file,
    session: file.session
      .filter((s) => s.id !== sessionId)
      .map((s) => (s.type === "ssh" && s.jump === sessionId ? { ...s, jump: "" } : s)),
  };
}

/** True if moving `folderId` inside `parentId` would put a folder inside itself. */
export function wouldLoop(folders: SavedFolder[], folderId: string, parentId: string): boolean {
  const byId = new Map(folders.map((f) => [f.id, f]));
  let at = parentId;
  const seen = new Set<string>();
  while (at && !seen.has(at)) {
    if (at === folderId) return true;
    seen.add(at);
    at = byId.get(at)?.parent ?? "";
  }
  return false;
}
