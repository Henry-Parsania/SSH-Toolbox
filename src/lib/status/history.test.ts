import { describe, expect, it } from "vitest";
import { netSpeed, pushReading } from "./history";

describe("pushReading", () => {
  it("keeps only the newest readings", () => {
    expect(pushReading([1, 2, 3], 4, 3)).toEqual([2, 3, 4]);
    expect(pushReading([], 1)).toEqual([1]);
  });
});

describe("netSpeed", () => {
  it("works out bytes per second between two readings", () => {
    expect(netSpeed({ at: 0, rx: 1000, tx: 0 }, { at: 5000, rx: 6000, tx: 2500 })).toEqual({ rx: 1000, tx: 500 });
  });

  it("says nothing without a reading before, or when the counters restart", () => {
    expect(netSpeed(null, { at: 0, rx: 1, tx: 1 })).toBeNull();
    expect(netSpeed({ at: 0, rx: 9000, tx: 0 }, { at: 5000, rx: 100, tx: 0 })).toBeNull();
    expect(netSpeed({ at: 0, rx: 0, tx: 0 }, { at: 100, rx: 1, tx: 1 })).toBeNull();
  });
});
