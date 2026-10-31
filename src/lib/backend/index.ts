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
export type SessionSpec = { type: "local"; shell_id: string };

export type TerminalStatus =
  | { type: "connecting" }
  | { type: "connected" }
  | { type: "exited"; code: number | null }
  | { type: "failed"; error: AppError };

export function listShells(): Promise<Shell[]> {
  return invoke<Shell[]>("list_shells");
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
