import { describe, expect, it } from "vitest";
import draft from "../../../../tests/fixtures/research/structure-BUMI.draft.json";
import type { StructureKind, StructureNode } from "../research/types";
import { expandableIds, isNarrow, layoutMindMap, NARROW_INDENT, NARROW_NODE_H, narrowNodeWidth, NODE_H, NODE_W, splitSides, TOGGLE, type MindMapLayout, type PlacedNode, type Rect } from "./mindmapLayout";
import { buildTree, type TreeNode } from "./structureData";

/** Draf BUMI nyata (label, induk, jenis, catatan dari laporan tahunan) dalam bentuk payload backend. */
const BUMI: StructureNode[] = draft.nodes.map((n) => ({
  id: n.id,
  parent: n.parent,
  label: n.label,
  kind: n.kind as StructureKind,
  kind_label: n.kind,
  metric: "metric" in n ? (n.metric as string) : null,
  metric_label: null,
  note: n.note ?? null,
  state: "proposed",
  state_label: "usulan",
  owner: false,
  source: n.source ?? null,
}));

const overlaps = (a: Rect, b: Rect) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

const leaves = (t: TreeNode): number => (t.children.length ? t.children.reduce((s, c) => s + leaves(c), 0) : 1);

const ALL = new Set(expandableIds(BUMI));
const NONE_OPEN: ReadonlySet<string> = new Set();

describe("mind map layout on the real BUMI structure, every branch open", () => {
  const layout = layoutMindMap(BUMI, ALL);
  const centreX = layout.centre.x + layout.centre.w / 2;
  const byId = new Map(layout.nodes.map((n) => [n.id, n]));

  it("places every node, each on exactly one side of the centre card", () => {
    expect(layout.nodes).toHaveLength(BUMI.length);
    for (const n of layout.nodes) {
      const mid = n.x + n.w / 2;
      expect(n.side === "right" ? mid > centreX : mid < centreX).toBe(true);
    }
  });

  it("never overlaps two cards, the centre card included", () => {
    const rects: Rect[] = [layout.centre, ...layout.nodes];
    for (let i = 0; i < rects.length; i += 1) {
      for (let j = i + 1; j < rects.length; j += 1) expect(overlaps(rects[i], rects[j]), `${i} vs ${j}`).toBe(false);
    }
  });

  it("puts every child farther from the centre than its parent, and the first branches next to the centre card", () => {
    const dist = (n: PlacedNode) => Math.abs(n.x + n.w / 2 - centreX);
    for (const n of layout.nodes) {
      if (n.parent === null) {
        expect(n.depth).toBe(1);
        continue;
      }
      const parent = byId.get(n.parent) as PlacedNode;
      expect(parent.side).toBe(n.side);
      expect(dist(n)).toBeGreaterThan(dist(parent));
    }
  });

  it("balances left and right leaves within the biggest depth-1 branch", () => {
    const { roots } = buildTree(BUMI);
    const biggest = Math.max(...roots.map(leaves));
    expect(layout.leaves.left + layout.leaves.right).toBe(BUMI.filter((n) => !BUMI.some((m) => m.parent === n.id)).length);
    expect(Math.abs(layout.leaves.left - layout.leaves.right)).toBeLessThanOrEqual(biggest);
    expect(layout.leaves.left).toBeGreaterThan(0);
    expect(layout.leaves.right).toBeGreaterThan(0);
  });

  it("keeps everything inside the reported box and draws one curved edge per node in its kind", () => {
    for (const r of [layout.centre, ...layout.nodes]) {
      expect(r.x).toBeGreaterThanOrEqual(0);
      expect(r.y).toBeGreaterThanOrEqual(0);
      expect(r.x + r.w).toBeLessThanOrEqual(layout.width);
      expect(r.y + r.h).toBeLessThanOrEqual(layout.height);
    }
    expect(layout.edges).toHaveLength(BUMI.length);
    for (const e of layout.edges) {
      expect(e.d).toMatch(/^M [\d.-]+ [\d.-]+ C /);
      expect(e.kind).toBe(BUMI.find((n) => n.id === e.id)?.kind);
    }
  });

  it("uses the card size constants and leaves rejected nodes out", () => {
    expect(layout.nodes.every((n) => n.w === NODE_W && n.h === NODE_H)).toBe(true);
    const withRejected = BUMI.map((n, i) => (i === 0 ? { ...n, state: "rejected" as const, state_label: "ditolak" as const } : n));
    const smaller = layoutMindMap(withRejected, ALL);
    expect(smaller.nodes.some((n) => n.id === BUMI[0].id)).toBe(false);
    // Anak simpul yang ditolak naik jadi anak pusat (sama dengan buildTree), jadi tidak ada yang hilang.
    expect(smaller.nodes).toHaveLength(BUMI.length - 1);
  });

  it("lays out an empty map as just the centre card", () => {
    const empty = layoutMindMap([]);
    expect(empty.nodes).toEqual([]);
    expect(empty.edges).toEqual([]);
    expect(empty.width).toBeGreaterThan(empty.centre.w);
  });
});

