import { useState, type ReactNode } from "react";
import { Button } from "../../components/Button";
import { Sheet } from "../../components/Sheet";
import { cn } from "../../lib/cn";
import type { StructureNode } from "../research/types";
import { pageRef } from "./structureData";
import { displayText } from "./structureText";

export const BADGE = "inline-flex h-5 items-center rounded-sm bg-surface-sunken px-1.5 text-[11px] font-bold uppercase leading-4 tracking-wide text-text-2";
const ACTION = "min-h-11 min-w-11 rounded-sm px-3 text-label font-bold";

export interface MindMapNodeSheetProps {
  node: StructureNode;
  /** Kedalaman simpul di peta (anak pusat = 1); simpul di `maxDepth` tidak bisa diberi cabang. */
  depth: number;
  maxDepth: number;
  color: string;
  /** Nilai terakhir metrik yang ditautkan (chip yang bisa diketuk), atau `null`. */
  metric: ReactNode;
  onAccept: (node: StructureNode) => void | Promise<unknown>;
  onReject: (node: StructureNode) => void | Promise<unknown>;
  onEdit: (node: StructureNode) => void;
  onAddChild: (node: StructureNode) => void;
  onClose: () => void;
}

/**
 * Sheet satu simpul peta: jenis, catatan lengkap, nilai metrik, sumber (dokumen, halaman, kutipan) dan aksi yang sama
 * dengan daftar: Terima / Ubah / Tolak untuk usulan, Ubah / Tambah cabang untuk yang sudah diterima. Keputusan
 * menutup sheet setelah selesai.
 */
export function MindMapNodeSheet({ node, depth, maxDepth, color, metric, onAccept, onReject, onEdit, onAddChild, onClose }: MindMapNodeSheetProps) {
  const [busy, setBusy] = useState(false);
  const proposed = node.state === "proposed";
  const decide = async (run: (n: StructureNode) => void | Promise<unknown>) => {
    setBusy(true);
    try {
      await run(node);
    } finally {
      setBusy(false);
      onClose();
    }
  };
  const footer = (
    <div className="flex flex-wrap items-center">
      {proposed ? (
        <>
          <Button variant="ghost" loading={busy} onClick={() => decide(onAccept)} className={cn(ACTION, "text-accent")}>
            Terima
          </Button>
          <Button variant="ghost" onClick={() => onEdit(node)} className={cn(ACTION, "text-accent")}>
            Ubah
          </Button>
          <Button variant="ghost" tone="danger" loading={busy} onClick={() => decide(onReject)} className={ACTION}>
            Tolak
          </Button>
        </>
      ) : (
        <>
          <Button variant="ghost" onClick={() => onEdit(node)} className={cn(ACTION, "text-accent")}>
            Ubah
          </Button>
          {depth < maxDepth ? (
            <Button variant="ghost" onClick={() => onAddChild(node)} className={cn(ACTION, "text-accent")}>
              Tambah cabang
            </Button>
          ) : null}
        </>
      )}
    </div>
  );
  return (
    <Sheet open title={displayText(node.label)} wrapTitle onClose={onClose} footer={footer}>
      <div className="flex flex-col gap-3 pb-2" data-node-sheet={node.id}>
        <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-caption text-text-2">
          <span aria-hidden="true" className="inline-block h-2.5 w-2.5 rounded-full" style={{ background: color }} />
          <span>{node.kind_label}</span>
          {node.owner ? <span className={BADGE}>PEMILIK</span> : null}
          {proposed ? <span className={BADGE}>USULAN</span> : null}
        </p>
        {node.note ? <p className="break-words text-body text-text">{displayText(node.note)}</p> : <p className="text-caption text-text-3">Belum ada catatan.</p>}
        {metric ? <div className="flex min-w-0 flex-wrap items-center">{metric}</div> : null}
        {node.source ? (
          <p className="break-words text-caption text-text-2">
            <span className="font-semibold">{node.source.document}</span>
            {`, ${pageRef(node)}: “${node.source.snippet}”`}
          </p>
        ) : node.owner ? (
          <p className="text-caption text-text-2">Ditambahkan pemilik, tanpa sumber laporan.</p>
        ) : null}
      </div>
    </Sheet>
  );
}
