import { describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { platformInfo } from "./index";

describe("platformInfo", () => {
  it("calls the platform_info command and returns its result", async () => {
    const info = { os: "windows", wsl_available: true, windows_build: 26200 };
    invoke.mockResolvedValueOnce(info);
    await expect(platformInfo()).resolves.toEqual(info);
    expect(invoke).toHaveBeenCalledWith("platform_info");
  });
});
