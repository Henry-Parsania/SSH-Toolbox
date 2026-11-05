// Which well-known terminal fonts are installed. A web page can't list a computer's fonts
// directly, so each candidate is drawn and measured: an installed font gives a different width
// from the fallback it would otherwise be drawn in.

const CANDIDATES = [
  "Cascadia Mono",
  "Cascadia Code",
  "Consolas",
  "Courier New",
  "Lucida Console",
  "JetBrains Mono",
  "Fira Code",
  "Fira Mono",
  "Source Code Pro",
  "Hack",
  "Inconsolata",
  "IBM Plex Mono",
  "Roboto Mono",
  "Ubuntu Mono",
  "Ubuntu Sans Mono",
  "DejaVu Sans Mono",
  "Liberation Mono",
  "Noto Sans Mono",
  "Cousine",
  "Menlo",
  "Monaco",
  "SF Mono",
  "Droid Sans Mono",
  "Anonymous Pro",
  "Victor Mono",
  "Iosevka",
  "Space Mono",
  "Red Hat Mono",
  "Geist Mono",
  "Commit Mono",
  "Maple Mono",
  "Monaspace Neon",
  "Terminus",
  "Courier Prime",
  "Share Tech Mono",
  "Overpass Mono",
  "Oxygen Mono",
  "MesloLGS NF",
  "Meslo LG M",
  "CaskaydiaCove Nerd Font Mono",
  "FiraCode Nerd Font Mono",
  "JetBrainsMono Nerd Font Mono",
  "Hack Nerd Font Mono",
];

const SAMPLE = "mmmmmmmmmmlli0Oo1WwQq@#";

/** The candidates found on this computer, in the list's order. */
export function installedFonts(): string[] {
  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d");
  if (!ctx) return [];
  const width = (font: string) => {
    ctx.font = `64px ${font}`;
    return ctx.measureText(SAMPLE).width;
  };
  const baseMono = width("monospace");
  const baseSerif = width("serif");
  return CANDIDATES.filter((name) => {
    const quoted = `'${name}'`;
    return width(`${quoted}, monospace`) !== baseMono || width(`${quoted}, serif`) !== baseSerif;
  });
}

/** A font name as it goes into a font list: quoted when it is a single name. */
export function quoteFont(family: string): string {
  const f = family.trim();
  return /[,'"]/.test(f) ? f : `'${f}'`;
}
