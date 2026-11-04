// A line-by-line comparison of two texts, for the editor's "Show changes" (task 7.10).

export type DiffLine = { type: "same" | "add" | "del"; text: string };

/** Above this many cells the exact comparison is skipped and the middle shown as replaced. */
const MAX_CELLS = 4_000_000;

/**
 * The lines of `before` turned into the lines of `after`. Common lines at the start and end are
 * set aside first, so a small edit in a big file is quick. If what is left in the middle is
 * still very big, it is shown as all removed and all added rather than compared exactly.
 */
export function diffLines(before: string, after: string): DiffLine[] {
  const a = before.split("\n");
  const b = after.split("\n");
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA--;
    endB--;
  }
  const out: DiffLine[] = a.slice(0, start).map((text) => ({ type: "same", text }));
  const midA = a.slice(start, endA);
  const midB = b.slice(start, endB);

  if (midA.length * midB.length > MAX_CELLS) {
    out.push(...midA.map((text): DiffLine => ({ type: "del", text })));
    out.push(...midB.map((text): DiffLine => ({ type: "add", text })));
  } else {
    // Longest common subsequence table, filled from the end.
    const w = midB.length + 1;
    const table = new Uint32Array((midA.length + 1) * w);
    for (let i = midA.length - 1; i >= 0; i--) {
      for (let j = midB.length - 1; j >= 0; j--) {
        table[i * w + j] =
          midA[i] === midB[j] ? table[(i + 1) * w + j + 1] + 1 : Math.max(table[(i + 1) * w + j], table[i * w + j + 1]);
      }
    }
    let i = 0;
    let j = 0;
    while (i < midA.length && j < midB.length) {
      if (midA[i] === midB[j]) {
        out.push({ type: "same", text: midA[i] });
        i++;
        j++;
      } else if (table[(i + 1) * w + j] >= table[i * w + j + 1]) {
        out.push({ type: "del", text: midA[i++] });
      } else {
        out.push({ type: "add", text: midB[j++] });
      }
    }
    while (i < midA.length) out.push({ type: "del", text: midA[i++] });
    while (j < midB.length) out.push({ type: "add", text: midB[j++] });
  }
  out.push(...a.slice(endA).map((text): DiffLine => ({ type: "same", text })));
  return out;
}

/** Only the changed lines with `context` unchanged lines around them; gaps become "..." markers. */
export function changedHunks(lines: DiffLine[], context = 3): (DiffLine | { type: "gap"; hidden: number })[] {
  const keep = new Array<boolean>(lines.length).fill(false);
  lines.forEach((l, i) => {
    if (l.type === "same") return;
    for (let k = Math.max(0, i - context); k <= Math.min(lines.length - 1, i + context); k++) keep[k] = true;
  });
  const out: (DiffLine | { type: "gap"; hidden: number })[] = [];
  let hidden = 0;
  lines.forEach((l, i) => {
    if (keep[i]) {
      if (hidden) out.push({ type: "gap", hidden });
      hidden = 0;
      out.push(l);
    } else {
      hidden++;
    }
  });
  if (hidden && out.length) out.push({ type: "gap", hidden });
  return out;
}

export function hasChanges(lines: DiffLine[]): boolean {
  return lines.some((l) => l.type !== "same");
}
