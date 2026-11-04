import { describe, expect, it } from "vitest";
import { nextRetryDelay } from "./autoReconnect";

const base = { enabled: true, isSsh: true, wasConnected: true, lost: true, retries: 0 };

describe("nextRetryDelay", () => {
  it("backs off and gives up after five tries", () => {
    expect([0, 1, 2, 3, 4].map((retries) => nextRetryDelay({ ...base, retries }))).toEqual([2, 4, 8, 16, 30]);
    expect(nextRetryDelay({ ...base, retries: 5 })).toBeNull();
  });

  it("leaves everything else alone", () => {
    expect(nextRetryDelay({ ...base, enabled: false })).toBeNull();
    expect(nextRetryDelay({ ...base, isSsh: false })).toBeNull();
    expect(nextRetryDelay({ ...base, wasConnected: false })).toBeNull();
    expect(nextRetryDelay({ ...base, lost: false })).toBeNull();
  });
});
