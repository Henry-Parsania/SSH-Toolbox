import { describe, expect, it } from "vitest";
import {
  closePane,
  findLeaf,
  focusAfterClose,
  leaves,
  moveDivider,
  neighbour,
  placeAll,
  newLeaf,
  splitPane,
  updateLeaf,
  type PaneNode,
} from "./panes";

const leaf = (id: string) => newLeaf(null, id, id);
const ids = (node: PaneNode) => leaves(node).map((l) => l.pane_id);
const sum = (n: number[]) => n.reduce((a, b) => a + b, 0);

/** Checks the rules every tree must follow. */
function assertValid(node: PaneNode) {
  if (node.kind === "leaf") return;
  expect(node.children.length).toBeGreaterThanOrEqual(2);
  expect(node.sizes.length).toBe(node.children.length);
  expect(sum(node.sizes)).toBeCloseTo(1, 9);
  for (const c of node.children) {
    if (c.kind === "split") expect(c.direction).not.toBe(node.direction);
    assertValid(c);
  }
}

describe("splitPane", () => {
  it("turns a single pane into a split of two halves", () => {
    const t = splitPane(leaf("a"), "a", "row", leaf("b"));
    expect(t).toMatchObject({ kind: "split", direction: "row", sizes: [0.5, 0.5] });
    expect(ids(t)).toEqual(["a", "b"]);
  });

  it("adds to the same split when it goes the same way, taking half of that pane", () => {
    let t = splitPane(leaf("a"), "a", "row", leaf("b"));
    t = splitPane(t, "a", "row", leaf("c"));
    expect(ids(t)).toEqual(["a", "c", "b"]);
    expect(t.kind === "split" && t.sizes).toEqual([0.25, 0.25, 0.5]);
    assertValid(t);
  });

  it("nests when the direction differs", () => {
    let t = splitPane(leaf("a"), "a", "row", leaf("b"));
    t = splitPane(t, "b", "column", leaf("c"));
    expect(t.kind === "split" && t.children[1]).toMatchObject({ kind: "split", direction: "column" });
    expect(ids(t)).toEqual(["a", "b", "c"]);
    assertValid(t);
  });

  it("splits a pane deep in the tree", () => {
    let t = splitPane(leaf("a"), "a", "row", leaf("b"));
    t = splitPane(t, "b", "column", leaf("c"));
    t = splitPane(t, "c", "row", leaf("d"));
    expect(ids(t)).toEqual(["a", "b", "c", "d"]);
    assertValid(t);
  });

  it("does nothing for an unknown pane", () => {
    const t = leaf("a");
    expect(splitPane(t, "zzz", "row", leaf("b"))).toBe(t);
  });

  it("does not change the tree it was given", () => {
    const before = splitPane(leaf("a"), "a", "row", leaf("b"));
    const copy = structuredClone(before);
    splitPane(before, "a", "row", leaf("c"));
    expect(before).toEqual(copy);
  });
});

describe("closePane", () => {
  it("closing the only pane leaves nothing", () => {
    expect(closePane(leaf("a"), "a")).toBeNull();
  });

  it("closing one of two leaves the other as the whole tree", () => {
    const t = splitPane(leaf("a"), "a", "row", leaf("b"));
    expect(closePane(t, "a")).toMatchObject({ kind: "leaf", pane_id: "b" });
  });

  it("closing the last child of a split removes the split", () => {
    let t = splitPane(leaf("a"), "a", "row", leaf("b"));
    t = splitPane(t, "b", "column", leaf("c"));
    const result = closePane(t, "c")!;
    expect(ids(result)).toEqual(["a", "b"]);
    expect(result.kind === "split" && result.children.every((c) => c.kind === "leaf")).toBe(true);
    assertValid(result);
  });

  it("gives the freed space to the others in proportion", () => {
    let t = splitPane(leaf("a"), "a", "row", leaf("b"));
    t = splitPane(t, "a", "row", leaf("c")); // a .25, c .25, b .5
    const result = closePane(t, "c")!;
    expect(result.kind === "split" && result.sizes[0]).toBeCloseTo(1 / 3, 9);
    expect(result.kind === "split" && result.sizes[1]).toBeCloseTo(2 / 3, 9);
    assertValid(result);
  });

  it("merges a collapsed split into a parent going the same way", () => {
    // row [ a, column [ b, row [ c, d ] ] ]  -> close b -> row [ a, c, d ]
    let t = splitPane(leaf("a"), "a", "row", leaf("b"));
    t = splitPane(t, "b", "column", leaf("c"));
    t = splitPane(t, "c", "row", leaf("d"));
    const result = closePane(t, "b")!;
    expect(ids(result)).toEqual(["a", "c", "d"]);
    expect(result.kind === "split" && result.children.length).toBe(3);
    assertValid(result);
  });

  it("does nothing for an unknown pane", () => {
    const t = splitPane(leaf("a"), "a", "row", leaf("b"));
    expect(closePane(t, "zzz")).toBe(t);
  });

  it("can close every pane one by one without breaking the rules", () => {
    let t: PaneNode | null = leaf("a");
    t = splitPane(t, "a", "row", leaf("b"));
    t = splitPane(t, "b", "column", leaf("c"));
    t = splitPane(t, "a", "column", leaf("d"));
    t = splitPane(t, "c", "row", leaf("e"));
    for (const id of ["c", "a", "e", "d"]) {
      t = closePane(t!, id);
      assertValid(t!);
    }
    expect(t).toMatchObject({ kind: "leaf", pane_id: "b" });
    expect(closePane(t!, "b")).toBeNull();
  });
});

