// The only place that talks to Tauri. Everything else imports from here.
import { Channel, invoke } from "@tauri-apps/api/core";
// Only this folder may import from @tauri-apps (scripts/check-boundaries.mjs).
import { getCurrentWindow } from "@tauri-apps/api/window";

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

/** The version and exact build, for telling which build is running. */
export function appVersion(): Promise<string> {
  return invoke<string>("app_version");
}

export function platformInfo(): Promise<PlatformInfo> {
  return invoke<PlatformInfo>("platform_info");
}

export interface Shell {
  id: string;
  name: string;
}

/** What to open in a pane. More types are added in the phases that build them. */
export type SessionSpec =
  | { type: "local"; shell_id: string }
  | { type: "wsl"; distro: string }
  | { type: "ssh"; saved_session_id: string; monitor?: boolean }
  | { type: "serial"; saved_session_id: string }
  | { type: "docker_logs"; docker_id: string; container_id: string }
  | { type: "docker_shell"; docker_id: string; container_id: string };

export type TerminalStatus =
  | { type: "connecting" }
  | { type: "connected" }
  | { type: "exited"; code: number | null }
  | { type: "failed"; error: AppError }
  /** A question for the user. Answer it with `promptRespond`. */
  | { type: "prompt"; prompt_id: string; prompt: Prompt };

export type Prompt =
  | { kind: "host_key_unknown"; host: string; port: number; algorithm: string; fingerprint: string }
  | {
      kind: "host_key_new_type";
      host: string;
      port: number;
      algorithm: string;
      fingerprint: string;
      known_algorithms: string[];
    }
  | { kind: "password"; user: string; host: string; offer_save: boolean }
  | { kind: "passphrase"; key_path: string; offer_save: boolean }
  | {
      kind: "ftps_certificate";
      host: string;
      reason: string;
      fingerprint: string;
      subject: string;
      expires: string;
    }
  | { kind: "plain_ftp"; host: string };

export interface PromptResponse {
  accepted: boolean;
  /** What was typed, for password and passphrase prompts. */
  text?: string;
  /** Remember it in the keychain. */
  save?: boolean;
}

export function promptRespond(promptId: string, response: PromptResponse): Promise<void> {
  return invoke<void>("prompt_respond", { promptId, response });
}

/** Saves terminal text to a new file in Downloads. Returns where it went. */
export function terminalSaveOutput(text: string): Promise<string> {
  return invoke<string>("terminal_save_output", { text });
}

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
/** Shows an open session in this window instead of the one that was showing it. */
export function terminalAttach(
  sessionId: string,
  onOutput: (chunk: Uint8Array) => void,
  onStatus: (status: TerminalStatus) => void,
): Promise<string> {
  const output = new Channel<ArrayBuffer>();
  output.onmessage = (chunk) => onOutput(new Uint8Array(chunk));
  const status = new Channel<TerminalStatus>();
  status.onmessage = onStatus;
  return invoke<string>("terminal_attach", { sessionId, onOutput: output, onStatus: status });
}

/** Opens another window. `payload` says what it shows (see layout/detach.ts). */
export function windowOpen(title: string, payload: string): Promise<void> {
  return invoke<void>("window_open", { title, payload });
}

/** Closes the window this is called from. */
export function windowCloseSelf(): Promise<void> {
  return getCurrentWindow().close();
}

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
  /** Start an SSH session again by itself when its connection drops. */
  auto_reconnect: boolean;
  /** A folder for text copies of every session, or empty for none. */
  log_folder: string;
  reachability_enabled: boolean;
  /** 0 means only at start and when the list changes. */
  reachability_interval_secs: number;
  stats_enabled: boolean;
  stats_interval_secs: number;
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
  tabs: { id: string; title: string; active: string; root: SavedPane; view?: "files" | "docker" | "agent" }[];
  /** The panes the window is divided into, and which tab each shows. Missing in older layouts. */
  groups?: SavedPane;
  group_tabs?: Record<string, string>;
  active_group?: string;
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

export interface SavedFolder {
  id: string;
  name: string;
  /** Empty means top level. */
  parent: string;
}

export type SavedSession =
  | {
      type: "ssh";
      id: string;
      name: string;
      folder: string;
      host: string;
      port: number;
      user: string;
      auth: "password" | "key" | "agent";
      key_path: string;
      jump: string;
      x11: boolean;
      /** "#rrggbb", or empty to follow the theme. */
      color?: string;
    }
  | {
      type: "serial";
      id: string;
      name: string;
      folder: string;
      port: string;
      baud: number;
      data_bits: number;
      parity: "none" | "odd" | "even";
      stop_bits: number;
      flow_control: "none" | "hardware" | "software";
      local_echo: boolean;
      line_ending: "cr" | "lf" | "crlf";
    }
  | {
      type: "ftp";
      id: string;
      name: string;
      folder: string;
      host: string;
      port: number;
      user: string;
      security: "plain" | "explicit" | "implicit";
      trusted_cert_sha256: string;
    };

/** The saved-sessions file. It never holds a password or passphrase. */
export interface SessionsFile {
  version: number;
  folder: SavedFolder[];
  session: SavedSession[];
}

export interface LoadedSessions {
  file: SessionsFile;
  /** Set when the file exists but can't be used. Nothing is overwritten until the user decides. */
  problem: string | null;
}

export function sessionsGet(): Promise<LoadedSessions> {
  return invoke<LoadedSessions>("sessions_get");
}

