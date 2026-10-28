import type { StructureNode } from "../research/types";

export interface TreeNode {
  node: StructureNode;
  /** Anak langsung akar = 1 (batas `limits.max_depth`). */
  depth: number;
  children: TreeNode[];
}

export interface StructureTreeData {
  roots: TreeNode[];
  /** Simpul yang ditolak: tidak masuk pohon, bisa diterima lagi. */
  rejected: StructureNode[];
}

/**
 * Daftar simpul datar → pohon top-down. Simpul ditolak keluar dari pohon; simpul yang induknya ditolak atau tidak ada
 * naik menjadi anak akar supaya tidak pernah hilang dari layar. Urutan saudara mengikuti urutan payload.
 */
export function buildTree(nodes: readonly StructureNode[]): StructureTreeData {
  const rejected = nodes.filter((n) => n.state === "rejected");
  const visible = nodes.filter((n) => n.state !== "rejected");
  const ids = new Set(visible.map((n) => n.id));
  const childrenOf = new Map<string | null, StructureNode[]>();
  for (const n of visible) {
    const parent = n.parent !== null && ids.has(n.parent) ? n.parent : null;
    childrenOf.set(parent, [...(childrenOf.get(parent) ?? []), n]);
  }
  const build = (parent: string | null, depth: number, seen: Set<string>): TreeNode[] =>
    (childrenOf.get(parent) ?? [])
      .filter((n) => !seen.has(n.id))
      .map((node) => ({ node, depth, children: build(node.id, depth + 1, new Set([...seen, node.id])) }));
  return { roots: build(null, 1, new Set()), rejected };
}

/** Jumlah simpul usulan yang menunggu keputusan. */
export const proposedCount = (nodes: readonly StructureNode[]): number => nodes.filter((n) => n.state === "proposed").length;

/** `hal. 37` untuk halaman PDF, `slide 12` untuk presentasi (nama berkas berakhiran .ppt/.pptx). */
export function pageRef(node: StructureNode): string | null {
  if (!node.source) return null;
  return /\.pptx?$/i.test(node.source.document) ? `slide ${node.source.page}` : `hal. ${node.source.page}`;
}