describe("moveDivider", () => {
  const two = () => splitPane(leaf("a"), "a", "row", leaf("b"));

  it("moves the boundary and keeps the total", () => {
    const t = moveDivider(two(), [], 0, 0.2);
    expect(t.kind === "split" && t.sizes[0]).toBeCloseTo(0.7, 9);
    expect(t.kind === "split" && t.sizes[1]).toBeCloseTo(0.3, 9);
    assertValid(t);
  });

  it("stops at the minimum on both sides", () => {
    const grown = moveDivider(two(), [], 0, 5);
    expect(grown.kind === "split" && grown.sizes[1]).toBeCloseTo(0.05, 9);
    const shrunk = moveDivider(two(), [], 0, -5);
    expect(shrunk.kind === "split" && shrunk.sizes[0]).toBeCloseTo(0.05, 9);
  });

  it("only changes the two panes beside the divider", () => {
    let t = splitPane(leaf("a"), "a", "row", leaf("b"));
    t = splitPane(t, "b", "row", leaf("c")); // .5 .25 .25
    const moved = moveDivider(t, [], 1, 0.1);
    expect(moved.kind === "split" && moved.sizes[0]).toBe(0.5);
    expect(moved.kind === "split" && moved.sizes[1]).toBeCloseTo(0.35, 9);
    expect(moved.kind === "split" && moved.sizes[2]).toBeCloseTo(0.15, 9);
  });

  it("works on a split deeper in the tree", () => {
    let t = splitPane(leaf("a"), "a", "row", leaf("b"));
    t = splitPane(t, "b", "column", leaf("c"));
    const moved = moveDivider(t, [1], 0, 0.25);
    const inner = moved.kind === "split" && moved.children[1];
    expect(inner && inner.kind === "split" && inner.sizes[0]).toBeCloseTo(0.75, 9);
    assertValid(moved);
  });

  it("ignores a bad path, a bad divider or a bad amount", () => {
    const t = two();
    expect(moveDivider(t, [5], 0, 0.1)).toBe(t);
    expect(moveDivider(t, [], 1, 0.1)).toBe(t);
    expect(moveDivider(t, [], -1, 0.1)).toBe(t);
    expect(moveDivider(t, [], 0, NaN)).toBe(t);
  });
});

describe("neighbour", () => {
  // a | ( b over c )
  function grid() {
    let t = splitPane(leaf("a"), "a", "row", leaf("b"));
    t = splitPane(t, "b", "column", leaf("c"));
    return t;
  }

  it("finds panes in each direction", () => {
    const t = grid();
    expect(neighbour(t, "a", "right")).toBe("b");
    expect(neighbour(t, "b", "left")).toBe("a");
    expect(neighbour(t, "c", "left")).toBe("a");
    expect(neighbour(t, "b", "down")).toBe("c");
    expect(neighbour(t, "c", "up")).toBe("b");
  });

  it("returns null at an edge", () => {
    const t = grid();
    expect(neighbour(t, "a", "left")).toBeNull();
    expect(neighbour(t, "a", "up")).toBeNull();
    expect(neighbour(t, "a", "down")).toBeNull();
    expect(neighbour(t, "b", "right")).toBeNull();
    expect(neighbour(t, "b", "up")).toBeNull();
    expect(neighbour(t, "c", "down")).toBeNull();
  });

  it("picks the pane that overlaps most when several touch", () => {
    // ( a over b ) | c, with a taller than b
    let t = splitPane(leaf("c"), "c", "row", leaf("zz"));
    t = splitPane(t, "c", "column", leaf("b"));
    t = moveDivider(t, [0], 0, 0.3); // c = .8 of height, b = .2
    expect(neighbour(t, "zz", "left")).toBe("c");
  });

  it("returns null for an unknown pane or a single pane", () => {
    expect(neighbour(grid(), "nope", "left")).toBeNull();
    expect(neighbour(leaf("a"), "a", "right")).toBeNull();
  });
});

describe("helpers", () => {
  it("updateLeaf changes only the named pane", () => {
    const t = splitPane(leaf("a"), "a", "row", leaf("b"));
    const u = updateLeaf(t, "b", { title: "renamed" });
    expect(findLeaf(u, "b")!.title).toBe("renamed");
    expect(findLeaf(u, "a")!.title).toBe("a");
    expect(findLeaf(t, "b")!.title).toBe("b");
  });

  it("focusAfterClose prefers the previous pane, then the next", () => {
    let t = splitPane(leaf("a"), "a", "row", leaf("b"));
    t = splitPane(t, "b", "row", leaf("c"));
    expect(focusAfterClose(t, "b")).toBe("a");
    expect(focusAfterClose(t, "a")).toBe("b");
    expect(focusAfterClose(leaf("a"), "a")).toBeNull();
  });

  it("gives each new pane its own id", () => {
    expect(newLeaf(null, "x").pane_id).not.toBe(newLeaf(null, "x").pane_id);
  });
});

describe("placeAll", () => {
  it("lets each column of a grid have its own horizontal divider", () => {
    const [a, b, c, d] = ["a", "b", "c", "d"].map((id) => newLeaf(null, id, id));
    let tree: PaneNode = splitPane(a, "a", "row", b);
    tree = splitPane(tree, "a", "column", c);
    tree = splitPane(tree, "b", "column", d);
    const before = placeAll(tree);
    expect(before.panes).toHaveLength(4);
    expect(before.dividers.filter((x) => x.direction === "column")).toHaveLength(2);

    const left = before.dividers.find((x) => x.direction === "column" && x.split.left === 0)!;
    tree = moveDivider(tree, left.path, left.index, 0.2);
    const after = placeAll(tree);
    const edge = (id: string) => after.panes.find((p) => p.leaf.pane_id === id)!.rect.bottom;
    expect(edge("a")).toBeCloseTo(0.7);
    expect(edge("b")).toBeCloseTo(0.5);
  });
});
