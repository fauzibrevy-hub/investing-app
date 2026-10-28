import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { ChevronDown, ChevronRight, X } from "lucide-react";
import { cn } from "../../../lib/cn";
import { isProfitLine, signChangePhrase, yoyCellText } from "../change";
import { formatExact, periodLabel, type Scale } from "../format";
import type { KpiPackCell, PackCell, PackGap, PackMetric, XbrlPackCell } from "../types";
import { useFloatingPosition } from "./floating";
import { KpiCell } from "./KpiCell";
import { gapReasonText } from "./gapReason";
import { formatMetric, metricScale, metricUnitHeader } from "./metricFormat";
import { PercentileBand } from "./PercentileBand";
import { ProvenanceBadge } from "./ProvenanceBadge";
import { Sparkline } from "./Sparkline";

/** Lebar panel asal angka (px); `w-[min(320px,…)]` di panelnya menjaganya di dalam layar sempit. */
const PANEL_WIDTH = 320;

export const isKpiCell = (c: PackCell): c is KpiPackCell => "state" in c;

/** Nilai sel untuk sparkline: angka xbrl, atau KPI yang diisi; selebihnya celah. */
export const cellValue = (c: PackCell): number | null => (isKpiCell(c) ? (c.state === "entered" && typeof c.value === "number" ? c.value : null) : c.value);

/** Sel KPI terakhir yang berlaku (KPI tahunan hanya di Q4). */
export const latestKpiCell = (m: PackMetric): KpiPackCell | null => {
  const cells = m.cells.filter(isKpiCell).filter((c) => c.state !== "not_applicable");
  return cells[cells.length - 1] ?? null;
};

/** Sel xbrl dari nilai `latest` (atau sel terakhir bila belum ada nilai: celahnya yang ditampilkan). */
export const latestXbrlCell = (m: PackMetric): XbrlPackCell | null => {
  const cells = m.cells.filter((c): c is XbrlPackCell => !isKpiCell(c));
  return (m.latest ? cells.find((c) => c.label === m.latest!.label) : undefined) ?? cells[cells.length - 1] ?? null;
};

/** Sel tahun sebelumnya untuk YoY. */
const yearAgo = (m: PackMetric, fy: number, q: number): PackCell | undefined => m.cells.find((c) => c.fy === fy - 1 && c.q === q);

export interface ProvenancePopoverProps {
  /** Angka yang tampil (sudah diformat, tanpa satuan). */
  text: string;
  /** "Asal angka <label> <periode>". */
  label: string;
  title: string;
  children: ReactNode;
  className?: string;
}

/**
 * Asal angka untuk nilai paket yang tidak punya satu fakta (rumus atas beberapa fakta, YoY): tombol angka
 * (`aria-haspopup="dialog"`, garis bawah titik seperti `FactChip`) yang membuka panel non-modal berisi rumus dan
 * masukannya. Esc atau klik di luar menutup dan fokus kembali ke angka.
 */
export function ProvenancePopover({ text, label, title, children, className }: ProvenancePopoverProps) {
  const panelId = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const floating = useFloatingPosition(trigger, open, "end", PANEL_WIDTH);

  const close = () => {
    setOpen(false);
    trigger.current?.focus();
  };

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      e.preventDefault();
      close();
    };
    const onDown = (e: MouseEvent) => {
      const t = e.target as Node;
      if (!panel.current?.contains(t) && !trigger.current?.contains(t)) setOpen(false);
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onDown);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onDown);
    };
  }, [open]);

  return (
    <span className={cn("relative inline-flex justify-end", className)}>
      <button
        ref={trigger}
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        aria-label={`${label}: ${text}`}
        onClick={() => setOpen((o) => !o)}
        className="num min-h-11 min-w-11 rounded-sm px-1 text-right decoration-dotted underline-offset-2 hover:underline focus-visible:underline"
      >
        <span aria-hidden="true">{text}</span>
      </button>
      {open ? (
        <div ref={panel} id={panelId} role="dialog" aria-label={title} style={floating} className="fixed z-30 w-[min(320px,calc(100vw-8px))] overflow-y-auto overscroll-contain rounded-md border border-border-hairline bg-bg text-left font-medium shadow-popover">
          <div className="sticky top-0 flex items-start justify-between gap-2 border-b border-border-hairline bg-bg py-1 pl-3">
            <p className="min-w-0 py-2 text-label font-bold text-text [overflow-wrap:anywhere]">{title}</p>
            <button type="button" aria-label="Tutup" onClick={close} className="inline-flex min-h-11 min-w-11 shrink-0 items-center justify-center rounded-sm text-text-2">
              <X aria-hidden="true" className="size-4" />
            </button>
          </div>
          <div className="flex flex-col gap-2 p-3">{children}</div>
        </div>
      ) : null}
    </span>
  );
}

