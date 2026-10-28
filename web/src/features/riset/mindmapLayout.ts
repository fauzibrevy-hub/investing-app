import type { StructureNode } from "../research/types";
import { buildTree, type TreeNode } from "./structureData";

/** Ukuran kartu dalam piksel kanvas (sebelum zoom). Tinggi tetap supaya tata letak bisa dihitung tanpa mengukur DOM. */
export const NODE_W = 144;
export const NODE_H = 128;
export const CENTRE_W = 152;
export const CENTRE_H = 144;
/** Tombol buka / tutup cabang: kotak 44 px di ruang antar-kolom, di sisi luar kartu induk. */
export const TOGGLE = 44;

/** Antar-kolom cukup lebar untuk tombol `TOGGLE` plus sisa garis lengkung sebelum kartu anak. */
const GAP_X = 76;
const GAP_CENTRE = 20;
const GAP_Y = 12;
const GAP_BRANCH = 16;
const PAD = 24;

/** Kanvas lebih sempit dari ini = peta satu sisi: kartu pusat di tepi kiri, cabang bertumpuk di kanannya. */
export const NARROW_BELOW = 600;
export const isNarrow = (canvasWidth: number | null | undefined): canvasWidth is number => canvasWidth != null && canvasWidth > 0 && canvasWidth < NARROW_BELOW;

/** Ukuran peta satu sisi: kartu pusat, bantalan, jarak ke cabang, dan batas lebar kartu cabang (dihitung dari lebar kanvas). */
const NARROW_CENTRE_W = 120;
const NARROW_PAD = 6;
const NARROW_GAP_CENTRE = 20;
const NARROW_NODE_MIN = 112;
const NARROW_NODE_MAX = 200;
/** Kartu ringkas di kanvas sempit: enam cabang kedalaman 1 (6 x 80 + 5 x 8 + bantalan) muat setinggi layar ponsel. */
export const NARROW_NODE_H = 80;
const NARROW_GAP_Y = 8;
/** Kanvas sempit: anak tampil di bawah induknya, menjorok sejauh ini per kedalaman; garisnya turun di tengah jarak itu. */
export const NARROW_INDENT = 12;
/** Ruang tombol buka / tutup di sisi luar kartu: jarak 4 px + `TOGGLE`. */
const TOGGLE_ROOM = 4 + TOGGLE;

/**
 * Lebar kartu cabang kedalaman 1 di kanvas sempit: sisa lebar setelah kartu pusat, jarak, tombol, dan bantalan, di antara
 * 112 dan 200 px. Kartu yang lebih dalam menjorok `NARROW_INDENT` per kedalaman dengan tepi kanan yang sama, jadi peta
 * tetap selebar kanvas di keadaan buka apa pun (kanvas >= 312 px).
 */
export const narrowNodeWidth = (canvasWidth: number): number =>
  Math.min(NARROW_NODE_MAX, Math.max(NARROW_NODE_MIN, Math.floor(canvasWidth - (2 * NARROW_PAD + NARROW_CENTRE_W + NARROW_GAP_CENTRE + TOGGLE_ROOM))));

export type Side = "left" | "right";

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface PlacedNode extends Rect {
  id: string;
  node: StructureNode;
  /** Anak langsung pusat = 1, sama dengan `TreeNode.depth`. */
  depth: number;
  side: Side;
  /** `null` = induknya kartu pusat. */
  parent: string | null;
  /** Jumlah anak langsung di data (tidak ditolak), tergambar atau tidak. */
  childCount: number;
  /** Anaknya sedang tergambar. */
  expanded: boolean;
}

export interface MapEdge {
  /** Id simpul anak ujung garis. */
  id: string;
  from: string | null;
  /** Jenis anak: warna garis mengikuti jenis. */
  kind: StructureNode["kind"];
  /** Kurva Bézier kubik dari tepi induk ke tepi anak. */
  d: string;
}

