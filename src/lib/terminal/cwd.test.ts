import { describe, expect, it } from "vitest";
import { parseCwd, wslPath } from "./cwd";

describe("parseCwd", () => {
  it("reads a folder from a file address", () => {
    expect(parseCwd("file://host/home/me")).toBe("/home/me");
    expect(parseCwd("file:///var/log")).toBe("/var/log");
    expect(parseCwd("file://host/")).toBe("/");
  });

  it("decodes percent escapes", () => {
    expect(parseCwd("file://h/home/my%20docs/caf%C3%A9")).toBe("/home/my docs/café");
  });

  it("keeps the drive of a Windows path", () => {
    expect(parseCwd("file://PC/C:/Users/me")).toBe("C:/Users/me");
  });

  it("refuses anything that isn't a plain folder address", () => {
    expect(parseCwd("")).toBeNull();
    expect(parseCwd("/home/me")).toBeNull();
    expect(parseCwd("http://host/home")).toBeNull();
    expect(parseCwd("file://host")).toBeNull();
    expect(parseCwd("file://host/bad%zz")).toBeNull();
    expect(parseCwd("file://host/a%00b")).toBeNull();
    expect(parseCwd("file://host/a%0Ab")).toBeNull();
    expect(parseCwd("file://host/a%1B[31mb")).toBeNull();
    expect(parseCwd("file://host/" + "a".repeat(5000))).toBeNull();
  });
});

describe("wslPath", () => {
  it("points into the distribution's file view", () => {
    expect(wslPath("Ubuntu", "/home/me")).toBe("\\\\wsl.localhost\\Ubuntu\\home\\me");
    expect(wslPath("Ubuntu", "/")).toBe("\\\\wsl.localhost\\Ubuntu\\");
  });
});
