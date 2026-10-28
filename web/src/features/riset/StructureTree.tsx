import { useState, type ReactNode } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { Button } from "../../components/Button";
import { cn } from "../../lib/cn";
import type { StructureNode } from "../research/types";
import { buildTree, pageRef, type TreeNode } from "./structureData";
import { displayText } from "./structureText";

export interface StructureTreeProps {
  nodes: readonly StructureNode[];
  /** Kedalaman maksimum (`limits.max_depth`): simpul di kedalaman ini tidak bisa diberi cabang. */
  maxDepth: number;
  /** Id simpul yang sedang disimpan; tombolnya dimatikan. */
  busyId?: string | null;
  /** Nilai terakhir metrik yang ditautkan simpul (chip yang bisa diketuk), atau `null`. */
  renderMetric: (node: StructureNode) => ReactNode;
  /** Keputusan boleh async; peta pikiran menutup sheet setelahnya. */
  onAccept: (node: StructureNode) => void | Promise<unknown>;
  onReject: (node: StructureNode) => void | Promise<unknown>;
  onEdit: (node: StructureNode, trigger: HTMLElement) => void;
  onAddChild: (node: StructureNode, trigger: HTMLElement) => void;
}

const ACTION = "min-h-11 min-w-11 rounded-sm px-3 text-label font-bold";

function NodeCard({ tree, props, collapsed, toggle }: { tree: TreeNode; props: StructureTreeProps; collapsed: boolean; toggle: () => void }) {
  const { node } = tree;
  const proposed = node.state === "proposed";
  const busy = props.busyId === node.id;
  const hasChildren = tree.children.length > 0;
  const Chevron = collapsed ? ChevronRight : ChevronDown;
  const metric = props.renderMetric(node);
  const label = displayText(node.label);
  return (
    <div
      data-node={node.id}
      data-state={node.state}
      className={cn("flex min-w-0 flex-col rounded-md", proposed ? "border border-dashed border-text-2 px-3 py-1" : "border border-transparent py-0")}
    >
      <div className="flex min-w-0 items-start gap-1">
        {hasChildren ? (
          // Nama tetap; keadaan buka / lipat hanya di `aria-expanded` (nama yang ikut berganti dibaca dobel).
          <button
            type="button"
            aria-expanded={!collapsed}
            aria-label={`${tree.children.length} cabang di ${label}`}
            onClick={toggle}
            className="-ml-1 inline-flex h-11 w-11 shrink-0 items-center justify-center text-text-2"
          >
            <Chevron aria-hidden="true" size={18} strokeWidth={1.75} />
          </button>
        ) : (
          <span aria-hidden="true" className="inline-block h-11 w-3 shrink-0" />
        )}
        <div className="flex min-h-11 min-w-0 flex-1 flex-col justify-center py-1">
          <span className="break-words text-body font-semibold text-text">{label}</span>
          <span className="flex flex-wrap items-center gap-x-2 text-caption text-text-2">
            <span>{node.kind_label}</span>
            {node.owner ? <span className="inline-flex h-5 items-center rounded-sm bg-surface-sunken px-1.5 text-[11px] font-bold uppercase leading-4 tracking-wide text-text-2">Pemilik</span> : null}
            {proposed ? (
              <>
                <span className="inline-flex h-5 items-center rounded-sm bg-surface-sunken px-1.5 text-[11px] font-bold uppercase leading-4 tracking-wide text-text-2">usulan</span>
                {pageRef(node) ? <span>{pageRef(node)}</span> : null}
              </>
            ) : null}
          </span>
        </div>
      </div>
      {node.note ? <p className="ml-3 break-words text-caption text-text-2">{displayText(node.note)}</p> : null}
      {proposed && node.source ? (
        <p className="ml-3 break-words text-caption text-text-2">
          <span className="font-semibold">{node.source.document}</span>
          {`: “${node.source.snippet}”`}
        </p>
      ) : null}
      {metric ? <div className="ml-3 flex min-w-0 flex-wrap items-center">{metric}</div> : null}
      <div className="flex flex-wrap items-center">
        {proposed ? (
          <>
            <Button variant="ghost" loading={busy} onClick={() => props.onAccept(node)} aria-label={`Terima ${label}`} className={cn(ACTION, "text-accent")}>
              Terima
            </Button>
            <Button variant="ghost" onClick={(e) => props.onEdit(node, e.currentTarget)} aria-label={`Ubah ${label}`} className={cn(ACTION, "text-accent")}>
              Ubah
            </Button>
            <Button variant="ghost" tone="danger" loading={busy} onClick={() => props.onReject(node)} aria-label={`Tolak ${label}`} className={ACTION}>
              Tolak
            </Button>
          </>
        ) : (
          <>
            <Button variant="ghost" onClick={(e) => props.onEdit(node, e.currentTarget)} aria-label={`Ubah ${label}`} className={cn(ACTION, "text-accent")}>
              Ubah
            </Button>
            {tree.depth < props.maxDepth ? (
              <Button variant="ghost" onClick={(e) => props.onAddChild(node, e.currentTarget)} aria-label={`Tambah cabang di ${label}`} className={cn(ACTION, "text-accent")}>
                Tambah cabang
              </Button>
            ) : null}
          </>
        )}
      </div>
    </div>
  );
}

