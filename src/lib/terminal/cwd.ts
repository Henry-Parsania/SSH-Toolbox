// The folder a shell says it is in (OSC 7, "file://host/path"). Whatever the program in the
// terminal prints is untrusted, so this only ever produces a plain path to look at: it is never
// run, never opened as a file, and never decides what is sent anywhere (F7).

const MAX_LENGTH = 4096;

/**
 * The folder named by an OSC 7 value, or null if it isn't a clean one. The path uses "/" and
 * starts with "/", as for a Unix shell. A Windows drive path keeps its drive ("C:/Users/me").
 */
export function parseCwd(data: string): string | null {
  if (data.length === 0 || data.length > MAX_LENGTH) return null;
  const match = /^file:\/\/([^/]*)(\/.*)$/i.exec(data);
  if (!match) return null;
  let path: string;
  try {
    path = decodeURIComponent(match[2]);
  } catch {
    return null; // A broken % escape.
  }
  // Control characters (including NUL, newlines and Escape) have no place in a folder name
  // that will be shown and looked up.
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f]/.test(path)) return null;
  // "/C:/Users/me" is how a Windows shell writes a drive path.
  if (/^\/[A-Za-z]:(\/|$)/.test(path)) path = path.slice(1);
  return path;
}

/** Where the folder is in the WSL file view of a distribution: \\wsl.localhost\Ubuntu\home\me. */
export function wslPath(distro: string, cwd: string): string {
  return `\\\\wsl.localhost\\${distro}${cwd.replaceAll("/", "\\")}`;
}
