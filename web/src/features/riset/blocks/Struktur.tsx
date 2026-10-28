import { useCallback, useMemo, useState } from "react";
import { Button } from "../../../components/Button";
import { ApiError } from "../../../lib/api";
import { importStructureDraft, updateStructureNode, useFinancials, usePack, useStructure } from "../../research/api";
import { cellValue, latestKpiCell, latestXbrlCell, PackValue } from "../../research/components/MetricRow";
import { formatMetric, metricScale } from "../../research/components/metricFormat";
import { useSheetTrigger } from "../../research/components/useSheetTrigger";
import { periodLabel } from "../../research/format";
import type { KpiPackCell, PackMetric, PackResponse, StructureNode, StructureResponse } from "../../research/types";
import { KpiEntrySheet } from "../KpiEntrySheet";
import { unitCaption } from "../lastQuarter";
import { NodeForm, type NodeFormTarget } from "../NodeForm";
import { StructureMindMap } from "../StructureMindMap";
import { StructureTree } from "../StructureTree";
import { proposedCount } from "../structureData";
import { Block, Empty, LoadError, Loading } from "../ui";
import { BusinessNote } from "./BusinessNote";

interface FillTarget {
  metric: PackMetric;
  cell: KpiPackCell;
}

type Notice = { tone: "info" | "error"; text: string; lines?: string[] };

/** Pesan Muat draf dari galat: 404 (belum ada berkas), 422 (berkas ditolak utuh, per field), atau lainnya. */
export function draftNotice(err: unknown, ticker: string, file: string): Notice {
  if (err instanceof ApiError && err.kind === "not_found") {
    return { tone: "error", text: `Belum ada draf untuk ${ticker}. Jalankan /draft-structure ${ticker} <berkas laporan tahunan> di Claude Code; ${file} akan muncul, lalu tekan Muat draf lagi.` };
  }
  if (err instanceof ApiError && err.kind === "validation") {
    const lines = Object.entries(err.fields).map(([field, message]) => `${field}: ${message}`);
    return { tone: "error", text: `Draf ${ticker} ditolak utuh, tidak ada yang dimuat. Perbaiki ${file} lalu muat lagi.`, lines: lines.slice(0, 6) };
  }
  return { tone: "error", text: err instanceof ApiError ? err.message : "Draf belum bisa dimuat" };
}

function MetricChip({ node, pack, packFailed, currency, onFill }: { node: StructureNode; pack: PackResponse | undefined; packFailed: boolean; currency: string | null; onFill: (t: FillTarget, trigger: HTMLElement) => void }) {
  if (!node.metric) return null;
  const name = node.metric_label ?? node.metric;
  const metric = pack?.dimensions.flatMap((d) => d.metrics).find((m) => m.id === node.metric);
  if (!pack || !metric) return <span className="text-caption text-text-3">{packFailed ? `${name}: angka belum bisa dimuat` : pack ? `${name}: belum ada di paket` : `${name}: memuat angka…`}</span>;
  if (metric.layer === "kpi") {
    const cell = latestKpiCell(metric);
    const period = cell ? periodLabel(cell.label) : "terbaru";
    if (!cell) return <span className="text-caption text-text-3">{`${metric.label}: belum ada kuartal`}</span>;
    const text = cell.state === "entered" && typeof cell.value === "number" ? formatMetric(cell.value, metric.unit) : null;
    return (
      <span className="inline-flex flex-wrap items-center gap-x-2 text-caption text-text-2" data-metric={metric.id}>
        <span>{`${metric.label} ${period}`}</span>
        <button
          type="button"
          aria-haspopup="dialog"
          aria-label={text ? `${metric.label} ${period}: ${text}, ubah KPI` : `Isi ${metric.label} ${period}`}
          onClick={(e) => onFill({ metric, cell }, e.currentTarget)}
          className="num inline-flex min-h-11 min-w-11 items-center rounded-sm px-1 text-body text-text decoration-dotted underline-offset-2 hover:underline"
        >
          {text ?? (cell.state === "unknown" ? "Tidak diketahui" : "Isi")}
        </button>
        <span className="text-table-head">{unitCaption(metric.unit, currency, null)}</span>
      </span>
    );
  }
  const cell = latestXbrlCell(metric);
  const scale = metricScale(metric.unit, [metric.latest?.value], currency);
  return (
    <span className="inline-flex flex-wrap items-center gap-x-2 text-caption text-text-2" data-metric={metric.id}>
      <span>{`${metric.label} ${cell ? periodLabel(cell.label) : "terbaru"}`}</span>
      <PackValue metric={metric} cell={cell} scale={scale} />
      <span className="text-table-head">{unitCaption(metric.unit, currency, scale)}</span>
    </span>
  );
}

