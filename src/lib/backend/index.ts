// The only place that talks to Tauri. Everything else imports from here.
import { invoke } from "@tauri-apps/api/core";

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