/** Celah paket: alasannya terlihat; `action` menambah **Isi** ke bagian Paket dossier (nilai milik emiten yang sedang dibuka). */
export function GapMark({ label, period, gap, metric, action = false, align = "end" }: { label: string; period: string; gap: PackGap | { text: string } | null; /** Metrik pemilik celah: namanya, bukan id rumus, yang tampil di alasan. */ metric?: { id: string; label: string }; action?: boolean; align?: "end" | "start" }) {
  const reason = gap ? (metric ? gapReasonText(gap.text, metric) : gap.text) : "Tidak ada di laporan";
  return (
    <span title={reason} data-gap="" className={cn("inline-flex min-w-0 max-w-full flex-col text-text-3", align === "end" ? "items-end text-right" : "items-start text-left")}>
      <span className="min-w-0 max-w-[168px] text-caption [overflow-wrap:anywhere]">{reason}</span>
      {action ? (
        <Link to={{ search: "?section=paket" }} aria-label={`Isi ${label} ${period}`} className="inline-flex min-h-11 min-w-11 items-center justify-end text-caption font-bold text-accent">
          Isi
        </Link>
      ) : null}
    </span>
  );
}

function Inputs({ cell }: { cell: XbrlPackCell }) {
  if (!cell.inputs.length) return <p className="text-caption text-text-2">Tanpa masukan tercatat</p>;
  return (
    <ul className="flex flex-col gap-1">
      {cell.inputs.map((i, n) => (
        <li key={`${i.ident}-${i.q}-${n}`} className="flex items-baseline justify-between gap-2 text-caption">
          <span className="min-w-0 text-text-2 [overflow-wrap:anywhere]">
            <span className="font-bold text-text">{i.ident}</span>
            {` · ${i.source} · ${periodLabel(i.q)} · ${i.view}`}
          </span>
          <span className="num shrink-0 text-text">{formatExact(i.value)}</span>
        </li>
      ))}
    </ul>
  );
}

