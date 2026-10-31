import { describe, expect, it, vi } from "vitest";
import { broadcastInput, registerReceiver } from "./broadcast";

describe("broadcastInput", () => {
  it("reaches the other panes of the same tab, not the sender or other tabs", () => {
    const a = vi.fn(), b = vi.fn(), c = vi.fn(), other = vi.fn();
    registerReceiver("t1", "a", a);
    registerReceiver("t1", "b", b);
    registerReceiver("t1", "c", c);
    registerReceiver("t2", "x", other);
    const data = new Uint8Array([104, 105]);
    broadcastInput("t1", "a", data);
    expect(a).not.toHaveBeenCalled();
    expect(b).toHaveBeenCalledWith(data);
    expect(c).toHaveBeenCalledWith(data);
    expect(other).not.toHaveBeenCalled();
  });

  it("a pane that was removed no longer receives anything", () => {
    const a = vi.fn(), b = vi.fn();
    registerReceiver("t3", "a", a);
    const remove = registerReceiver("t3", "b", b);
    remove();
    broadcastInput("t3", "a", new Uint8Array([1]));
    expect(b).not.toHaveBeenCalled();
  });

  it("does nothing for a tab nobody registered", () => {
    expect(() => broadcastInput("none", "a", new Uint8Array([1]))).not.toThrow();
  });
});
