// Colours chosen for saved SSH sessions, by session ID. App keeps this in step with the library,
// so tabs and terminals can look a colour up without being handed the whole list.
export const sessionColors = $state<Record<string, string>>({});

export function setSessionColors(sessions: { id: string; type: string; color?: string }[]) {
  for (const id of Object.keys(sessionColors)) delete sessionColors[id];
  for (const s of sessions) if (s.type === "ssh" && s.color) sessionColors[s.id] = s.color;
}

/** Light text on dark colours and dark text on light ones. */
export function readableOn(color: string): string {
  const n = parseInt(color.slice(1), 16);
  const lum = (0.299 * (n >> 16) + 0.587 * ((n >> 8) & 255) + 0.114 * (n & 255)) / 255;
  return lum > 0.6 ? "#14161b" : "#e8eaf0";
}

/** Ready-made choices for the session form. */
export const COLOR_CHOICES = ["#7a1f2b", "#8a4b12", "#6b5b0a", "#1f5a2e", "#0f5a5a", "#1f3f7a", "#4a2a7a", "#7a2a5a"];
