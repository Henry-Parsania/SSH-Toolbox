import { describe, expect, it } from "vitest";
import { compareFolders, relativeTo, type Item } from "./compare";

const map = (o: Record<string, Item>) => new Map(Object.entries(o));
const f = (size: number, modified: number | null): Item => ({ size, modified });

describe("compareFolders", () => {
  it("sorts files into only here, only there, newer on one side, and the same", () => {
    const c = compareFolders(
      map({ "a.txt": f(1, 100), "new.txt": f(5, 100), "both-same.txt": f(3, 1000), "mine-newer.txt": f(3, 2000), "theirs-newer.txt": f(4, 1000) }),
      map({ "a.txt": f(1, 101), "old.txt": f(2, 50), "both-same.txt": f(3, 1000), "mine-newer.txt": f(2, 1000), "theirs-newer.txt": f(4, 3000) }),
    );
    expect(c.onlyLocal).toEqual(["new.txt"]);
    expect(c.onlyRemote).toEqual(["old.txt"]);
    expect(c.localNewer).toEqual(["mine-newer.txt"]);
    expect(c.remoteNewer).toEqual(["theirs-newer.txt"]);
    expect(c.same).toBe(2); // a.txt (1 s apart) and both-same.txt
  });

  it("flags a size difference with the same time instead of guessing", () => {
    const c = compareFolders(map({ "x": f(1, 100) }), map({ "x": f(2, 100) }));
    expect(c.differs).toEqual(["x"]);
    const unknown = compareFolders(map({ "x": f(1, null) }), map({ "x": f(2, 100) }));
    expect(unknown.differs).toEqual(["x"]);
  });
});

describe("relativeTo", () => {
  it("cuts the folder off and uses / between names", () => {
    expect(relativeTo("/srv/app", "/srv/app/sub/a.txt")).toBe("sub/a.txt");
    expect(relativeTo("C:\\work\\app\\", "C:\\work\\app\\sub\\a.txt")).toBe("sub/a.txt");
    expect(relativeTo("/", "/etc/hosts")).toBe("etc/hosts");
  });
});
