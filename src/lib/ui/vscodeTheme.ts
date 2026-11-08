// Turns a VS Code colour theme into an app theme. VS Code themes are JSON with comments and
// trailing commas, may "include" another theme, and name colours like "editor.background".

import { isHex, mix, type AppTheme, type ThemeColors } from "./themes";
import type { TerminalTheme } from "../terminal/themes";

/** Reads JSON that may contain comments and trailing commas, as VS Code files do. */
export function parseJsonc(text: string): unknown {
  let out = "";
  let i = 0;
  const s = text.replace(/^﻿/, "");
  while (i < s.length) {
    const c = s[i];
    if (c === '"') {
      let j = i + 1;
      while (j < s.length && s[j] !== '"') j += s[j] === "\\" ? 2 : 1;
      out += s.slice(i, j + 1);
      i = j + 1;
    } else if (c === "/" && s[i + 1] === "/") {
      while (i < s.length && s[i] !== "\n") i++;
    } else if (c === "/" && s[i + 1] === "*") {
      const end = s.indexOf("*/", i + 2);
      i = end < 0 ? s.length : end + 2;
    } else {
      out += c;
      i++;
    }
  }
  return JSON.parse(out.replace(/,(\s*[}\]])/g, "$1"));
}

/** `#rgb`, `#rgba` and `#rrggbbaa` all become `#rrggbb`; anything else is nothing. */
export function normalizeColor(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const v = value.trim();
  if (!/^#[0-9a-fA-F]+$/.test(v)) return null;
  let hex = v.slice(1);
  if (hex.length === 3 || hex.length === 4) hex = [...hex].map((ch) => ch + ch).join("");
  if (hex.length !== 6 && hex.length !== 8) return null;
  const out = `#${hex.slice(0, 6).toLowerCase()}`;
  return isHex(out) ? out : null;
}

interface RawTheme {
  include?: string;
  type?: string;
  colors?: Record<string, unknown>;
}

/** The first colour in `keys` that the theme sets. */
function pick(colors: Record<string, string>, ...keys: string[]): string | null {
  for (const k of keys) if (colors[k]) return colors[k];
  return null;
}

/**
 * Works out an app theme from VS Code's colour table. `uiTheme` is the extension's own word for
 * dark or light (`vs`, `vs-dark`, `hc-black`, `hc-light`).
 */
export function vscodeToTheme(name: string, uiTheme: string, raw: Record<string, unknown>, id: string): AppTheme {
  const colors: Record<string, string> = {};
  for (const [k, v] of Object.entries(raw)) {
    const hex = normalizeColor(v);
    if (hex) colors[k] = hex;
  }
  const light = uiTheme === "vs" || uiTheme === "hc-light";
  const bg = pick(colors, "editor.background", "terminal.background") ?? (light ? "#ffffff" : "#1e1e1e");
  const text = pick(colors, "editor.foreground", "foreground", "terminal.foreground") ?? (light ? "#333333" : "#d4d4d4");
  const panel =
    pick(colors, "sideBar.background", "activityBar.background", "editorWidget.background") ?? mix(bg, text, 0.05);
  const accent =
    pick(colors, "focusBorder", "button.background", "textLink.foreground", "activityBarBadge.background") ??
    (light ? "#2f6fe4" : "#6aa2ff");
  const go = pick(colors, "terminal.ansiGreen", "gitDecoration.addedResourceForeground", "charts.green") ?? (light ? "#1f9d63" : "#3fb97a");
  const danger = pick(colors, "errorForeground", "terminal.ansiRed", "editorError.foreground", "charts.red") ?? (light ? "#d33f3a" : "#f2706b");

  const ansi: Partial<Record<keyof TerminalTheme, string>> = {};
  const names = ["Black", "Red", "Green", "Yellow", "Blue", "Magenta", "Cyan", "White"];
  for (const n of names) {
    const lower = n[0].toLowerCase() + n.slice(1);
    const normal = colors[`terminal.ansi${n}`];
    const bright = colors[`terminal.ansiBright${n}`];
    if (normal) ansi[lower as keyof TerminalTheme] = normal;
    if (bright) ansi[`bright${n}` as keyof TerminalTheme] = bright;
  }
  const termBg = colors["terminal.background"];
  if (termBg && termBg !== bg) ansi.background = termBg;
  const termFg = colors["terminal.foreground"];
  if (termFg && termFg !== text) ansi.foreground = termFg;
  const cursor = colors["terminalCursor.foreground"];
  if (cursor) ansi.cursor = cursor;

  const themeColors: ThemeColors = { bg, panel, text, accent, go, danger };
  return {
    id,
    name: name.slice(0, 40),
    base: light ? "light" : "dark",
    colors: themeColors,
    terminal: ansi,
  };
}

/**
 * Reads a theme file and anything it includes. `read` fetches one file's text; `resolve` joins a
 * relative `include` onto the folder of the file that names it.
 */
export async function loadVsCodeColors(
  path: string,
  read: (path: string) => Promise<string>,
  resolve: (from: string, rel: string) => string,
  depth = 0,
): Promise<Record<string, unknown>> {
  if (depth > 4) return {};
  const raw = parseJsonc(await read(path)) as RawTheme;
  const base = raw.include ? await loadVsCodeColors(resolve(path, raw.include), read, resolve, depth + 1) : {};
  return { ...base, ...(raw.colors && typeof raw.colors === "object" ? raw.colors : {}) };
}

/** Joins a relative path (with `./` and `../`) onto the folder of `from`; works with either slash. */
export function resolveSibling(from: string, rel: string): string {
  const sep = from.includes("\\") ? "\\" : "/";
  const parts = from.split(/[\\/]/);
  parts.pop();
  for (const part of rel.split(/[\\/]/)) {
    if (part === "." || part === "") continue;
    if (part === "..") parts.pop();
    else parts.push(part);
  }
  return parts.join(sep);
}
