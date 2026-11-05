import { describe, expect, it } from "vitest";
import { BUILTIN_THEMES, cleanTheme, contrast, mix, readableOn, terminalThemeFor, themeVariables, warnings } from "./themes";

describe("colour maths", () => {
  it("mixes two colours", () => {
    expect(mix("#000000", "#ffffff", 0.5)).toBe("#808080");
    expect(mix("#102030", "#102030", 0.7)).toBe("#102030");
  });
  it("measures contrast", () => {
    expect(contrast("#000000", "#ffffff")).toBeCloseTo(21, 0);
    expect(contrast("#777777", "#777777")).toBeCloseTo(1, 5);
  });
  it("picks readable text", () => {
    expect(readableOn("#ffffff")).toBe("#0b1220");
    expect(readableOn("#101010")).toBe("#ffffff");
  });
});

describe("themes", () => {
  it("every built-in theme is readable and complete", () => {
    for (const theme of BUILTIN_THEMES) {
      expect(warnings(theme), theme.name).toEqual([]);
      const vars = themeVariables(theme);
      expect(Object.values(vars).every((v) => /^#[0-9a-f]{6,8}$/i.test(v)), theme.name).toBe(true);
    }
  });
  it("warns about unreadable colours", () => {
    const bad = { ...BUILTIN_THEMES[2], colors: { ...BUILTIN_THEMES[2].colors, text: "#303744" } };
    expect(warnings(bad).length).toBeGreaterThan(0);
  });
  it("the terminal follows the theme", () => {
    const nord = BUILTIN_THEMES.find((t) => t.id === "nord")!;
    expect(terminalThemeFor(nord).background).toBe(nord.colors.bg);
    expect(terminalThemeFor(nord).red).toBe(nord.colors.danger);
    expect(terminalThemeFor({ ...nord, terminal: { red: "#ff0000" } }).red).toBe("#ff0000");
  });
  it("rejects a damaged saved theme", () => {
    expect(cleanTheme(null)).toBeNull();
    expect(cleanTheme({ id: "x", name: "x", base: "dark", colors: { bg: "red" } })).toBeNull();
    const good = cleanTheme({ ...BUILTIN_THEMES[2], id: "mine", terminal: { red: "#ff0000", bogus: "#00ff00", green: "nope" } });
    expect(good?.terminal).toEqual({ red: "#ff0000" });
  });
});
