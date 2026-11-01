import { describe, expect, it } from "vitest";
import type { FileEntry } from "../backend/files";
import {
  baseName,
  shellQuote,
  crumbs,
  formatSize,
  joinPath,
  modeToText,
  parentPath,
  parseMode,
  percent,
  sortEntries,
} from "./paths";

const entry = (name: string, kind: FileEntry["kind"] = "file"): FileEntry => ({
  name,
  path: "/" + name,
  kind,
  size: null,
  modified: null,
  permissions: null,
});

describe("paths", () => {
  it("joins with the right separator and does not double it", () => {
    expect(joinPath("/home/a", "f", "/")).toBe("/home/a/f");
    expect(joinPath("/", "f", "/")).toBe("/f");
    expect(joinPath("C:\\Users\\a", "f", "\\")).toBe("C:\\Users\\a\\f");
    expect(joinPath("C:\\", "f", "\\")).toBe("C:\\f");
  });

  it("finds the parent, and a root stays a root", () => {
    expect(parentPath("/home/a/docs", "/")).toBe("/home/a");
    expect(parentPath("/home", "/")).toBe("/");
    expect(parentPath("/", "/")).toBe("/");
    expect(parentPath("/home/a/", "/")).toBe("/home");
    expect(parentPath("C:\\Users\\a", "\\")).toBe("C:\\Users");
    expect(parentPath("C:\\Users", "\\")).toBe("C:\\");
    expect(parentPath("C:\\", "\\")).toBe("C:\\");
  });

  it("takes the last part", () => {
    expect(baseName("/a/b/c.txt")).toBe("c.txt");
    expect(baseName("C:\\a\\b")).toBe("b");
    expect(baseName("/a/b/")).toBe("b");
  });

  it("breaks a path into clickable parts", () => {
    expect(crumbs("/home/a", "/")).toEqual([
      { label: "/", path: "/" },
      { label: "home", path: "/home" },
      { label: "a", path: "/home/a" },
    ]);
    expect(crumbs("C:\\Users\\a", "\\")).toEqual([
      { label: "C:", path: "C:\\" },
      { label: "Users", path: "C:\\Users" },
      { label: "a", path: "C:\\Users\\a" },
    ]);
  });
});

describe("sorting and showing", () => {
  it("puts folders first and sorts names without regard to case, numbers in order", () => {
    const sorted = sortEntries([
      entry("b.txt"),
      entry("Zed", "dir"),
      entry("a.txt"),
      entry("file10"),
      entry("file2"),
      entry("alpha", "dir"),
    ]);
    expect(sorted.map((e) => e.name)).toEqual(["alpha", "Zed", "a.txt", "b.txt", "file2", "file10"]);
  });

  it("formats sizes", () => {
    expect(formatSize(null)).toBe("");
    expect(formatSize(0)).toBe("0 B");
    expect(formatSize(1023)).toBe("1023 B");
    expect(formatSize(1536)).toBe("1.5 KB");
    expect(formatSize(5 * 1024 * 1024)).toBe("5.0 MB");
    expect(formatSize(200 * 1024 * 1024 * 1024)).toBe("200 GB");
  });

  it("turns permission bits into text and back", () => {
    expect(modeToText(0o755)).toBe("rwxr-xr-x");
    expect(modeToText(0o600)).toBe("rw-------");
    expect(parseMode("755")).toBe(0o755);
    expect(parseMode(" 0644 ")).toBe(0o644);
    for (const bad of ["", "9", "75", "75555", "rwx", "8"]) expect(parseMode(bad)).toBeNull();
  });

  it("works out progress", () => {
    expect(percent(50, 200)).toBe(25);
    expect(percent(300, 200)).toBe(100);
    expect(percent(5, null)).toBeNull();
    expect(percent(5, 0)).toBeNull();
  });
});

describe("shellQuote", () => {
  it("keeps odd names literal", () => {
    expect(shellQuote("/home/a b")).toBe("'/home/a b'");
    expect(shellQuote("/tmp/it's; rm -rf ~")).toBe("'/tmp/it'\\''s; rm -rf ~'");
    expect(shellQuote("$(whoami)")).toBe("'$(whoami)'");
  });
});
