import { describe, expect, it } from "vitest";
import { FinishDetector } from "./finish";

describe("FinishDetector", () => {
  it("fires once after a long burst of output goes quiet", () => {
    const d = new FinishDetector();
    for (let t = 1000; t <= 12000; t += 500) d.chunk(t);
    expect(d.check(13000)).toBe(false); // only 1 s of quiet
    expect(d.check(15500)).toBe(true);
    expect(d.check(20000)).toBe(false); // already reported
  });

  it("ignores short bursts such as a prompt or a quick command", () => {
    const d = new FinishDetector();
    d.chunk(1000);
    d.chunk(1500);
    expect(d.check(10000)).toBe(false);
  });

  it("starts a new burst after a long pause", () => {
    const d = new FinishDetector();
    for (let t = 1000; t <= 12000; t += 500) d.chunk(t);
    expect(d.check(16000)).toBe(true);
    d.chunk(30000);
    d.chunk(30500);
    expect(d.check(40000)).toBe(false);
  });
});
