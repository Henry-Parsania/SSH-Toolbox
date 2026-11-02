import { describe, expect, it } from "vitest";
import { ipKey } from "./job.svelte";

describe("ipKey", () => {
  it("orders addresses numerically, not as text", () => {
    const sorted = ["10.0.0.100", "10.0.0.9", "9.255.255.255", "10.0.1.0"].sort((a, b) => ipKey(a) - ipKey(b));
    expect(sorted).toEqual(["9.255.255.255", "10.0.0.9", "10.0.0.100", "10.0.1.0"]);
  });

  it("puts anything else last", () => {
    expect(ipKey("::1")).toBeGreaterThan(ipKey("255.255.255.255"));
  });
});
