import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ACK_DELAY_MS, ACK_THRESHOLD, AckBatcher, InputSender } from "./flow";
import { chooseRenderer, webglFailureNotice } from "./renderer";

describe("AckBatcher", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("sends at once when the threshold is reached", () => {
    const send = vi.fn();
    const batcher = new AckBatcher(send);
    batcher.processed(ACK_THRESHOLD - 1);
    expect(send).not.toHaveBeenCalled();
    batcher.processed(1);
    expect(send).toHaveBeenCalledExactlyOnceWith(ACK_THRESHOLD);
  });

  it("joins small reports and sends them after the delay", () => {
    const send = vi.fn();
    const batcher = new AckBatcher(send);
    batcher.processed(100);
    batcher.processed(200);
    vi.advanceTimersByTime(ACK_DELAY_MS - 1);
    expect(send).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(send).toHaveBeenCalledExactlyOnceWith(300);
  });

  it("never holds a report longer than the delay, however many arrive", () => {
    const send = vi.fn();
    const batcher = new AckBatcher(send);
    for (let i = 0; i < 10; i++) {
      batcher.processed(10);
      vi.advanceTimersByTime(ACK_DELAY_MS / 10);
    }
    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledWith(100);
  });

  it("starts a fresh batch after sending", () => {
    const send = vi.fn();
    const batcher = new AckBatcher(send);
    batcher.processed(ACK_THRESHOLD);
    batcher.processed(5);
    vi.advanceTimersByTime(ACK_DELAY_MS);
    expect(send.mock.calls).toEqual([[ACK_THRESHOLD], [5]]);
  });

  it("flush sends what is waiting, and nothing when empty", () => {
    const send = vi.fn();
    const batcher = new AckBatcher(send);
    batcher.flush();
    expect(send).not.toHaveBeenCalled();
    batcher.processed(7);
    batcher.flush();
    expect(send).toHaveBeenCalledExactlyOnceWith(7);
    vi.advanceTimersByTime(ACK_DELAY_MS * 2);
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("ignores empty reports and sends nothing after dispose", () => {
    const send = vi.fn();
    const batcher = new AckBatcher(send);
    batcher.processed(0);
    batcher.processed(10);
    batcher.dispose();
    vi.advanceTimersByTime(ACK_DELAY_MS * 2);
    expect(send).not.toHaveBeenCalled();
  });

  it("keeps the threshold above what the backend needs to resume", () => {
    // The backend resumes at 100,000 unacknowledged bytes (LOW). The interface can hold back
    // less than that, or the backend could wait for an acknowledgement that never comes.
    expect(ACK_THRESHOLD).toBeLessThan(100_000);
  });
});

describe("InputSender", () => {
  const bytes = (...n: number[]) => new Uint8Array(n);

  /** A write that finishes only when the test says so. */
  function controlledWrite() {
    const calls: { data: number[]; done: () => void; fail: () => void }[] = [];
    const write = vi.fn(
      (data: Uint8Array) =>
        new Promise<void>((resolve, reject) => {
          calls.push({ data: Array.from(data), done: resolve, fail: () => reject(new Error("gone")) });
        }),
    );
    return { write, calls };
  }

  it("sends nothing until enabled, then everything in order", async () => {
    const { write, calls } = controlledWrite();
    const sender = new InputSender(write);
    sender.push(bytes(1));
    sender.push(bytes(2, 3));
    expect(write).not.toHaveBeenCalled();
    sender.enable();
    expect(calls.map((c) => c.data)).toEqual([[1, 2, 3]]);
  });

  it("never has two requests in flight, and joins what arrives meanwhile", async () => {
    const { write, calls } = controlledWrite();
    const sender = new InputSender(write);
    sender.enable();
    sender.push(bytes(1));
    sender.push(bytes(2));
    sender.push(bytes(3));
    expect(write).toHaveBeenCalledTimes(1);
    calls[0].done();
    await vi.waitFor(() => expect(write).toHaveBeenCalledTimes(2));
    expect(calls[1].data).toEqual([2, 3]);
    calls[1].done();
  });

  it("ignores empty input", () => {
    const { write } = controlledWrite();
    const sender = new InputSender(write);
    sender.enable();
    sender.push(bytes());
    expect(write).not.toHaveBeenCalled();
  });

  it("drops what is waiting if a write fails, and carries on with new input", async () => {
    const { write, calls } = controlledWrite();
    const sender = new InputSender(write);
    sender.enable();
    sender.push(bytes(1));
    sender.push(bytes(2));
    calls[0].fail();
    await vi.waitFor(() => expect(write).toHaveBeenCalledTimes(1));
    await Promise.resolve();
    sender.push(bytes(9));
    await vi.waitFor(() => expect(write).toHaveBeenCalledTimes(2));
    expect(calls[1].data).toEqual([9]);
  });

  it("stop discards waiting input and sends nothing more", () => {
    const { write } = controlledWrite();
    const sender = new InputSender(write);
    sender.push(bytes(1));
    sender.stop();
    sender.enable();
    sender.push(bytes(2));
    expect(write).toHaveBeenCalledTimes(1);
  });
});

describe("renderer choice", () => {
  it("auto is WebGL on Windows and DOM on Linux and macOS", () => {
    expect(chooseRenderer("auto", "windows")).toBe("webgl");
    expect(chooseRenderer("auto", "linux")).toBe("dom");
    expect(chooseRenderer("auto", "macos")).toBe("dom");
  });

  it("an explicit choice wins on every system", () => {
    for (const os of ["windows", "linux", "macos"] as const) {
      expect(chooseRenderer("dom", os)).toBe("dom");
      expect(chooseRenderer("webgl", os)).toBe("webgl");
    }
  });

  it("only an explicit WebGL choice gets a notice when WebGL fails", () => {
    expect(webglFailureNotice("webgl")).toMatch(/WebGL/);
    expect(webglFailureNotice("auto")).toBeNull();
    expect(webglFailureNotice("dom")).toBeNull();
  });
});
