// The settings the screen is using. Terminals read these and follow changes as they happen.

import { settingsGet, settingsSet, type Settings } from "./backend";

export const DEFAULT_SETTINGS: Settings = {
  version: 1,
  font_family: "monospace",
  font_size: 14,
  theme: "dark",
  scrollback: 10000,
  renderer: "auto",
  webkit_workaround: "off",
  default_shell: "",
  restore_layout: true,
  auto_reconnect: true,
  log_folder: "",
  tray_icon: false,
  close_to_tray: false,
  reachability_enabled: true,
  reachability_interval_secs: 60,
  stats_enabled: true,
  stats_interval_secs: 5,
};

/** Always has a value, so terminals can start before the file has been read. */
export const settings = $state<Settings>({ ...DEFAULT_SETTINGS });

export async function loadSettings(): Promise<void> {
  Object.assign(settings, await settingsGet());
}

/** Saves, then applies. If saving fails, nothing changes and the error goes to the caller. */
export async function saveSettings(next: Settings): Promise<void> {
  await settingsSet(next);
  Object.assign(settings, next);
}