const noOverlap = (l: MindMapLayout) => {
  const rects: Rect[] = [l.centre, ...l.nodes];
  for (let i = 0; i < rects.length; i += 1) {
    for (let j = i + 1; j < rects.length; j += 1) expect(overlaps(rects[i], rects[j]), `${i} vs ${j}`).toBe(false);
  }
};

/** Tinggi sisi: dari tepi atas kartu tertinggi sampai tepi bawah kartu terendah di sisi itu. */
const sideHeight = (l: MindMapLayout, side: "left" | "right") => {
  const own = l.nodes.filter((n) => n.side === side);
  return Math.max(...own.map((n) => n.y + n.h)) - Math.min(...own.map((n) => n.y));
};

describe("mind map layout with branches closed (progressive disclosure)", () => {
  const { roots } = buildTree(BUMI);
  const closed = layoutMindMap(BUMI);

  it("draws only the depth-1 branches around the centre card, each one at least 44 px tall", () => {
    expect(closed.nodes.map((n) => n.id).sort()).toEqual(roots.map((r) => r.node.id).sort());
    expect(closed.nodes.every((n) => n.depth === 1 && n.h >= 44 && !n.expanded)).toBe(true);
    expect(closed.edges).toHaveLength(roots.length);
  });

  it("knows how many children each closed branch hides, so the card can say +N", () => {
    const byId = new Map(closed.nodes.map((n) => [n.id, n]));
    for (const r of roots) expect(byId.get(r.node.id)?.childCount).toBe(r.children.length);
    expect(roots.some((r) => r.children.length > 0)).toBe(true);
  });

  it("never overlaps and keeps both sides within 1.5x of each other in height", () => {
    noOverlap(closed);
    const left = sideHeight(closed, "left");
    const right = sideHeight(closed, "right");
    expect(Math.max(left, right) / Math.min(left, right)).toBeLessThanOrEqual(1.5);
  });

  it("opening one branch draws its children farther out, keeps every card apart and rebalances by visible leaves", () => {
    const biggest = roots.reduce((a, b) => (b.children.length > a.children.length ? b : a));
    const open = layoutMindMap(BUMI, new Set([biggest.node.id]));
    noOverlap(open);
    expect(open.nodes).toHaveLength(roots.length + biggest.children.length);
    const parent = open.nodes.find((n) => n.id === biggest.node.id) as PlacedNode;
    expect(parent.expanded).toBe(true);
    const kids = open.nodes.filter((n) => n.parent === biggest.node.id);
    expect(kids).toHaveLength(biggest.children.length);
    expect(kids.every((k) => k.side === parent.side)).toBe(true);
    // The wide branch takes a side to itself: the other branches stack on the opposite side.
    expect(open.nodes.filter((n) => n.depth === 1 && n.side === parent.side)).toEqual([parent]);
    expect(open.leaves[parent.side]).toBe(biggest.children.length);
    expect(open.leaves[parent.side === "left" ? "right" : "left"]).toBe(roots.length - 1);
  });

  it("an unknown id in the open set is ignored", () => {
    expect(layoutMindMap(BUMI, new Set(["nope"])).nodes).toHaveLength(roots.length);
  });
});

