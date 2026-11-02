import { describe, expect, it } from "vitest";
import { diffLines, showChanges } from "./diff";

describe("diffLines", () => {
  it("finds added, removed and unchanged lines", () => {
    const out = diffLines("a\nb\nc", "a\nx\nc\nd");
    expect(out.map((l) => `${l.kind}:${l.text}`)).toEqual(["same:a", "del:b", "add:x", "same:c", "add:d"]);
  });

  it("treats a new file as all additions and an emptied file as all deletions", () => {
    expect(diffLines("", "a\nb").every((l) => l.kind === "add")).toBe(true);
    expect(diffLines("a\nb", "").every((l) => l.kind === "del")).toBe(true);
    expect(diffLines("same", "same")).toEqual([{ kind: "same", text: "same" }]);
  });
});

describe("showChanges", () => {
  it("shows the changes with a little context and marks what was left out", () => {
    const before = Array.from({ length: 20 }, (_, i) => `line ${i}`).join("\n");
    const after = before.replace("line 10", "LINE TEN");
    const text = showChanges(before, after, 1);
    expect(text).toContain("- line 10");
    expect(text).toContain("+ LINE TEN");
    expect(text).toContain("  line 9");
    expect(text).not.toContain("line 3");
    expect(text).toContain("…");
  });
});
