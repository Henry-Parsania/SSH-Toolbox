// The only place that talks to Tauri. Everything else imports from here.
import { Channel, invoke } from "@tauri-apps/api/core";

export type ErrorCode =
  | "not_found"
  | "permission_denied"
  | "connection_failed"
  | "connection_lost"
  | "auth_failed"
  | "host_key_changed"
  | "host_key_rejected"
  | "certificate_rejected"
  | "keychain_unavailable"
  | "already_exists"
  | "cancelled"
  | "unsupported"
  | "invalid_input"
  | "internal";

export interface AppError {
  code: ErrorCode;
  message: string;
}

export interface PlatformInfo {
  os: "windows" | "linux" | "macos";
  wsl_available: boolean;
  windows_build: number | null;
}

export function platformInfo(): Promise<PlatformInfo> {
  return invoke<PlatformInfo>("platform_info");
}

export interface Shell {
  id: string;
  name: string;
}

/** What to open in a pane. More types are added in the phases that build them. */
export type SessionSpec = { type: "local"; shell_id: string } | { type: "wsl"; distro: string };

export type TerminalStatus =
  | { type: "connecting" }
  | { type: "connected" }
  | { type: "exited"; code: number | null }
  | { type: "failed"; error: AppError };

export function listShells(): Promise<Shell[]> {
  return invoke<Shell[]>("list_shells");
}

/** Installed WSL distributions, or none when WSL isn't there. */
export function listWslDistros(): Promise<string[]> {
  return invoke<string[]>("list_wsl_distros");
}

/**
 * Opens a terminal and returns its session ID. Output arrives as raw bytes, so a character
 * split across two chunks isn't broken: pass the chunks straight to xterm's `write`.
 */
export function terminalOpen(
  spec: SessionSpec,
  cols: number,
  rows: number,
  onOutput: (chunk: Uint8Array) => void,
  onStatus: (status: TerminalStatus) => void,
): Promise<string> {
  const output = new Channel<ArrayBuffer>();
  output.onmessage = (chunk) => onOutput(new Uint8Array(chunk));
  const status = new Channel<TerminalStatus>();
  status.onmessage = onStatus;
  return invoke<string>("terminal_open", { spec, cols, rows, onOutput: output, onStatus: status });
}

export function terminalWrite(sessionId: string, data: Uint8Array): Promise<void> {
  return invoke<void>("terminal_write", { sessionId, data: Array.from(data) });
}

export function terminalResize(sessionId: string, cols: number, rows: number): Promise<void> {
  return invoke<void>("terminal_resize", { sessionId, cols, rows });
}

/** Tells the backend that `bytes` of output have been processed (flow control). */
export function terminalAck(sessionId: string, bytes: number): Promise<void> {
  return invoke<void>("terminal_ack", { sessionId, bytes });
}

export function terminalClose(sessionId: string): Promise<void> {
  return invoke<void>("terminal_close", { sessionId });
}

export interface Settings {
  version: number;
  font_family: string;
  font_size: number;
  theme: "dark" | "light";
  scrollback: number;
  renderer: "auto" | "dom" | "webgl";
  webkit_workaround: "off" | "disable_dmabuf" | "disable_compositing";
  default_shell: string;
  restore_layout: boolean;
}

export function settingsGet(): Promise<Settings> {
  return invoke<Settings>("settings_get");
}

export function settingsSet(settings: Settings): Promise<void> {
  return invoke<void>("settings_set", { settings });
}

/** Opens a web link in the browser. The backend refuses anything but http and https. */
export function openLink(url: string): Promise<void> {
  return invoke<void>("open_link", { url });
}

/** The layout as it is saved: tabs and their pane trees. */
export interface SavedLayout {
  version: number;
  active_tab: string;
  tabs: { id: string; title: string; active: string; root: SavedPane }[];
}

export type SavedPane =
  | { kind: "leaf"; pane_id: string; spec?: SessionSpec | null; title: string }
  | { kind: "split"; direction: "row" | "column"; sizes: number[]; children: SavedPane[] };

export function layoutGet(): Promise<SavedLayout | null> {
  return invoke<SavedLayout | null>("layout_get");
}

export function layoutSet(layout: SavedLayout): Promise<void> {
  return invoke<void>("layout_set", { layout });
}

/** Marks the first terminal output, so the backend can log how long startup took (Q3). */
export function startupReady(): Promise<void> {
  return invoke<void>("startup_ready");
}
