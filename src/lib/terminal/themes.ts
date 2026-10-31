// Terminal colour themes (T7).

export type ThemeName = "dark" | "light";

export interface TerminalTheme {
  background: string;
  foreground: string;
  cursor: string;
  selectionBackground: string;
}

const THEMES: Record<ThemeName, TerminalTheme> = {
  dark: { background: "#1e1e1e", foreground: "#d4d4d4", cursor: "#d4d4d4", selectionBackground: "#264f78" },
  light: { background: "#ffffff", foreground: "#1e1e1e", cursor: "#1e1e1e", selectionBackground: "#add6ff" },
};

export function terminalTheme(name: ThemeName): TerminalTheme {
  return THEMES[name] ?? THEMES.dark;
}
