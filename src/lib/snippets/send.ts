import type { Snippet } from "../backend";

const encoder = new TextEncoder();

/**
 * What to type for a snippet. Lines are separated with Enter, as if typed, and a trailing Enter
 * is added when the snippet is set to run straight away. Line endings are normalised first.
 */
export function snippetBytes(snippet: Pick<Snippet, "text" | "run">): Uint8Array {
  const lines = snippet.text.replace(/\r\n?/g, "\n").replace(/\n+$/, "").split("\n");
  return encoder.encode(lines.join("\r") + (snippet.run ? "\r" : ""));
}
