// The theme in use and the themes the person made. Made-up themes are kept in the app's own
// storage (they aren't secrets and are only colours), and survive updates.

import { settings } from "../settings.svelte";
import { BUILTIN_THEMES, cleanTheme, themeVariables, type AppTheme } from "./themes";

const KEY = "themes";

function read(): { active: string; custom: AppTheme[] } {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? "null");
    const custom = Array.isArray(raw?.custom) ? raw.custom.map(cleanTheme).filter((t: AppTheme | null): t is AppTheme => !!t) : [];
    return { active: typeof raw?.active === "string" ? raw.active : "", custom };
  } catch {
    return { active: "", custom: [] };
  }
}

export const themeStore = $state(read());

function persist() {
  try {
    localStorage.setItem(KEY, JSON.stringify($state.snapshot(themeStore)));
  } catch {
    // Not saving only means the themes are gone next time.
  }
}

/** A theme shown while the person is still choosing or editing, not yet saved. */
const preview = $state<{ theme: AppTheme | null }>({ theme: null });

export function setPreview(theme: AppTheme | null) {
  preview.theme = theme ? structuredClone($state.snapshot(theme)) : null;
}

export function allThemes(): AppTheme[] {
  return [...BUILTIN_THEMES, ...themeStore.custom];
}

/** The theme in use: the chosen one, or the plain dark or light one that the old setting names. */
export function currentTheme(): AppTheme {
  if (preview.theme) return preview.theme;
  const found = allThemes().find((t) => t.id === themeStore.active);
  return found ?? BUILTIN_THEMES.find((t) => t.id === settings.theme) ?? BUILTIN_THEMES[0];
}

export function chooseTheme(id: string) {
  themeStore.active = id;
  persist();
}

export function saveCustomTheme(theme: AppTheme) {
  const at = themeStore.custom.findIndex((t) => t.id === theme.id);
  const clean = { ...theme, builtin: undefined };
  if (at >= 0) themeStore.custom[at] = clean;
  else themeStore.custom.push(clean);
  persist();
}

export function deleteCustomTheme(id: string) {
  themeStore.custom = themeStore.custom.filter((t) => t.id !== id);
  if (themeStore.active === id) themeStore.active = "";
  persist();
}

export function newThemeId(): string {
  return `custom-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
}

const KNOWN = Object.keys(themeVariables(BUILTIN_THEMES[0]));

/** Puts a theme on the whole window. The two original themes come from theme.css; the others set variables. */
export function applyTheme(theme: AppTheme) {
  const root = document.documentElement;
  root.dataset.theme = theme.base;
  if (theme.id === "dark" || theme.id === "light") {
    for (const name of KNOWN) root.style.removeProperty(name);
    return;
  }
  for (const [name, value] of Object.entries(themeVariables(theme))) root.style.setProperty(name, value);
}
