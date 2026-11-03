// The file panel's side of the backend: browsing, changing and transferring files.

import { Channel, invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { AppError, TerminalStatus } from "./index";

export type FileKind = "file" | "dir" | "symlink" | "other";

export interface FileEntry {
  name: string;
  path: string;
  kind: FileKind;
  size: number | null;
  /** Seconds since 1970. */
  modified: number | null;
  /** Unix permission bits, if the system has them. */
  permissions: number | null;
}

export interface OpenedFiles {
  id: string;
  /** Where to start looking. */
  home: string;
  /** Whether permissions can be changed here. */
  chmod: boolean;
  /** Whether "delete as root" (sudo) is offered. */
  root_delete: boolean;
  /** "/" for servers; a backslash or "/" for this computer. */
  separator: string;
}

export type JobState =
  | { state: "queued" }
  | { state: "asking" }
  | { state: "running" }
  | { state: "done" }
  | { state: "skipped" }
  | { state: "cancelled" }
  | { state: "failed"; error: AppError };

export interface TransferJob {
  id: number;
  kind: "copy" | "folder";
  name: string;
  from: string;
  to: string;
  state: JobState;
  done: number;
  total: number | null;
}

export type OverwriteChoice = "overwrite" | "skip" | "overwrite_all" | "skip_all";

export function filesOpenLocal(): Promise<OpenedFiles> {
  return invoke<OpenedFiles>("files_open_local");
}

/** The files of a server, on the session's connection. Questions arrive through onStatus. */
export function filesOpenSftp(
  operationId: string,
  savedSessionId: string,
  onStatus: (status: TerminalStatus) => void,
): Promise<OpenedFiles> {
  const channel = new Channel<TerminalStatus>();
  channel.onmessage = onStatus;
  return invoke<OpenedFiles>("files_open_sftp", { operationId, savedSessionId, onStatus: channel });
}

/**
 * The files of a saved FTP or FTPS session. Plain FTP is warned about, and an unchecked
 * certificate is asked about, through onStatus.
 */
export function filesOpenFtp(
  operationId: string,
  savedSessionId: string,
  onStatus: (status: TerminalStatus) => void,
): Promise<OpenedFiles> {
  const channel = new Channel<TerminalStatus>();
  channel.onmessage = onStatus;
  return invoke<OpenedFiles>("files_open_ftp", { operationId, savedSessionId, onStatus: channel });
}

/** The files inside a WSL distribution (Windows only). */
export function filesOpenWsl(distro: string): Promise<OpenedFiles> {
  return invoke<OpenedFiles>("files_open_wsl", { distro });
}

/**
 * Tells the page about files dragged in from the desktop. `hover` is called while they are over
 * the window, and `drop` when they are let go, both with the position in page pixels. Returns
 * a function that stops listening.
 */
export function onDesktopFileDrop(handlers: {
  hover: (x: number, y: number) => void;
  leave: () => void;
  drop: (paths: string[], x: number, y: number) => void;
}): Promise<() => void> {
  const scale = () => window.devicePixelRatio || 1;
  return getCurrentWebview().onDragDropEvent((event) => {
    const p = event.payload;
    if (p.type === "enter" || p.type === "over") handlers.hover(p.position.x / scale(), p.position.y / scale());
    else if (p.type === "drop") handlers.drop(p.paths, p.position.x / scale(), p.position.y / scale());
    else handlers.leave();
  });
}

export function filesClose(id: string): Promise<void> {
  return invoke<void>("files_close", { id });
}

export function filesList(id: string, path: string): Promise<FileEntry[]> {
  return invoke<FileEntry[]>("files_list", { id, path });
}

export function filesMkdir(id: string, path: string): Promise<void> {
  return invoke<void>("files_mkdir", { id, path });
}

export function filesRename(id: string, from: string, to: string): Promise<void> {
  return invoke<void>("files_rename", { id, from, to });
}

export function filesChmod(id: string, path: string, mode: number): Promise<void> {
  return invoke<void>("files_chmod", { id, path, mode });
}

/** How much deleting would remove, for the confirmation. */
export function filesDeletePlan(id: string, path: string): Promise<{ files: number; folders: number }> {
  return invoke("files_delete_plan", { id, path });
}

export function filesDelete(id: string, path: string): Promise<void> {
  return invoke<void>("files_delete", { id, path });
}

/**
 * Deletes with sudo on the server. `password` is the sudo password (empty if none is needed). It
 * is sent once and not kept.
 */
export function filesDeleteAsRoot(id: string, path: string, password: string): Promise<void> {
  return invoke<void>("files_delete_as_root", { id, path, password });
}

/** Queues a file or folder: fromPath on one side is copied into the folder toDir on the other. */
export function transferAdd(
  fromId: string,
  fromPath: string,
  toId: string,
  toDir: string,
): Promise<{ queued: number; skipped: string[] }> {
  return invoke("transfer_add", { fromId, fromPath, toId, toDir });
}

/** Calls onChange with the whole queue now, and again every time it changes. */
export function transfersSubscribe(onChange: (jobs: TransferJob[]) => void): Promise<void> {
  const channel = new Channel<TransferJob[]>();
  channel.onmessage = onChange;
  return invoke<void>("transfers_subscribe", { onChange: channel });
}

export function transferCancel(id: number): Promise<void> {
  return invoke<void>("transfer_cancel", { id });
}

export function transferCancelAll(): Promise<void> {
  return invoke<void>("transfer_cancel_all");
}

export function transferClear(): Promise<void> {
  return invoke<void>("transfer_clear");
}

export function transferAnswer(jobId: number, choice: OverwriteChoice): Promise<void> {
  return invoke<void>("transfer_answer", { jobId, choice });
}

export interface EditInfo {
  id: string;
  name: string;
  local_path: string;
}

/** What the watcher reports about a downloaded copy. */
export type EditEvent = { type: "changed" } | { type: "gone" };

/**
 * Downloads a server file, opens it on this computer and watches it. `onEvent` hears each time
 * it is saved.
 */
export function editOpen(
  fileId: string,
  path: string,
  onEvent: (event: EditEvent) => void,
  choose = false,
): Promise<EditInfo> {
  const channel = new Channel<EditEvent>();
  channel.onmessage = onEvent;
  return invoke<EditInfo>("edit_open", { fileId, path, choose, onEvent: channel });
}

/** Sends the edited copy back. "remote_changed" means the server's file changed meanwhile and nothing was sent. */
export function editUpload(id: string, force: boolean): Promise<"uploaded" | "remote_changed"> {
  return invoke("edit_upload", { id, force });
}

/** Opens the local copy again, or asks which program to use. */
export function editReopen(id: string, choose: boolean): Promise<void> {
  return invoke<void>("edit_reopen", { id, choose });
}

/** Stops watching and deletes the local copy. */
export function editStop(id: string): Promise<void> {
  return invoke<void>("edit_stop", { id });
}

/** Opens a file on this computer in the program the system uses for it. */
export function openLocalFile(path: string, choose = false): Promise<void> {
  return invoke<void>("open_local_file", { path, choose });
}
