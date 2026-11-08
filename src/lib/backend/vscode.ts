// VS Code's colour themes, found on this computer.

import { invoke } from "@tauri-apps/api/core";

export interface VsCodeThemeInfo {
  label: string;
  ui_theme: string;
  path: string;
  source: string;
}

export function vscodeThemes(): Promise<{ themes: VsCodeThemeInfo[]; current: string | null }> {
  return invoke("vscode_themes");
}

/** Reads one theme file from inside a VS Code folder. */
export function vscodeThemeRead(path: string): Promise<string> {
  return invoke<string>("vscode_theme_read", { path });
}
