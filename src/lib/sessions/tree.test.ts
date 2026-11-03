import { describe, expect, it } from "vitest";
import type { SavedSession, SessionsFile } from "../backend";
import {
  buildTree,
  checkSshForm,
  folderChoices,
  removeFolder,
  removeSession,
  wouldLoop,
  type TreeItem,
} from "./tree";

const ssh = (id: string, name: string, folder = "", host = "h.example", jump = ""): SavedSession => ({
  type: "ssh",
  id,
  name,
  folder,
  host,
  port: 22,
  user: "u",
  auth: "agent",
  key_path: "",
  jump,
  x11: false,
});

const file = (): SessionsFile => ({
  version: 1,
  folder: [
    { id: "f1", name: "Work", parent: "" },
    { id: "f2", name: "Lab", parent: "f1" },
    { id: "f3", name: "Home", parent: "" },
  ],
  session: [
    ssh("s1", "build", "f1", "build.example.com"),
    ssh("s2", "bench", "f2", "10.0.0.5"),
    ssh("s3", "alpha"),
    {
      type: "serial",
      id: "s4",
      name: "router",
      folder: "f3",
      port: "COM3",
      baud: 115200,
      data_bits: 8,
      parity: "none",
      stop_bits: 1,
      flow_control: "none",
      local_echo: false,
      line_ending: "cr",
    },
  ],
});

const names = (items: TreeItem[]): string[] =>
  items.map((i) => (i.kind === "folder" ? `[${i.folder.name}]` : i.session.name));

describe("buildTree", () => {
  it("puts folders first, sorted, with sessions inside them", () => {
    const tree = buildTree(file());
    expect(names(tree)).toEqual(["[Home]", "[Work]", "alpha"]);
    const work = tree[1];
    expect(work.kind === "folder" && names(work.children)).toEqual(["[Lab]", "build"]);
  });

  it("search keeps matching sessions and the folders that hold them", () => {
    const tree = buildTree(file(), "bench");
    expect(names(tree)).toEqual(["[Work]"]);
    const work = tree[0];
    expect(work.kind === "folder" && names(work.children)).toEqual(["[Lab]"]);
  });

  it("search matches the host and the serial port, ignoring case", () => {
    expect(names(buildTree(file(), "BUILD.EXAMPLE"))).toEqual(["[Work]"]);
    expect(names(buildTree(file(), "com3"))).toEqual(["[Home]"]);
    expect(buildTree(file(), "nothing-like-this")).toEqual([]);
  });

  it("a session whose folder is missing still appears at the top", () => {
    const f = file();
    f.session.push(ssh("s9", "orphan", "gone"));
    expect(names(buildTree(f))).toContain("orphan");
  });
});

describe("folderChoices", () => {
  it("labels nested folders with their path", () => {
    expect(folderChoices(file().folder).map((c) => c.label)).toEqual(["Home", "Work", "Work / Lab"]);
  });
});

describe("checkSshForm", () => {
  const ok = { name: "n", host: "h", port: 22, user: "u", auth: "agent" as const, key_path: "" };

  it("accepts a good form", () => {
    expect(checkSshForm(ok)).toBeNull();
    expect(checkSshForm({ ...ok, port: "2222" })).toBeNull();
  });

  it("explains the first problem", () => {
    expect(checkSshForm({ ...ok, name: " " })).toMatch(/name/);
    expect(checkSshForm({ ...ok, host: "" })).toMatch(/host/);
    expect(checkSshForm({ ...ok, host: "a b" })).toMatch(/spaces/);
    expect(checkSshForm({ ...ok, port: 0 })).toMatch(/port/);
    expect(checkSshForm({ ...ok, port: 70000 })).toMatch(/port/);
    expect(checkSshForm({ ...ok, port: "x" })).toMatch(/port/);
    expect(checkSshForm({ ...ok, user: "" })).toMatch(/user/);
    expect(checkSshForm({ ...ok, auth: "key" })).toMatch(/key/);
    expect(checkSshForm({ ...ok, auth: "key", key_path: "~/.ssh/id" })).toBeNull();
  });
});

describe("removing things", () => {
  it("removing a folder moves its contents up one level", () => {
    const after = removeFolder(file(), "f1");
    expect(after.folder.map((f) => f.id)).toEqual(["f2", "f3"]);
    expect(after.folder.find((f) => f.id === "f2")?.parent).toBe("");
    expect(after.session.find((s) => s.id === "s1")?.folder).toBe("");
    expect(after.session.find((s) => s.id === "s2")?.folder).toBe("f2");
  });

  it("removing a session clears it as a jump host elsewhere", () => {
    const f = file();
    f.session.push(ssh("s5", "behind", "", "x", "s3"));
    const after = removeSession(f, "s3");
    expect(after.session.some((s) => s.id === "s3")).toBe(false);
    expect(after.session.find((s) => s.id === "s5")).toMatchObject({ jump: "" });
  });

  it("removing something that isn't there changes nothing", () => {
    const f = file();
    expect(removeFolder(f, "zzz")).toBe(f);
  });
});

describe("wouldLoop", () => {
  it("catches moving a folder into itself or its own child", () => {
    const folders = file().folder;
    expect(wouldLoop(folders, "f1", "f1")).toBe(true);
    expect(wouldLoop(folders, "f1", "f2")).toBe(true);
    expect(wouldLoop(folders, "f2", "f3")).toBe(false);
    expect(wouldLoop(folders, "f2", "")).toBe(false);
  });
});
