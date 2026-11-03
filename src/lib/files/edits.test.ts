import { describe, expect, it } from "vitest";
import type { EditEvent } from "../backend/files";
import { EditSync, type EditDeps } from "./edits.svelte";

function fake(results: Array<"uploaded" | "remote_changed" | Error> = []) {
  const calls: string[] = [];
  let emit: (e: EditEvent) => void = () => {};
  const deps: EditDeps = {
    async open(_f, path, onEvent) {
      emit = onEvent;
      calls.push(`open ${path}`);
      return { id: "e1", name: "compose.yaml", local_path: "C:\\tmp\\compose.yaml" };
    },
    async upload(id, force) {
      calls.push(`upload ${id} ${force}`);
      const r = results.shift() ?? "uploaded";
      if (r instanceof Error) throw r;
      return r;
    },
    async reopen() {},
    async stop(id) {
      calls.push(`stop ${id}`);
    },
  };
  return { deps, calls, emit: (e: EditEvent) => emit(e) };
}
const tick = () => new Promise((r) => setTimeout(r, 0));

describe("EditSync", () => {
  it("asks before sending by default, and sends when told to", async () => {
    const f = fake();
    const s = new EditSync(f.deps);
    await s.open("files1", "/srv/compose.yaml");
    f.emit({ type: "changed" });
    await tick();
    expect(s.questions).toEqual([{ id: "e1", kind: "send" }]);
    expect(f.calls).toEqual(["open /srv/compose.yaml"]);
    await s.answer("e1", "send");
    expect(f.calls.at(-1)).toBe("upload e1 false");
    expect(s.items[0].state).toBe("sent");
    expect(s.questions).toEqual([]);
    expect(s.mode).toBe("ask");
  });

  it("'always' sends this time and from then on without asking", async () => {
    const f = fake();
    const s = new EditSync(f.deps);
    await s.open("files1", "/srv/compose.yaml");
    f.emit({ type: "changed" });
    await tick();
    await s.answer("e1", "always");
    expect(s.mode).toBe("auto");
    f.emit({ type: "changed" });
    await tick();
    expect(f.calls.filter((c) => c.startsWith("upload")).length).toBe(2);
    expect(s.questions).toEqual([]);
    s.setMode("ask");
  });

  it("'not now' keeps watching and sends nothing", async () => {
    const f = fake();
    const s = new EditSync(f.deps);
    await s.open("files1", "/srv/a.yaml");
    f.emit({ type: "changed" });
    await tick();
    await s.answer("e1", "later");
    expect(s.items[0].state).toBe("watching");
    expect(f.calls.some((c) => c.startsWith("upload"))).toBe(false);
  });

  it("asks again before replacing a server file that changed", async () => {
    const f = fake(["remote_changed", "uploaded"]);
    const s = new EditSync(f.deps);
    await s.open("files1", "/srv/a.yaml");
    f.emit({ type: "changed" });
    await tick();
    await s.answer("e1", "send");
    expect(s.questions).toEqual([{ id: "e1", kind: "overwrite" }]);
    expect(s.items[0].message).toMatch(/changed/);
    await s.answer("e1", "send");
    expect(f.calls.at(-1)).toBe("upload e1 true");
    expect(s.items[0].state).toBe("sent");
  });

  it("a failed send is shown and the copy is kept", async () => {
    const f = fake([Object.assign(new Error("x"), { message: "Permission denied." })]);
    const s = new EditSync(f.deps);
    await s.open("files1", "/srv/a.yaml");
    f.emit({ type: "changed" });
    await tick();
    await s.answer("e1", "send");
    expect(s.items[0]).toMatchObject({ state: "failed", message: "Permission denied." });
  });

  it("stopping removes it and deletes the copy", async () => {
    const f = fake();
    const s = new EditSync(f.deps);
    await s.open("files1", "/srv/a.yaml");
    f.emit({ type: "changed" });
    await tick();
    await s.stop("e1");
    expect(s.items).toEqual([]);
    expect(s.questions).toEqual([]);
    expect(f.calls.at(-1)).toBe("stop e1");
  });
});