function Branch({ trees, props, collapsed, toggle }: { trees: TreeNode[]; props: StructureTreeProps; collapsed: ReadonlySet<string>; toggle: (id: string) => void }) {
  return (
    <ul className="flex min-w-0 flex-col gap-2">
      {trees.map((t) => {
        const isCollapsed = collapsed.has(t.node.id);
        return (
          <li key={t.node.id} className="min-w-0">
            <NodeCard tree={t} props={props} collapsed={isCollapsed} toggle={() => toggle(t.node.id)} />
            {t.children.length && !isCollapsed ? (
              <div className="ml-3 mt-1 border-l border-border-hairline pl-3">
                <Branch trees={t.children} props={props} collapsed={collapsed} toggle={toggle} />
              </div>
            ) : null}
          </li>
        );
      })}
    </ul>
  );
}

/**
 * Peta struktur bisnis sebagai pohon top-down yang bisa dilipat (tanpa mind map radial, tanpa pustaka kanvas). Simpul
 * usulan bergaris putus-putus dengan kata "usulan", halaman sumber, kutipan, dan **Terima / Ubah / Tolak**; simpul
 * diterima tampil polos. Simpul ditolak keluar dari pohon dan bisa diterima lagi dari daftar di bawahnya.
 */
export function StructureTree(props: StructureTreeProps) {
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  const { roots, rejected } = buildTree(props.nodes);
  const toggle = (id: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  return (
    <div className="flex min-w-0 flex-col gap-3">
      {roots.length ? <Branch trees={roots} props={props} collapsed={collapsed} toggle={toggle} /> : null}
      <RejectedList rejected={rejected} busyId={props.busyId} onAccept={props.onAccept} />
    </div>
  );
}

/** Daftar "Ditolak (n)": simpul yang keluar dari peta dan bisa diterima lagi. Dipakai pohon dan peta pikiran. */
export function RejectedList({ rejected, busyId, onAccept }: { rejected: readonly StructureNode[]; busyId?: string | null; onAccept: (node: StructureNode) => void | Promise<unknown> }) {
  if (!rejected.length) return null;
  return (
    <details className="rounded-md border border-border-hairline px-3">
      <summary className="flex min-h-11 cursor-pointer items-center text-label font-semibold text-text-2">{`Ditolak (${rejected.length})`}</summary>
      <ul className="flex flex-col pb-2">
        {rejected.map((n) => (
          <li key={n.id} data-node={n.id} data-state="rejected" className="flex items-center justify-between gap-2 border-b border-border-hairline last:border-b-0">
            <span className="min-w-0 break-words py-2 text-label text-text-2">{displayText(n.label)}</span>
            <Button variant="ghost" loading={busyId === n.id} onClick={() => onAccept(n)} aria-label={`Terima ${displayText(n.label)}`} className={cn(ACTION, "text-accent")}>
              Terima
            </Button>
          </li>
        ))}
      </ul>
    </details>
  );
}