export interface MindMapLayout {
  width: number;
  height: number;
  centre: Rect;
  nodes: PlacedNode[];
  edges: MapEdge[];
  /** Jumlah daun di tiap sisi (untuk tes keseimbangan). */
  leaves: Record<Side, number>;
}

const leafCount = (t: TreeNode): number => (t.children.length ? t.children.reduce((sum, c) => sum + leafCount(c), 0) : 1);

/**
 * Bagi cabang kedalaman 1 ke kiri dan kanan menurut bobot daun yang tergambar: yang terbesar dulu, tiap cabang ke sisi
 * yang bebannya lebih ringan (seri ke kanan). Di dalam sisi, urutan payload dipertahankan. Selisih daun kedua sisi tak
 * pernah melebihi daun cabang terbesar.
 */
export function splitSides(roots: readonly TreeNode[]): Record<Side, TreeNode[]> {
  const order = roots.map((root, index) => ({ root, index, weight: leafCount(root) })).sort((a, b) => b.weight - a.weight || a.index - b.index);
  const picked: Record<Side, typeof order> = { left: [], right: [] };
  const load: Record<Side, number> = { left: 0, right: 0 };
  for (const entry of order) {
    const side: Side = load.left < load.right ? "left" : "right";
    picked[side].push(entry);
    load[side] += entry.weight;
  }
  const byPayload = (list: typeof order) => list.sort((a, b) => a.index - b.index).map((e) => e.root);
  return { left: byPayload(picked.left), right: byPayload(picked.right) };
}

/** Id simpul yang punya anak (tidak ditolak): yang bisa dibuka. */
export function expandableIds(nodes: readonly StructureNode[]): string[] {
  const out: string[] = [];
  const walk = (list: readonly TreeNode[]) => {
    for (const t of list) {
      if (t.children.length) out.push(t.node.id);
      walk(t.children);
    }
  };
  walk(buildTree(nodes).roots);
  return out;
}

const NONE: ReadonlySet<string> = new Set();

/** Pohon yang hanya memuat anak simpul yang dibuka; simpul yang tertutup menjadi daun. */
const prune = (t: TreeNode, open: ReadonlySet<string>): TreeNode => ({ ...t, children: open.has(t.node.id) ? t.children.map((c) => prune(c, open)) : [] });

interface Raw {
  tree: TreeNode;
  parent: string | null;
  /** Titik tengah vertikal, relatif terhadap pusat peta. */
  cy: number;
}

/** Pohon horizontal rapi satu sisi: daun berurutan dari atas, induk di tengah anak-anaknya, sisi dipusatkan di y = 0. */
function layoutSide(roots: readonly TreeNode[]): Raw[] {
  const out: Raw[] = [];
  let next = 0;
  const walk = (tree: TreeNode, parent: string | null): number => {
    let cy: number;
    if (tree.children.length === 0) {
      cy = next + NODE_H / 2;
      next += NODE_H + GAP_Y;
    } else {
      const ys = tree.children.map((c) => walk(c, tree.node.id));
      cy = (ys[0] + ys[ys.length - 1]) / 2;
    }
    out.push({ tree, parent, cy });
    return cy;
  };
  for (const root of roots) {
    walk(root, null);
    next += GAP_BRANCH;
  }
  if (out.length === 0) return out;
  const top = Math.min(...out.map((r) => r.cy)) - NODE_H / 2;
  const bottom = Math.max(...out.map((r) => r.cy)) + NODE_H / 2;
  const shift = -(top + bottom) / 2;
  return out.map((r) => ({ ...r, cy: r.cy + shift }));
}

const edgePath = (x1: number, y1: number, x2: number, y2: number): string => {
  const mx = (x1 + x2) / 2;
  return `M ${x1} ${y1} C ${mx} ${y1}, ${mx} ${y2}, ${x2} ${y2}`;
};

/** Jumlah anak langsung (tidak ditolak) per simpul, seluruh pohon. */
const countChildren = (list: readonly TreeNode[], out = new Map<string, number>()): Map<string, number> => {
  for (const t of list) {
    out.set(t.node.id, t.children.length);
    countChildren(t.children, out);
  }
  return out;
};

