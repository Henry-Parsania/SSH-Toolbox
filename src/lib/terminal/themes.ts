// Terminal colour themes (T7). The background matches the window, so the terminal blends in.

export type ThemeName = "dark" | "light";

export interface TerminalTheme {
  background: string;
  foreground: string;
  cursor: string;
  cursorAccent: string;
  selectionBackground: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
}

const THEMES: Record<ThemeName, TerminalTheme> = {
  dark: {
    background: "#14161b",
    foreground: "#e4e7ee",
    cursor: "#6aa2ff",
    cursorAccent: "#14161b",
    selectionBackground: "#6aa2ff40",
    black: "#2a2f3a",
    red: "#f2706b",
    green: "#7ed99e",
    yellow: "#f0c674",
    blue: "#6aa2ff",
    magenta: "#c79bff",
    cyan: "#63d6dc",
    white: "#cfd4df",
    brightBlack: "#5a6172",
    brightRed: "#ff8a85",
    brightGreen: "#9be8b5",
    brightYellow: "#f8d88e",
    brightBlue: "#8bb8ff",
    brightMagenta: "#dab5ff",
    brightCyan: "#85e6eb",
    brightWhite: "#ffffff",
  },
  light: {
    background: "#ffffff",
    foreground: "#1d2230",
    cursor: "#2f6fe4",
    cursorAccent: "#ffffff",
    selectionBackground: "#2f6fe430",
    black: "#3a4150",
    red: "#c93a35",
    green: "#1d8a55",
    yellow: "#9a6a00",
    blue: "#2158c4",
    magenta: "#8a3fc9",
    cyan: "#0e7f8a",
    white: "#8b92a4",
    brightBlack: "#5c6477",
    brightRed: "#e0514b",
    brightGreen: "#27a468",
    brightYellow: "#b77a14",
    brightBlue: "#3d78e8",
    brightMagenta: "#a35be0",
    brightCyan: "#1a98a5",
    brightWhite: "#1d2230",
  },
};

export function terminalTheme(name: ThemeName): TerminalTheme {
  return THEMES[name] ?? THEMES.dark;
}