export function sessionsSave(file: SessionsFile): Promise<void> {
  return invoke<void>("sessions_save", { file });
}

export function sessionsSetAsideUnreadable(): Promise<void> {
  return invoke<void>("sessions_set_aside_unreadable");
}

/** Whether saved passwords can be used. When false, hide "save password". */
export function keychainAvailable(): Promise<boolean> {
  return invoke<boolean>("keychain_available");
}

/** Saves a secret the user typed. The interface can never read one back. */
export function keychainSet(
  sessionId: string,
  kind: "password" | "passphrase" | "ftp_password",
  secret: string,
): Promise<void> {
  return invoke<void>("keychain_set", { sessionId, kind, secret });
}

/** Removes every saved secret of a session. Call this when a session is deleted. */
export function keychainDeleteSession(sessionId: string): Promise<void> {
  return invoke<void>("keychain_delete_session", { sessionId });
}

export interface NewKey {
  private_path: string;
  public_path: string;
  /** The line for `authorized_keys`. */
  public_line: string;
  fingerprint: string;
}

export interface ExistingKey {
  private_path: string;
  public_line: string;
}

/** Makes a new ed25519 key in ~/.ssh, optionally protected by a passphrase. */
export function keyGenerate(passphrase?: string): Promise<NewKey> {
  return invoke<NewKey>("key_generate", { passphrase: passphrase || null });
}

export function keyList(): Promise<ExistingKey[]> {
  return invoke<ExistingKey[]>("key_list");
}

export interface KeySetupResult {
  outcome: "installed" | "already_present";
  /** The key login was tried on a fresh connection and worked. */
  verified: boolean;
}

/**
 * Installs a public key on the server of a saved session and checks the key login. Questions
 * (host key, password) arrive through `onStatus` as prompts, to be answered with `promptRespond`.
 */
export function keySetup(
  operationId: string,
  savedSessionId: string,
  privateKeyPath: string,
  publicLine: string,
  onStatus: (status: TerminalStatus) => void,
): Promise<KeySetupResult> {
  const channel = new Channel<TerminalStatus>();
  channel.onmessage = onStatus;
  return invoke<KeySetupResult>("key_setup", {
    operationId,
    savedSessionId,
    privateKeyPath,
    publicLine,
    onStatus: channel,
  });
}

export function keySetupCancel(operationId: string): Promise<void> {
  return invoke<void>("key_setup_cancel", { operationId });
}

/** Removes one saved secret. Only after the user agrees. */
export function keychainDeleteSecret(
  sessionId: string,
  kind: "password" | "passphrase" | "ftp_password",
): Promise<void> {
  return invoke<void>("keychain_delete_secret", { sessionId, kind });
}

export interface ImportPreviewItem {
  alias: string;
  hostname: string;
  port: number;
  user: string;
  /** Already saved (same host, port and user), so it would be skipped. */
  duplicate: boolean;
  /** What it goes through, as written in the file, or empty. */
  jump: string;
}

export interface ImportResult {
  added: string[];
  /** Hosts left out, each with the reason. */
  skipped: string[];
}

/** The hosts in ~/.ssh/config. The file is only read. */
export function sshConfigPreview(): Promise<ImportPreviewItem[]> {
  return invoke<ImportPreviewItem[]>("ssh_config_preview");
}

export function sshConfigImport(aliases: string[]): Promise<ImportResult> {
  return invoke<ImportResult>("ssh_config_import", { aliases });
}

export type ForwardSpec = (
  | { type: "local"; dest_host: string; dest_port: number }
  | { type: "remote"; dest_host: string; dest_port: number }
  | { type: "socks" }
) & {
  /** Where to listen: this computer for local and SOCKS, the server for remote. */
  bind_host: string;
  /** 0 lets the system choose a free port. */
  bind_port: number;
};

export interface ForwardInfo {
  id: string;
  session_id: string;
  spec: ForwardSpec;
  /** The port really listening. */
  port: number;
}

/**
 * Starts a port forward on a saved session, connecting first if needed (questions arrive through
 * `onStatus`). `confirmExternal` is only for a listener open to the network, after warning.
 */
export function forwardStart(
  operationId: string,
  savedSessionId: string,
  spec: ForwardSpec,
  confirmExternal: boolean,
  onStatus: (status: TerminalStatus) => void,
): Promise<ForwardInfo> {
  const channel = new Channel<TerminalStatus>();
  channel.onmessage = onStatus;
  return invoke<ForwardInfo>("forward_start", {
    operationId,
    savedSessionId,
    spec,
    confirmExternal,
    onStatus: channel,
  });
}

export function forwardStop(id: string): Promise<void> {
  return invoke<void>("forward_stop", { id });
}

export function forwardStopSession(savedSessionId: string): Promise<void> {
  return invoke<void>("forward_stop_session", { savedSessionId });
}

export function forwardList(): Promise<ForwardInfo[]> {
  return invoke<ForwardInfo[]>("forward_list");
}

export interface Snippet {
  id: string;
  name: string;
  text: string;
  /** Press Enter after the text. */
  run: boolean;
}

export function snippetsGet(): Promise<{ version: number; snippet: Snippet[] }> {
  return invoke("snippets_get");
}

export function snippetsSet(snippets: Snippet[]): Promise<void> {
  return invoke("snippets_set", { snippets: { version: 1, snippet: snippets } });
}
