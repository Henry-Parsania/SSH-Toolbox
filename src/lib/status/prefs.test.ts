import { describe, expect, it } from "vitest";
import { METRICS, defaultPrefs, parsePrefs } from "./prefs";

describe("footer preferences", () => {
  it("starts with everything shown and every graph off", () => {
    const p = defaultPrefs();
    for (const m of METRICS) {
      expect(p[m.id].show).toBe(true);
      expect(p[m.id].graph).toBe(false);
    }
  });

  it("reads saved choices and fills in anything missing", () => {
    const p = parsePrefs(JSON.stringify({ cpu: { graph: true, show: false }, gpu: { numbers: false }, unknown: { show: false } }));
    expect(p.cpu).toEqual({ show: false, bar: true, numbers: true, graph: true });
    expect(p.gpu.numbers).toBe(false);
    expect(p.ram).toEqual(defaultPrefs().ram);
  });

  it("falls back to the defaults for damaged values", () => {
    expect(parsePrefs(null)).toEqual(defaultPrefs());
    expect(parsePrefs("not json")).toEqual(defaultPrefs());
    expect(parsePrefs("42")).toEqual(defaultPrefs());
    expect(parsePrefs(JSON.stringify({ cpu: { graph: "yes", show: 1 } }))).toEqual(defaultPrefs());
  });
});
