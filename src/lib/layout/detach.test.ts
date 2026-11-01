import { describe, expect, it } from "vitest";
import { decodePayload, encodePayload, type WindowPayload } from "./detach";

const payload: WindowPayload = {
  title: "server · top é",
  root: {
    kind: "split",
    direction: "row",
    sizes: [0.5, 0.5],
    children: [
      { kind: "leaf", pane_id: "a", spec: { type: "ssh", saved_session_id: "s1", monitor: true }, title: "one" },
      { kind: "leaf", pane_id: "b", spec: null, title: "two" },
    ],
  },
  attach: { a: "session-1" },
  active: "a",
};

describe("window payload", () => {
  it("survives a round trip, including non-ASCII text", () => {
    expect(decodePayload(encodePayload(payload))).toEqual(payload);
  });

  it("only uses characters that are safe in an address", () => {
    expect(encodePayload(payload)).toMatch(/^[A-Za-z0-9_-]+$/);
  });

  it("refuses anything that isn't a description made by the app", () => {
    expect(decodePayload("")).toBeNull();
    expect(decodePayload("not base64 !!")).toBeNull();
    expect(decodePayload(encodePayload({ ...payload, root: { kind: "nope" } as never }))).toBeNull();
    const broken = { ...payload, root: { kind: "split", direction: "row", sizes: [1], children: [] } } as never;
    expect(decodePayload(encodePayload(broken))).toBeNull();
  });
});
