// App themes: a few colours in, a whole palette out. The two original themes ("dark" and "light")
// stay in theme.css; every other theme is worked out from six colours and set as CSS variables.

import { terminalTheme, type TerminalTheme } from "../terminal/themes";

export interface ThemeColors {
  /** The terminal and window background, the darkest (or lightest) surface. */
  bg: string;
  /** Side panels, a little lifted from the background. */
  panel: string;
  text: string;
  accent: string;
  /** Connect, start and "ok". */
  go: string;
  danger: string;
}

export interface AppTheme {
  id: string;
  name: string;
  /** Whether the window is mostly dark or mostly light: decides the defaults for what isn't picked. */
  base: "dark" | "light";
  colors: ThemeColors;
  /** Terminal colours to use in place of the worked-out ones (any of the 16, by name). */
  terminal?: Partial<TerminalTheme>;
  builtin?: boolean;
}

export const COLOR_LABELS: { key: keyof ThemeColors; label: string; hint: string }[] = [
  { key: "bg", label: "Background", hint: "The terminal and window" },
  { key: "panel", label: "Panels", hint: "Sessions, files and dialogs" },
  { key: "text", label: "Text", hint: "Main text" },
  { key: "accent", label: "Accent", hint: "Selection and focus" },
  { key: "go", label: "Go", hint: "Connect and start buttons" },
  { key: "danger", label: "Danger", hint: "Errors and delete" },
];

// ---- Colour maths ------------------------------------------------------------------------

export function isHex(value: string): boolean {
  return /^#[0-9a-fA-F]{6}$/.test(value);
}

function parse(hex: string): [number, number, number] {
  const h = isHex(hex) ? hex : "#808080";
  return [parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16)];
}

function toHex([r, g, b]: [number, number, number]): string {
  const part = (n: number) => Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, "0");
  return `#${part(r)}${part(g)}${part(b)}`;
}

/** `amount` of `b` mixed into `a` (0 gives a, 1 gives b). */
export function mix(a: string, b: string, amount: number): string {
  const x = parse(a);
  const y = parse(b);
  return toHex([x[0] + (y[0] - x[0]) * amount, x[1] + (y[1] - x[1]) * amount, x[2] + (y[2] - x[2]) * amount]);
}

