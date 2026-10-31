import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke, channels } = vi.hoisted(() => ({
  invoke: vi.fn(),
  channels: [] as { onmessage: (m: unknown) => void }[],
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke,
  Channel: class {
    onmessage: (m: unknown) => void = () => {};
    constructor() {
      channels.push(this);
    }
  },
}));

import {
  listShells,
  platformInfo,
  terminalAck,
  terminalClose,
  terminalOpen,
  terminalResize,
  terminalWrite,
  type TerminalStatus,
} from "./index";

beforeEach(() => {
  invoke.mockReset();
  channels.length = 0;
});

describe("platformInfo", () => {
  it("calls the platform_info command and returns its result", async () => {
    const info = { os: "windows", wsl_available: true, windows_build: 26200 };
    invoke.mockResolvedValueOnce(info);
    await expect(platformInfo()).resolves.toEqual(info);
    expect(invoke).toHaveBeenCalledWith("platform_info");
  });
});

describe("terminal commands", () => {
  it("lists shells", async () => {
    invoke.mockResolvedValueOnce([{ id: "cmd", name: "Command Prompt" }]);
    await expect(listShells()).resolves.toEqual([{ id: "cmd", name: "Command Prompt" }]);
    expect(invoke).toHaveBeenCalledWith("list_shells");
  });

  it("opens a terminal with two channels and returns the session id", async () => {
    invoke.mockResolvedValueOnce("session-1");
    const id = await terminalOpen({ type: "local", shell_id: "cmd" }, 80, 24, () => {}, () => {});
    expect(id).toBe("session-1");
    const [name, args] = invoke.mock.calls[0];
    expect(name).toBe("terminal_open");
    expect(args.spec).toEqual({ type: "local", shell_id: "cmd" });
    expect([args.cols, args.rows]).toEqual([80, 24]);
    expect(args.onOutput).toBe(channels[0]);
    expect(args.onStatus).toBe(channels[1]);
  });

  it("passes output through as bytes, unchanged", async () => {
    invoke.mockResolvedValueOnce("s");
    const chunks: Uint8Array[] = [];
    await terminalOpen({ type: "local", shell_id: "x" }, 80, 24, (c) => chunks.push(c), () => {});
    const sent = new Uint8Array([0, 1, 2, 255, 128]);
    channels[0].onmessage(sent.buffer);
    expect(chunks).toHaveLength(1);
    expect(chunks[0]).toBeInstanceOf(Uint8Array);
    expect(Array.from(chunks[0])).toEqual([0, 1, 2, 255, 128]);
  });

  it("passes statuses through", async () => {
    invoke.mockResolvedValueOnce("s");
    const statuses: TerminalStatus[] = [];
    await terminalOpen({ type: "local", shell_id: "x" }, 80, 24, () => {}, (s) => statuses.push(s));
    channels[1].onmessage({ type: "exited", code: 0 });
    expect(statuses).toEqual([{ type: "exited", code: 0 }]);
  });

  it("sends keystrokes as a plain number array", async () => {
    invoke.mockResolvedValueOnce(undefined);
    await terminalWrite("s", new Uint8Array([104, 105, 255]));
    expect(invoke).toHaveBeenCalledWith("terminal_write", { sessionId: "s", data: [104, 105, 255] });
  });

  it("resizes, acknowledges and closes by session id", async () => {
    invoke.mockResolvedValue(undefined);
    await terminalResize("s", 100, 30);
    await terminalAck("s", 32768);
    await terminalClose("s");
    expect(invoke.mock.calls).toEqual([
      ["terminal_resize", { sessionId: "s", cols: 100, rows: 30 }],
      ["terminal_ack", { sessionId: "s", bytes: 32768 }],
      ["terminal_close", { sessionId: "s" }],
    ]);
  });
});