/**
 * Peta pikiran dari daftar simpul: kartu perusahaan di tengah, cabang kedalaman 1 terbagi kiri dan kanan seimbang menurut
 * jumlah daun yang tergambar, tiap sisi pohon horizontal ke arah menjauh dari pusat. Hanya cabang kedalaman 1 dan anak
 * simpul di `expanded` yang tergambar; simpul ditolak tidak ikut. Semua koordinat bergeser agar kotak peta mulai di
 * (0, 0) dengan bantalan `PAD`. Bila `canvasWidth` sempit (< `NARROW_BELOW`), peta satu sisi (`layoutNarrow`).
 */
export function layoutMindMap(nodes: readonly StructureNode[], expanded: ReadonlySet<string> = NONE, canvasWidth: number | null = null): MindMapLayout {
  const { roots: all } = buildTree(nodes);
  const childCounts = countChildren(all);
  const roots = all.map((t) => prune(t, expanded));
  if (isNarrow(canvasWidth)) return layoutNarrow(roots, childCounts, canvasWidth);
  const sides = splitSides(roots);
  const placed: Array<Rect & { raw: Raw; side: Side }> = [];
  for (const side of ["left", "right"] as const) {
    for (const raw of layoutSide(sides[side])) {
      const offset = CENTRE_W / 2 + GAP_CENTRE + (raw.tree.depth - 1) * (NODE_W + GAP_X);
      const x = side === "right" ? offset : -offset - NODE_W;
      placed.push({ x, y: raw.cy - NODE_H / 2, w: NODE_W, h: NODE_H, raw, side });
    }
  }
  const centre: Rect = { x: -CENTRE_W / 2, y: -CENTRE_H / 2, w: CENTRE_W, h: CENTRE_H };
  const rects: Rect[] = [centre, ...placed];
  // Tombol buka / tutup di sisi luar kartu ikut menentukan batas kiri dan kanan peta, supaya tidak keluar dari kotaknya.
  const reach = (r: (typeof placed)[number]): [number, number] => {
    const room = childCounts.get(r.raw.tree.node.id) ? TOGGLE_ROOM : 0;
    return [r.x - (r.side === "left" ? room : 0), r.x + r.w + (r.side === "right" ? room : 0)];
  };
  const minX = Math.min(centre.x, ...placed.map((r) => reach(r)[0]));
  const maxX = Math.max(centre.x + centre.w, ...placed.map((r) => reach(r)[1]));
  const minY = Math.min(...rects.map((r) => r.y));
  const maxY = Math.max(...rects.map((r) => r.y + r.h));
  const dx = PAD - minX;
  const dy = PAD - minY;
  const move = (r: Rect): Rect => ({ x: r.x + dx, y: r.y + dy, w: r.w, h: r.h });

  const centreAt = move(centre);
  const out: PlacedNode[] = placed.map((p) => ({
    ...move(p),
    id: p.raw.tree.node.id,
    node: p.raw.tree.node,
    depth: p.raw.tree.depth,
    side: p.side,
    parent: p.raw.parent,
    childCount: childCounts.get(p.raw.tree.node.id) ?? 0,
    expanded: p.raw.tree.children.length > 0,
  }));
  const byId = new Map(out.map((n) => [n.id, n]));
  const edges: MapEdge[] = out.map((n) => {
    const from = n.parent === null ? centreAt : (byId.get(n.parent) as Rect);
    const right = n.side === "right";
    const x1 = right ? from.x + from.w : from.x;
    const x2 = right ? n.x : n.x + n.w;
    return { id: n.id, from: n.parent, kind: n.node.kind, d: edgePath(x1, from.y + from.h / 2, x2, n.y + n.h / 2) };
  });
  const sum = (list: readonly TreeNode[]) => list.reduce((s, t) => s + leafCount(t), 0);
  return {
    width: maxX - minX + 2 * PAD,
    height: maxY - minY + 2 * PAD,
    centre: centreAt,
    nodes: out,
    edges,
    leaves: { left: sum(sides.left), right: sum(sides.right) },
  };
}

