// A small line-by-line comparison, for showing what a file write would change.

export type DiffLine = { kind: "same" | "add" | "del"; text: string };

const MAX_CELLS = 4_000_000;

/** Compares two texts line by line. Very large inputs fall back to "everything replaced". */
export function diffLines(before: string, after: string): DiffLine[] {
  const a = before === "" ? [] : before.split("\n");
  const b = after === "" ? [] : after.split("\n");
  if (a.length * b.length > MAX_CELLS) {
    return [...a.map((text) => ({ kind: "del" as const, text })), ...b.map((text) => ({ kind: "add" as const, text }))];
  }
  // Longest common subsequence table, built from the end so the walk goes forwards.
  const width = b.length + 1;
  const table = new Uint32Array((a.length + 1) * width);
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      table[i * width + j] =
        a[i] === b[j] ? table[(i + 1) * width + j + 1] + 1 : Math.max(table[(i + 1) * width + j], table[i * width + j + 1]);
    }
  }
  const out: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      out.push({ kind: "same", text: a[i] });
      i++;
      j++;
    } else if (table[(i + 1) * width + j] >= table[i * width + j + 1]) {
      out.push({ kind: "del", text: a[i++] });
    } else {
      out.push({ kind: "add", text: b[j++] });
    }
  }
  while (i < a.length) out.push({ kind: "del", text: a[i++] });
  while (j < b.length) out.push({ kind: "add", text: b[j++] });
  return out;
}

/** The changed lines with a little unchanged text around them, as a unified-style listing. */
export function showChanges(before: string, after: string, context = 2): string {
  const lines = diffLines(before, after);
  const keep = new Array<boolean>(lines.length).fill(false);
  lines.forEach((line, i) => {
    if (line.kind === "same") return;
    for (let k = Math.max(0, i - context); k <= Math.min(lines.length - 1, i + context); k++) keep[k] = true;
  });
  const out: string[] = [];
  let skipped = false;
  lines.forEach((line, i) => {
    if (!keep[i]) {
      skipped = true;
      return;
    }
    if (skipped) out.push("  …");
    skipped = false;
    out.push(`${line.kind === "add" ? "+" : line.kind === "del" ? "-" : " "} ${line.text}`);
  });
  if (skipped) out.push("  …");
  return out.join("\n");
}