function luminance(hex: string): number {
  const [r, g, b] = parse(hex).map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** How easily `a` reads on `b`: 1 is invisible, 21 is black on white. 4.5 is the usual bar for text. */
export function contrast(a: string, b: string): number {
  const la = luminance(a);
  const lb = luminance(b);
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

/** Black or white, whichever reads better on `color`. */
export function readableOn(color: string): string {
  return contrast("#0b1220", color) >= contrast("#ffffff", color) ? "#0b1220" : "#ffffff";
}

// ---- The palette ----------------------------------------------------------------------------

/** The CSS variables for a theme, worked out from its colours. */
export function themeVariables(theme: AppTheme): Record<string, string> {
  const { bg, panel, text, accent, go, danger } = theme.colors;
  const dark = theme.base === "dark";
  const ink = dark ? "#ffffff" : "#000000";
  return {
    "--bg-0": bg,
    "--bg-1": panel,
    "--bg-2": mix(panel, text, 0.05),
    "--bg-3": mix(panel, text, 0.1),
    "--bg-hover": `${ink}${dark ? "0d" : "0a"}`,
    "--bg-active": `${ink}${dark ? "14" : "14"}`,
    "--border": mix(panel, text, 0.12),
    "--border-strong": mix(panel, text, 0.24),
    "--text": text,
    "--text-muted": mix(text, bg, 0.22),
    "--text-faint": mix(text, bg, 0.42),
    "--accent": accent,
    "--accent-strong": mix(accent, dark ? "#000000" : "#000000", 0.2),
    "--accent-soft": `${accent}22`,
    "--on-accent": readableOn(accent),
    "--go": go,
    "--go-strong": mix(go, "#000000", 0.18),
    "--on-go": readableOn(go),
    "--success": go,
    "--danger": danger,
    "--danger-strong": mix(danger, "#000000", 0.2),
    "--danger-soft": `${danger}1f`,
    "--warning": dark ? "#f0b45a" : "#b77a14",
    "--warning-soft": dark ? "#f0b45a1f" : "#b77a141a",
  };
}

/** The terminal's colours: the base palette, with the theme's own colours on top. */
export function terminalThemeFor(theme: AppTheme): TerminalTheme {
  const base = terminalTheme(theme.base);
  if (theme.id === "dark" || theme.id === "light") return base;
  const { bg, text, accent, go, danger } = theme.colors;
  return {
    ...base,
    background: bg,
    foreground: text,
    cursor: accent,
    cursorAccent: bg,
    selectionBackground: `${accent}40`,
    red: danger,
    green: go,
    blue: accent,
    brightRed: mix(danger, "#ffffff", 0.2),
    brightGreen: mix(go, "#ffffff", 0.2),
    brightBlue: mix(accent, "#ffffff", 0.2),
    ...theme.terminal,
  };
}

/** Things worth telling the person about a theme: colours that would be hard to read. */
export function warnings(theme: AppTheme): string[] {
  const { bg, panel, text, accent, go, danger } = theme.colors;
  const out: string[] = [];
  if (contrast(text, bg) < 4.5) out.push("The text is hard to read on the background.");
  if (contrast(text, panel) < 4.5) out.push("The text is hard to read on the panels.");
  if (contrast(accent, bg) < 3) out.push("The accent is hard to see on the background.");
  if (contrast(go, bg) < 3) out.push("The Go colour is hard to see on the background.");
  if (contrast(danger, bg) < 3) out.push("The Danger colour is hard to see on the background.");
  return out;
}

// ---- Built-in themes -------------------------------------------------------------------------

export const BUILTIN_THEMES: AppTheme[] = [
  {
    id: "dark",
    name: "Dark",
    base: "dark",
    builtin: true,
    colors: { bg: "#14161b", panel: "#1a1d24", text: "#e4e7ee", accent: "#6aa2ff", go: "#3fb97a", danger: "#f2706b" },
  },
  {
    id: "light",
    name: "Light",
    base: "light",
    builtin: true,
    colors: { bg: "#ffffff", panel: "#f4f5f8", text: "#1d2230", accent: "#2f6fe4", go: "#1f9d63", danger: "#d33f3a" },
  },
  {
    id: "nord",
    name: "Nord",
    base: "dark",
    builtin: true,
    colors: { bg: "#2e3440", panel: "#353c4a", text: "#eceff4", accent: "#88c0d0", go: "#a3be8c", danger: "#bf616a" },
  },
  {
    id: "dracula",
    name: "Dracula",
    base: "dark",
    builtin: true,
    colors: { bg: "#282a36", panel: "#303342", text: "#f8f8f2", accent: "#bd93f9", go: "#50fa7b", danger: "#ff5555" },
  },
  {
    id: "solarized-dark",
    name: "Solarized Dark",
    base: "dark",
    builtin: true,
    colors: { bg: "#002b36", panel: "#07404d", text: "#a7b5b5", accent: "#268bd2", go: "#859900", danger: "#dc322f" },
  },
  {
    id: "gruvbox",
    name: "Gruvbox Dark",
    base: "dark",
    builtin: true,
    colors: { bg: "#282828", panel: "#32302f", text: "#ebdbb2", accent: "#83a598", go: "#b8bb26", danger: "#fb4934" },
  },
  {
    id: "high-contrast",
    name: "High contrast",
    base: "dark",
    builtin: true,
    colors: { bg: "#000000", panel: "#0f0f0f", text: "#ffffff", accent: "#5cc8ff", go: "#3dff8b", danger: "#ff6b6b" },
  },
  {
    id: "soft-light",
    name: "Soft light",
    base: "light",
    builtin: true,
    colors: { bg: "#fbf8f3", panel: "#f0ebe2", text: "#33322e", accent: "#3a6ea5", go: "#3f7f38", danger: "#b5453a" },
  },
];

/** Checks a theme read from storage, so a damaged or hand-edited one can't break the screen. */
export function cleanTheme(value: unknown): AppTheme | null {
  if (!value || typeof value !== "object") return null;
  const v = value as Partial<AppTheme>;
  if (typeof v.id !== "string" || !v.id || typeof v.name !== "string") return null;
  if (v.base !== "dark" && v.base !== "light") return null;
  const c = v.colors as Partial<ThemeColors> | undefined;
  if (!c) return null;
  const colors = {} as ThemeColors;
  for (const { key } of COLOR_LABELS) {
    const got = c[key];
    if (typeof got !== "string" || !isHex(got)) return null;
    colors[key] = got;
  }
  const terminal: Partial<TerminalTheme> = {};
  if (v.terminal && typeof v.terminal === "object") {
    for (const [k, got] of Object.entries(v.terminal)) {
      if (typeof got === "string" && isHex(got) && k in terminalTheme("dark")) {
        (terminal as Record<string, string>)[k] = got;
      }
    }
  }
  return { id: v.id, name: v.name.slice(0, 40), base: v.base, colors, terminal };
}