/** Fakta dari laporan PDF yang menopang sel: badge PDF dengan halaman, lalu elemen dan label tercetak. */
function PdfSources({ cell }: { cell: XbrlPackCell }) {
  if (!cell.sources?.length) return null;
  return (
    <div className="flex flex-col gap-1">
      <p className="text-caption text-text-2">Dibaca dari laporan asli (PDF), bukan XBRL</p>
      <ul className="flex flex-col gap-1">
        {cell.sources.map((p) => (
          <li key={`${p.filing_id}-${p.element}-${p.page}-${p.label}`} className="flex items-baseline gap-2 text-caption">
            <ProvenanceBadge kind="pdf" text={`PDF · hal. ${p.page}`} />
            <span className="min-w-0 text-text-2 [overflow-wrap:anywhere]">{`${p.filing_id} · ${p.element} · ${p.label}`}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export interface PackValueProps {
  metric: PackMetric;
  cell: XbrlPackCell | null;
  scale: Scale | null;
  /** Rata kanan di tabel (bawaan) atau kiri di kartu; hanya berlaku untuk celah. */
  align?: "end" | "start";
}

/** Nilai xbrl paket: angka + panel asal (rumus, masukan per fakta, tautan ke Keuangan); celah dengan alasannya. */
export function PackValue({ metric, cell, scale, align }: PackValueProps) {
  const period = cell ? periodLabel(cell.label) : "terbaru";
  if (!cell || cell.value === null) return <GapMark label={metric.label} period={period} gap={cell?.gap ?? null} metric={metric} action align={align} />;
  const text = formatMetric(cell.value, metric.unit, scale);
  return (
    <ProvenancePopover text={text} label={`Asal angka ${metric.label} ${period}`} title={`${metric.label} ${period}`}>
      {metric.formula ? <p className="break-words text-caption text-text-2">{`Rumus: ${metric.formula}`}</p> : null}
      <Inputs cell={cell} />
      <PdfSources cell={cell} />
      {cell.flags.includes("out_of_range") ? <p className="text-caption text-text-2">Di luar rentang yang dinyatakan paket</p> : null}
      <Link to={{ search: "?section=keuangan" }} className="inline-flex min-h-11 items-center text-label font-bold text-accent">
        Lihat fakta di Keuangan
      </Link>
    </ProvenancePopover>
  );
}

/** YoY dengan asal: dua nilai yang dibandingkan dan rumusnya. */
export function YoyValue({ metric, scale }: { metric: PackMetric; scale: Scale | null }) {
  const latest = metric.latest;
  if (!metric.yoy || !latest) return <GapMark label={`YoY ${metric.label}`} period={latest ? periodLabel(latest.label) : "terbaru"} gap={{ text: "Belum ada pembanding tahun lalu" }} />;
  const prev = yearAgo(metric, latest.fy, latest.q);
  const prevValue = prev ? cellValue(prev) : null;
  const period = periodLabel(latest.label);
  const prevPeriod = prev ? periodLabel(prev.label) : `${latest.fy - 1}Q${latest.q}`;
  const profit = isProfitLine(metric.id);
  const formula =
    metric.yoy.kind === "pp"
      ? `${period} − ${prevPeriod} (poin persen)`
      : metric.yoy.kind === "x"
        ? `${period} − ${prevPeriod} (selisih kali)`
        : metric.yoy.kind === "sign_change"
        ? `Tanda berubah ${signChangePhrase(metric.yoy.from, metric.yoy.to, profit)}; tanpa persentase`
        : `(${period} − ${prevPeriod}) ÷ |${prevPeriod}| × 100`;
  return (
    <ProvenancePopover text={yoyCellText(metric.yoy, profit)} label={`Asal angka YoY ${metric.label} ${period}`} title={`YoY ${metric.label} ${period}`}>
      <p className="text-caption text-text-2">{`Rumus: ${formula}`}</p>
      <dl className="grid grid-cols-[1fr_auto] gap-x-3 gap-y-1 text-caption">
        <dt className="text-text-2">{period}</dt>
        <dd className="num text-right text-text">{formatMetric(latest.value, metric.unit, scale)}</dd>
        <dt className="text-text-2">{prevPeriod}</dt>
        <dd className="num text-right text-text">{prevValue !== null ? formatMetric(prevValue, metric.unit, scale) : "tidak ada"}</dd>
      </dl>
    </ProvenancePopover>
  );
}

/** Nilai pembanding: tautan ke paket emiten itu (asal angkanya), atau celah dengan alasan backend. */
export function PeerValue({ metric, peer, scale }: { metric: PackMetric; peer: PackMetric["peers"][number]; scale: Scale | null }) {
  const period = peer.fy && peer.q ? periodLabel(`${peer.fy}Q${peer.q}`) : "terbaru";
  if (peer.value === null) return <GapMark label={`${metric.label} ${peer.ticker}`} period={period} gap={peer.gap} metric={metric} />;
  const text = formatMetric(peer.value, metric.unit, scale);
  return (
    <Link
      to={`/research/${peer.ticker}?section=paket`}
      aria-label={`Asal angka ${metric.label} ${peer.ticker} ${period}: ${text}, buka paket ${peer.ticker}`}
      className="num inline-flex min-h-11 min-w-11 items-center justify-end rounded-sm px-1 decoration-dotted underline-offset-2 hover:underline"
    >
      {text}
    </Link>
  );
}

export interface MetricRowProps {
  ticker: string;
  metric: PackMetric;
  /** Kolom pembanding (sampai tiga), urutan sama untuk seluruh kartu. */
  peers: readonly string[];
  /** Mata uang pelaporan untuk metrik `ccy`. */
  currency: string | null;
  /** Jumlah kolom tabel (untuk baris KPI kuartal lain). */
  columns: number;
}

const KPI_HISTORY = 4;

/**
 * Baris metrik paket (design.md §6, tinggi 44): label 600 dengan satuan · `Sparkline` 96 × 24 · nilai terbaru · YoY ·
 * `PercentileBand` 120 × 12 · nilai pembanding. Baris `kpi` memakai `KpiCell` dan bisa membuka empat kuartal terakhir
 * yang diharapkan agar celah lama juga bisa diisi. Satuan hanya di header baris.
 */
export function MetricRow({ ticker, metric, peers, currency, columns }: MetricRowProps) {
  const [history, setHistory] = useState(false);
  const historyId = useId();
  const kpi = metric.layer === "kpi";
  const peerValues = peers.map((t) => metric.peers.find((p) => p.ticker === t));
  const scale = metricScale(metric.unit, [metric.latest?.value, ...peerValues.map((p) => p?.value)], currency);
  const unit = metricUnitHeader(metric.unit, currency, scale);
  const latestKpi = kpi ? latestKpiCell(metric) : null;
  const kpiHistory = kpi ? metric.cells.filter(isKpiCell).filter((c) => c.state !== "not_applicable").slice(-KPI_HISTORY) : [];
  const Chevron = history ? ChevronDown : ChevronRight;

  return (
    <>
      <tr className="h-11 border-b border-border-hairline" data-metric={metric.id} data-layer={metric.layer}>
        <th scope="row" className="sticky left-0 z-[1] min-w-[220px] border-r border-border-hairline bg-bg py-1 pl-4 pr-3 text-left align-middle">
          <span className="flex min-w-0 flex-col">
            <span className="text-body font-semibold text-text" title={metric.why}>
              {metric.label}
            </span>
            <span className="text-table-head uppercase text-text-2">{unit}</span>
          </span>
        </th>
        <td className="px-3 align-middle">
          {/* Tombol riwayat di sel sendiri, bukan di header baris: header baris dibacakan di setiap sel. */}
          <span className="flex items-center gap-1">
            {kpi && kpiHistory.length > 1 ? (
              <button
                type="button"
                aria-expanded={history}
                aria-controls={historyId}
                aria-label={`${KPI_HISTORY} kuartal terakhir ${metric.label}`}
                onClick={() => setHistory((h) => !h)}
                className="-ml-2 inline-flex h-11 w-11 shrink-0 items-center justify-center text-text-2"
              >
                <Chevron aria-hidden="true" size={16} strokeWidth={1.75} />
              </button>
            ) : null}
            <Sparkline values={metric.cells.map(cellValue)} label={`Tren 12 kuartal ${metric.label}`} />
          </span>
        </td>
        <td className="num whitespace-nowrap px-3 text-right align-middle">
          {kpi ? latestKpi ? <KpiCell ticker={ticker} metric={metric.id} label={metric.label} unit={metric.unit} cell={latestKpi} /> : <GapMark label={metric.label} period="terbaru" gap={null} action /> : <PackValue metric={metric} cell={latestXbrlCell(metric)} scale={scale} />}
        </td>
        <td className="num whitespace-nowrap px-3 text-right align-middle">
          <YoyValue metric={metric} scale={scale} />
        </td>
        <td className="whitespace-nowrap px-3 align-middle">
          <PercentileBand percentile={metric.percentile?.value ?? null} caption={metric.percentile?.caption} label={`Persentil ${metric.label} dalam riwayat sendiri`} />
        </td>
        {peers.map((t, i) => (
          <td key={t} className="num whitespace-nowrap px-3 text-right align-middle last:pr-4">
            {peerValues[i] ? <PeerValue metric={metric} peer={peerValues[i]!} scale={scale} /> : <GapMark label={`${metric.label} ${t}`} period="terbaru" gap={{ text: "Tidak ada nilai pembanding" }} />}
          </td>
        ))}
      </tr>
      {kpi && history ? (
        <tr id={historyId} className="border-b border-border-hairline bg-surface-sunken">
          <td colSpan={columns} className="px-4 py-2">
            <ul aria-label={`${metric.label}, ${KPI_HISTORY} kuartal terakhir`} className="flex flex-wrap items-center gap-x-6 gap-y-1">
              {kpiHistory.map((c) => (
                <li key={c.label} className="flex items-center gap-2">
                  <span className="text-table-head uppercase text-text-2">{periodLabel(c.label)}</span>
                  <KpiCell ticker={ticker} metric={metric.id} label={metric.label} unit={metric.unit} cell={c} align="start" />
                </li>
              ))}
            </ul>
          </td>
        </tr>
      ) : null}
    </>
  );
}
