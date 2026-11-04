import { describe, expect, it } from "vitest";
import { changedHunks, diffLines, hasChanges } from "./diff";

const show = (lines: ReturnType<typeof diffLines>) => lines.map((l) => (l.type === "same" ? " " : l.type === "add" ? "+" : "-") + l.text).join("|");

describe("diffLines", () => {
  it("finds added, removed and changed lines", () => {
    expect(show(diffLines("a\nb\nc", "a\nb\nc"))).toBe(" a| b| c");
    expect(show(diffLines("a\nb\nc", "a\nX\nc"))).toBe(" a|-b|+X| c");
    expect(show(diffLines("a\nc", "a\nb\nc"))).toBe(" a|+b| c");
    expect(show(diffLines("a\nb\nc", "a\nc"))).toBe(" a|-b| c");
    expect(show(diffLines("", "x"))).toBe("-|+x");
  });

  it("works on a big file with one change", () => {
    const base = Array.from({ length: 20000 }, (_, i) => `line ${i}`);
    const edited = [...base];
    edited[10000] = "changed";
    const lines = diffLines(base.join("\n"), edited.join("\n"));
    expect(lines.filter((l) => l.type !== "same")).toEqual([
      { type: "del", text: "line 10000" },
      { type: "add", text: "changed" },
    ]);
  });

  it("shows only the changes with context", () => {
    const before = Array.from({ length: 30 }, (_, i) => `l${i}`).join("\n");
    const after = before.replace("l15", "L15");
    const lines = diffLines(before, after);
    expect(hasChanges(lines)).toBe(true);
    expect(hasChanges(diffLines(before, before))).toBe(false);
    const hunks = changedHunks(lines, 2);
    expect(hunks.filter((h) => h.type === "gap")).toHaveLength(2);
    expect(hunks.filter((h) => h.type !== "gap")).toHaveLength(6); // 2 before, -, +, 2 after
  });
});