/**
 * Peta satu sisi untuk kanvas sempit: kartu pusat di tepi kiri, cabang kedalaman 1 bertumpuk di kanannya dengan garis
 * lengkung dari kartu pusat. Anak cabang yang dibuka tampil di bawah induknya sebagai kerangka menjorok
 * (`NARROW_INDENT` per kedalaman, tepi kanan sama), dengan garis yang turun dari tepi bawah induk. Jadi lebar peta
 * selalu selebar kanvas, di keadaan buka apa pun: tidak ada kartu yang terpotong tepi kanvas pada skala 1, hanya
 * tinggi yang digeser. Selama semua tertutup, kartu pusat di tengah tumpukan; begitu ada yang dibuka, di puncaknya,
 * supaya awal pohon (pusat dan cabang pertama) terlihat bersama.
 */
function layoutNarrow(roots: readonly TreeNode[], childCounts: ReadonlyMap<string, number>, canvasWidth: number): MindMapLayout {
  const rootW = narrowNodeWidth(canvasWidth);
  const colX = NARROW_PAD + NARROW_CENTRE_W + NARROW_GAP_CENTRE;
  const rows: Array<{ tree: TreeNode; parent: string | null }> = [];
  const walk = (tree: TreeNode, parent: string | null) => {
    rows.push({ tree, parent });
    for (const c of tree.children) walk(c, tree.node.id);
  };
  for (const root of roots) walk(root, null);
  const stackH = rows.length ? rows.length * NARROW_NODE_H + (rows.length - 1) * NARROW_GAP_Y : 0;
  const anyOpen = roots.some((r) => r.children.length > 0);
  // Tumpukan dan kartu pusat berbagi sumbu tengah selama semua tertutup; yang lebih pendek dipusatkan pada yang lain.
  const stackTop = NARROW_PAD + (anyOpen ? 0 : Math.max(0, (CENTRE_H - stackH) / 2));
  const centre: Rect = { x: NARROW_PAD, y: NARROW_PAD + (anyOpen ? 0 : Math.max(0, (stackH - CENTRE_H) / 2)), w: NARROW_CENTRE_W, h: CENTRE_H };

  const out: PlacedNode[] = rows.map(({ tree, parent }, i) => {
    const indent = (tree.depth - 1) * NARROW_INDENT;
    return {
      x: colX + indent,
      y: stackTop + i * (NARROW_NODE_H + NARROW_GAP_Y),
      w: rootW - indent,
      h: NARROW_NODE_H,
      id: tree.node.id,
      node: tree.node,
      depth: tree.depth,
      side: "right",
      parent,
      childCount: childCounts.get(tree.node.id) ?? 0,
      expanded: tree.children.length > 0,
    };
  });
  const byId = new Map(out.map((n) => [n.id, n]));
  const edges: MapEdge[] = out.map((n) => {
    const cy = n.y + n.h / 2;
    if (n.parent === null) return { id: n.id, from: null, kind: n.node.kind, d: edgePath(centre.x + centre.w, centre.y + centre.h / 2, n.x, cy) };
    // Garis siku membulat: turun dari tepi bawah induk di tengah jarak jorok, lalu berbelok ke tepi kiri anak.
    const from = byId.get(n.parent) as Rect;
    const sx = from.x + NARROW_INDENT / 2;
    return { id: n.id, from: n.parent, kind: n.node.kind, d: `M ${sx} ${from.y + from.h} V ${cy - NARROW_INDENT / 2} Q ${sx} ${cy}, ${n.x} ${cy}` };
  });
  const hasToggle = out.some((n) => n.childCount > 0);
  return {
    width: colX + rootW + (hasToggle ? TOGGLE_ROOM : 0) + NARROW_PAD,
    height: Math.max(centre.y + centre.h, stackTop + stackH) + NARROW_PAD,
    centre,
    nodes: out,
    edges,
    leaves: { left: 0, right: roots.reduce((s, t) => s + leafCount(t), 0) },
  };
}
