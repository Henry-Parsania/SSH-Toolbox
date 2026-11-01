import { describe, expect, it } from "vitest";
import { bytes, percent, uptime } from "./format";

describe("footer formatting", () => {
  it("shows sizes in the largest sensible unit", () => {
    expect(bytes(0)).toBe("0 B");
    expect(bytes(1536)).toBe("1.5 KB");
    expect(bytes(16 * 1024 ** 3)).toBe("16.0 GB");
    expect(bytes(250 * 1024 ** 3)).toBe("250 GB");
  });

  it("keeps a percentage inside 0 to 100, and copes with an empty total", () => {
    expect(percent(50, 200)).toBe(25);
    expect(percent(300, 200)).toBe(100);
    expect(percent(5, 0)).toBe(0);
  });

  it("shows how long a computer has been on", () => {
    expect(uptime(90)).toBe("1m");
    expect(uptime(3 * 3600 + 120)).toBe("3h 2m");
    expect(uptime(2 * 86400 + 5 * 3600)).toBe("2d 5h");
  });
});