describe("mind map layout on a narrow canvas (one-sided, 393 px phone = 361 px canvas)", () => {
  const CANVAS = 361;
  const { roots } = buildTree(BUMI);
  const closed = layoutMindMap(BUMI, NONE_OPEN, CANVAS);

  it("puts the company card at the left and every depth-1 card to its right, each fully inside the canvas at scale 1", () => {
    expect(closed.nodes.map((n) => n.id).sort()).toEqual(roots.map((r) => r.node.id).sort());
    expect(closed.width).toBeLessThanOrEqual(CANVAS);
    expect(closed.centre.w).toBeLessThanOrEqual(130);
    for (const n of closed.nodes) {
      expect(n.side).toBe("right");
      expect(n.x).toBeGreaterThanOrEqual(closed.centre.x + closed.centre.w);
      expect(n.x + n.w).toBeLessThanOrEqual(CANVAS);
      // The +N chip beside the card is on the canvas too.
      if (n.childCount > 0) expect(n.x + n.w + 4 + TOGGLE).toBeLessThanOrEqual(CANVAS);
    }
    expect(closed.leaves.left).toBe(0);
    expect(closed.leaves.right).toBe(roots.length);
    noOverlap(closed);
  });

  it("keeps payload order from top to bottom and one curved edge per card, ending on the card's left edge", () => {
    expect(closed.nodes.map((n) => n.id)).toEqual(roots.map((r) => r.node.id));
    const ys = closed.nodes.map((n) => n.y);
    expect(ys).toEqual([...ys].sort((a, b) => a - b));
    expect(closed.edges).toHaveLength(roots.length);
    for (const n of closed.nodes) expect(closed.edges.find((e) => e.id === n.id)?.d.endsWith(`${n.x} ${n.y + n.h / 2}`)).toBe(true);
  });

  it("opens a branch below its parent as an indented outline: children right after it, one indent further in, same right edge", () => {
    const biggest = roots.reduce((a, b) => (b.children.length > a.children.length ? b : a));
    const open = layoutMindMap(BUMI, new Set([biggest.node.id]), CANVAS);
    noOverlap(open);
    const order = open.nodes.map((n) => n.id);
    const at = order.indexOf(biggest.node.id);
    const parent = open.nodes[at];
    const kids = open.nodes.filter((n) => n.parent === biggest.node.id);
    expect(kids).toHaveLength(biggest.children.length);
    // Rows run top to bottom in tree order: the parent, then its children, then the next depth-1 branch.
    expect(order.slice(at + 1, at + 1 + kids.length)).toEqual(biggest.children.map((c) => c.node.id));
    for (const k of kids) {
      expect(k.x).toBe(parent.x + NARROW_INDENT);
      expect(k.x + k.w).toBe(parent.x + parent.w);
      expect(k.y).toBeGreaterThan(parent.y + parent.h);
      expect(k.h).toBe(NARROW_NODE_H);
      // The connector drops from the parent's bottom edge and bends into the child's left edge.
      const d = open.edges.find((e) => e.id === k.id)?.d ?? "";
      expect(d.startsWith(`M ${parent.x + NARROW_INDENT / 2} ${parent.y + parent.h}`)).toBe(true);
      expect(d.endsWith(`${k.x} ${k.y + k.h / 2}`)).toBe(true);
    }
    const ys = open.nodes.map((n) => n.y);
    expect(ys).toEqual([...ys].sort((a, b) => a - b));
    expect(open.nodes.every((n) => n.side === "right")).toBe(true);
    noOverlap(layoutMindMap(BUMI, ALL, CANVAS));
  });

  it("with a branch open the company card moves to the top of the stack, so the tree start (centre and first branch) is one view", () => {
    expect(closed.centre.y).toBeGreaterThan(closed.nodes[0].y);
    const open = layoutMindMap(BUMI, ALL, CANVAS);
    expect(open.centre.y).toBe(open.nodes[0].y);
    expect(open.centre.x + open.centre.w).toBeLessThan(open.nodes[0].x);
  });

  it("stacks the six BUMI depth-1 cards compactly: 80 px tall, 8 px apart, the whole stack and the company card within 540 px (G4)", () => {
    expect(closed.nodes).toHaveLength(6);
    for (const n of closed.nodes) expect(n.h).toBe(NARROW_NODE_H);
    for (let i = 1; i < closed.nodes.length; i++) expect(closed.nodes[i].y - (closed.nodes[i - 1].y + closed.nodes[i - 1].h)).toBe(8);
    expect(closed.height).toBeLessThanOrEqual(540);
    expect(closed.centre.y).toBeGreaterThanOrEqual(0);
    expect(closed.centre.y + closed.centre.h).toBeLessThanOrEqual(closed.height);
  });

  it("no card or chip ever runs past the canvas width, in every expand state and on every phone width (bug: n1 open cut 8 cards)", () => {
    const states: ReadonlySet<string>[] = [NONE_OPEN, ALL, ...[...ALL].map((id) => new Set([id]))];
    for (const canvas of [320, 343, 355, CANVAS, 412, 599]) {
      for (const open of states) {
        const l = layoutMindMap(BUMI, open, canvas);
        expect(l.width).toBeLessThanOrEqual(canvas);
        for (const n of l.nodes) {
          expect(n.x).toBeGreaterThanOrEqual(l.centre.x + l.centre.w);
          expect(n.x + n.w + (n.childCount > 0 ? 4 + TOGGLE : 0)).toBeLessThanOrEqual(canvas);
          // Deepest BUMI card (depth 3) on the narrowest phone is still a real card.
          expect(n.w).toBeGreaterThanOrEqual(80);
        }
      }
    }
  });

  it("widens depth-1 cards to the free width, between 112 and 200 px", () => {
    expect(narrowNodeWidth(361)).toBeGreaterThanOrEqual(NODE_W);
    expect(narrowNodeWidth(361)).toBeLessThanOrEqual(200);
    expect(narrowNodeWidth(599)).toBe(200);
    expect(narrowNodeWidth(320)).toBe(320 - 200);
    expect(narrowNodeWidth(280)).toBe(112);
    expect(layoutMindMap(BUMI, NONE_OPEN, 560).nodes[0].w).toBe(200);
  });

  it("is two-sided again from 600 px, and when no width is given", () => {
    expect(isNarrow(599)).toBe(true);
    expect(isNarrow(600)).toBe(false);
    expect(isNarrow(0)).toBe(false);
    expect(isNarrow(null)).toBe(false);
    for (const wide of [layoutMindMap(BUMI, NONE_OPEN, 600), layoutMindMap(BUMI, NONE_OPEN, null), layoutMindMap(BUMI)]) {
      expect(wide.leaves.left).toBeGreaterThan(0);
      expect(wide.nodes.every((n) => n.w === NODE_W)).toBe(true);
    }
  });
});