/**
 * Nilai terakhir metrik yang ditautkan simpul sebagai satu baris teks untuk kartu peta (tanpa tombol: kartu sendiri
 * sudah tombol). Angka KPI yang belum diisi atau metrik tanpa sel menyebut celahnya; paket yang belum dimuat hanya nama.
 */
export function metricLine(node: StructureNode, pack: PackResponse | undefined, currency: string | null): string | null {
  if (!node.metric) return null;
  const name = node.metric_label ?? node.metric;
  const metric = pack?.dimensions.flatMap((d) => d.metrics).find((m) => m.id === node.metric);
  if (!metric) return name;
  const cell = metric.layer === "kpi" ? latestKpiCell(metric) : latestXbrlCell(metric);
  const value = cell ? cellValue(cell) : null;
  if (value === null) return `${metric.label}: belum ada angka`;
  const scale = metricScale(metric.unit, [value], currency);
  return `${metric.label} ${formatMetric(value, metric.unit, scale)} ${unitCaption(metric.unit, currency, scale)}`;
}

function Toolbar({ structure, onAdd, onImport, importing }: { structure: StructureResponse; onAdd: (t: HTMLElement) => void; onImport: () => void; importing: boolean }) {
  const pending = proposedCount(structure.nodes);
  return (
    <div className="flex flex-wrap items-center gap-x-1">
      <Button variant="ghost" onClick={(e) => onAdd(e.currentTarget)} className="-ml-3">
        Tambah cabang
      </Button>
      <Button variant="ghost" onClick={onImport} loading={importing}>
        Muat draf
      </Button>
      {pending ? <span className="text-caption text-text-2">{`${pending} usulan menunggu keputusan`}</span> : null}
    </div>
  );
}

/**
 * Blok 1 "Struktur bisnis": deskripsi bisnis yang disunting di tempat dan peta struktur operasional (G1) sebagai peta
 * pikiran di kanvas yang bisa dicubit (`StructureMindMap`), dengan daftar pohon di balik **Lihat sebagai daftar** untuk
 * pembaca layar dan penyuntingan. Draf ditulis Claude Code di luar aplikasi; **Muat draf** mengimpornya sebagai usulan
 * dan pemilik menerima, mengubah, atau menolak tiap cabang. Peta kosong menjelaskan alurnya dan menawarkan **Muat draf**
 * dan **Tambah cabang**.
 */
