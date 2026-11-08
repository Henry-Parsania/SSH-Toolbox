import { describe, expect, it } from "vitest";
import { loadVsCodeColors, normalizeColor, parseJsonc, resolveSibling, vscodeToTheme } from "./vscodeTheme";
import { cleanTheme } from "./themes";

describe("VS Code themes", () => {
  it("reads comments and trailing commas but keeps strings alone", () => {
    const v = parseJsonc('{ // note\n "a": "x // not a comment", /* c */ "b": [1, 2,], }') as { a: string; b: number[] };
    expect(v.a).toBe("x // not a comment");
    expect(v.b).toEqual([1, 2]);
  });
  it("normalises colours and drops alpha", () => {
    expect(normalizeColor("#FFF")).toBe("#ffffff");
    expect(normalizeColor("#11223344")).toBe("#112233");
    expect(normalizeColor("#abcd")).toBe("#aabbcc");
    expect(normalizeColor("red")).toBeNull();
    expect(normalizeColor("#12345")).toBeNull();
  });
  it("makes a valid app theme, with fallbacks for missing colours", () => {
    const t = vscodeToTheme("Sample", "vs-dark", { "editor.background": "#101820", "terminal.ansiRed": "#ff0000", "terminal.ansiBrightRed": "#ff8888" }, "custom-1");
    expect(t.colors.bg).toBe("#101820");
    expect(t.colors.danger).toBe("#ff0000");
    expect(t.terminal?.brightRed).toBe("#ff8888");
    expect(cleanTheme(t)).not.toBeNull();
    expect(vscodeToTheme("L", "vs", {}, "x").base).toBe("light");
  });
  it("follows includes", async () => {
    const files: Record<string, string> = {
      "/t/a.json": '{"include":"./base.json","colors":{"editor.background":"#000001"}}',
      "/t/base.json": '{"colors":{"editor.background":"#000002","editor.foreground":"#ffffff"}}',
    };
    const colors = await loadVsCodeColors("/t/a.json", async (p) => files[p], resolveSibling);
    expect(colors["editor.background"]).toBe("#000001");
    expect(colors["editor.foreground"]).toBe("#ffffff");
    expect(resolveSibling(["C:", "x", "a.json"].join(String.fromCharCode(92)), "../b/c.json")).toBe(["C:", "b", "c.json"].join(String.fromCharCode(92)));
  });
});