describe("splitSides", () => {
  const leaf = (id: string): TreeNode => ({ node: { id } as StructureNode, depth: 1, children: [] });
  const branch = (id: string, kids: number): TreeNode => ({ node: { id } as StructureNode, depth: 1, children: Array.from({ length: kids }, (_, i) => leaf(`${id}.${i}`)) });

  it("places the heaviest branch first, the rest on the lighter side, and keeps payload order inside a side", () => {
    const sides = splitSides([leaf("a"), branch("b", 5), leaf("c"), leaf("d"), branch("e", 2)]);
    expect(sides.right.map((t) => t.node.id)).toEqual(["b"]);
    expect(sides.left.map((t) => t.node.id)).toEqual(["a", "c", "d", "e"]);
  });

  it("alternates equal branches starting on the right", () => {
    const sides = splitSides([leaf("a"), leaf("b"), leaf("c")]);
    expect(sides.right.map((t) => t.node.id)).toEqual(["a", "c"]);
    expect(sides.left.map((t) => t.node.id)).toEqual(["b"]);
  });
});

describe("expandableIds", () => {
  it("lists the nodes that have children, rejected children not counted", () => {
    const ids = expandableIds(BUMI);
    expect(ids).toContain(buildTree(BUMI).roots[0].node.id);
    expect(ids.every((id) => BUMI.some((n) => n.parent === id))).toBe(true);
    const rejectedKids = BUMI.map((n) => (n.parent === "n25" ? { ...n, state: "rejected" as const, state_label: "ditolak" as const } : n));
    expect(expandableIds(rejectedKids)).not.toContain("n25");
  });
});