export function Struktur({ ticker, name = null, note }: { ticker: string; name?: string | null; note: string | null }) {
  const structure = useStructure(ticker);
  const pack = usePack(ticker);
  const fin = useFinancials(ticker, "standalone", 12);
  const form = useSheetTrigger<NodeFormTarget>();
  const fill = useSheetTrigger<FillTarget>();
  const [busyId, setBusyId] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [asList, setAsList] = useState(false);
  /** Satu baris status sopan yang tetap terpasang saat peta dan daftar berganti: pergantian tampilan dan pengumuman peta lewat sini. Teks sama berturut-turut tetap diumumkan lagi. */
  const [status, setStatus] = useState({ text: "", n: 0 });
  const announce = useCallback((text: string) => setStatus((s) => ({ text, n: s.n + 1 })), []);
  const currency = fin.data?.currency ?? null;
  const data = structure.data;
  const hasChild = useMemo(() => new Set((data?.nodes ?? []).filter((n) => n.state !== "rejected" && n.parent).map((n) => n.parent as string)), [data]);

  const decide = async (node: StructureNode, state: "accepted" | "rejected") => {
    setBusyId(node.id);
    setNotice(null);
    try {
      await updateStructureNode(ticker, node.id, { state });
    } catch (err) {
      setNotice({ tone: "error", text: err instanceof ApiError ? err.message : "Keputusan belum tersimpan" });
    } finally {
      setBusyId(null);
    }
  };

  const loadDraft = async () => {
    setImporting(true);
    setNotice(null);
    try {
      const r = await importStructureDraft(ticker);
      setNotice({ tone: "info", text: `${r.imported} usulan dimuat${r.replaced ? `, ${r.replaced} usulan lama diganti` : ""}. Terima, ubah, atau tolak tiap cabang.` });
    } catch (err) {
      setNotice(draftNotice(err, ticker, data?.draft.file ?? `${ticker}.draft.json`));
    } finally {
      setImporting(false);
    }
  };

  let body;
  if (structure.status === "error" && !data) body = <LoadError message={structure.error?.message ?? "Gagal memuat peta struktur"} onRetry={structure.reload} />;
  else if (!data) body = <Loading label="Memuat peta struktur…" rows={3} height={56} />;
  else if (data.nodes.length === 0)
    body = (
      <Empty
        action={
          <div className="flex flex-wrap items-center gap-x-1">
            <Button variant="sunken" onClick={loadDraft} loading={importing}>
              Muat draf
            </Button>
            <Button variant="ghost" onClick={(e) => form.open({ mode: "add", parent: null }, e.currentTarget)}>
              Tambah cabang
            </Button>
          </div>
        }
      >
        Peta ini menunjukkan dari mana {ticker} mendapat uang: segmen, produk, pembeli, harga acuan, biaya, dan porsi untuk pemilik. Claude Code menyusun draf dari laporan tahunan, lalu Anda menerima, mengubah, atau menolak tiap cabang.
        {data.draft.available ? ` Berkas ${data.draft.file} sudah ada.` : ""}
      </Empty>
    );
  else {
    const treeProps = {
      nodes: data.nodes,
      maxDepth: data.limits.max_depth,
      busyId,
      renderMetric: (n: StructureNode) => <MetricChip node={n} pack={pack.data} packFailed={pack.status === "error"} currency={currency} onFill={(t: FillTarget, trigger: HTMLElement) => fill.open(t, trigger)} />,
      onAccept: (n: StructureNode) => decide(n, "accepted"),
      onReject: (n: StructureNode) => decide(n, "rejected"),
      onEdit: (n: StructureNode, trigger: HTMLElement) => form.open({ mode: "edit", node: n, canDelete: !hasChild.has(n.id) }, trigger),
      onAddChild: (n: StructureNode, trigger: HTMLElement) => form.open({ mode: "add", parent: n }, trigger),
    };
    body = (
      <>
        <div className="flex justify-end">
          <Button
            variant="ghost"
            onClick={() => {
              announce(asList ? "Peta struktur ditampilkan" : "Daftar struktur ditampilkan");
              setAsList((v) => !v);
            }}
            className="-mr-3"
          >
            {asList ? "Lihat sebagai peta" : "Lihat sebagai daftar"}
          </Button>
        </div>
        {asList ? <StructureTree {...treeProps} /> : <StructureMindMap {...treeProps} ticker={ticker} name={name} note={note} metricText={(n) => metricLine(n, pack.data, currency)} announce={announce} error={notice?.tone === "error" ? notice.text : null} />}
        <Toolbar structure={data} onAdd={(trigger) => form.open({ mode: "add", parent: null }, trigger)} onImport={loadDraft} importing={importing} />
      </>
    );
  }

  return (
    <Block id="struktur" title="Struktur bisnis">
      <BusinessNote ticker={ticker} note={note} />
      {body}
      {/* Selalu dirender, juga saat peta atau daftar belum ada: wilayah status yang baru muncul tidak diumumkan. */}
      <div role="status" aria-live="polite" aria-atomic="true" data-struktur-status="" className="sr-only">
        {status.n % 2 === 1 ? `${status.text} ` : status.text}
      </div>
      {notice ? (
        <div role={notice.tone === "error" ? "alert" : "status"} className={notice.tone === "error" ? "text-caption text-status-over" : "text-caption text-text-2"}>
          <p>{notice.text}</p>
          {notice.lines?.length ? (
            <ul className="mt-1 list-disc pl-4">
              {notice.lines.map((l) => (
                <li key={l} className="break-words">
                  {l}
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}
      {form.value && data ? <NodeForm ticker={ticker} structure={data} target={form.value} onClose={form.close} /> : null}
      {fill.value ? <KpiEntrySheet ticker={ticker} metric={fill.value.metric} cell={fill.value.cell} currency={currency} onClose={fill.close} /> : null}
    </Block>
  );
}
