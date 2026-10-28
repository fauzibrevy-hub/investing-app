import { Fragment, useMemo } from "react";
import { useFinancials, usePack } from "../../research/api";
import { PackValue, YoyValue, isKpiCell } from "../../research/components/MetricRow";
import { formatMetric, metricScale } from "../../research/components/metricFormat";
import { currencyWord, formatScaled, periodLabel, type Scale } from "../../research/format";
import type { KpiPackCell, PackCell, PackMetric, PackResponse, XbrlPackCell } from "../../research/types";
import { useSheetTrigger } from "../../research/components/useSheetTrigger";
import { KpiEntrySheet } from "../KpiEntrySheet";
import { gapSentence } from "../dataGaps";
import { kpiMoneyUnit, lastQuarterGroups, quarterTableLabel, tableScale, unitCaption, type QuarterGroup, type QuarterRow } from "../lastQuarter";
import { Block, Empty, LoadError, Loading } from "../ui";

const CELL_BUTTON = "num inline-flex min-h-11 min-w-11 items-center justify-end whitespace-nowrap rounded-sm px-1 text-right decoration-dotted underline-offset-2 hover:underline focus-visible:underline";

interface FillTarget {
  metric: PackMetric;
  cell: KpiPackCell;
}

function Absent({ reason }: { reason: string }) {
  return (
    <span title={reason} data-gap="" className="inline-flex min-h-11 items-center whitespace-nowrap text-caption text-text-3">
      <span aria-hidden="true">—</span>
      <span className="sr-only">{reason}</span>
    </span>
  );
}

/** KPI tahunan di kuartal selain Q4: bukan celah, hanya belum waktunya. */
function AnnualOnly({ metric, period }: { metric: PackMetric; period?: string }) {
  return (
    <span title="Metrik tahunan: dilaporkan di kuartal 4" className="inline-flex min-h-11 items-center justify-end text-right text-caption leading-snug text-text-3">
      <span aria-hidden="true">Tahunan · dilaporkan di Q4</span>
      <span className="sr-only">{`${metric.label}${period ? ` ${period}` : ""}: tidak berlaku, metrik tahunan dilaporkan di kuartal 4`}</span>
    </span>
  );
}

interface CellProps {
  metric: PackMetric;
  cell: PackCell | null;
  /** `Q2 26` atau `Q2 25`, untuk nama aksesibel. */
  period: string;
  scale: ReturnType<typeof metricScale>;
  /** KPI uang pada skala tabel; `null` memakai satuan metriknya. */
  amount: ((v: number) => string) | null;
  onFill: (target: FillTarget, trigger: HTMLElement) => void;
}

/** Sel KPI: nilai (ketuk untuk mengubah), Tidak diketahui, atau **Isi**; semuanya membuka sheet isian. */
function KpiValue({ metric, cell, period, onFill, amount }: { metric: PackMetric; cell: KpiPackCell; period: string; onFill: CellProps["onFill"]; amount: CellProps["amount"] }) {
  const open = (e: { currentTarget: HTMLElement }) => onFill({ metric, cell }, e.currentTarget);
  if (cell.state === "not_applicable") return <AnnualOnly metric={metric} period={period} />;
  if (cell.state === "entered" && typeof cell.value === "number") {
    const text = amount ? amount(cell.value) : formatMetric(cell.value, metric.unit);
    return (
      <button type="button" onClick={open} aria-haspopup="dialog" aria-label={`${metric.label} ${period}: ${text}, ubah KPI`} className={CELL_BUTTON}>
        {text}
      </button>
    );
  }
  if (cell.state === "unknown") {
    return (
      <button type="button" onClick={open} aria-haspopup="dialog" aria-label={`${metric.label} ${period}: Tidak diketahui, ubah KPI`} className="inline-flex min-h-11 items-center justify-end rounded-sm px-1 text-right text-caption leading-snug text-text-2 decoration-dotted underline-offset-2 hover:underline">
        Tidak diketahui
      </button>
    );
  }
  return (
    <button type="button" onClick={open} aria-haspopup="dialog" aria-label={`Isi ${metric.label} ${period}`} className="inline-flex min-h-11 min-w-11 items-center justify-center">
      <span className="inline-flex h-8 items-center rounded-sm bg-surface-sunken px-3 text-caption font-bold text-accent">Isi</span>
    </button>
  );
}

function QuarterValue({ metric, cell, period, scale, amount, onFill }: CellProps) {
  if (!cell) return <Absent reason={`${metric.label} ${period}: tidak ada kuartal ini di paket`} />;
  if (isKpiCell(cell)) return <KpiValue metric={metric} cell={cell} period={period} amount={amount} onFill={onFill} />;
  const x: XbrlPackCell = cell;
  if (x.value === null) return <Absent reason={`${metric.label}: ${x.gap ? gapSentence(x.gap, metric, period) : `tidak ada di laporan ${period}`}`} />;
  return <PackValue metric={metric} cell={x} scale={scale} />;
}

function Row({ row, latestLabel, priorLabel, currency, amountScale, onFill }: { row: QuarterRow; latestLabel: string; priorLabel: string; currency: string | null; amountScale: Scale | null; onFill: CellProps["onFill"] }) {
  const { metric } = row;
  const value = (c: PackCell | null) => (c && !isKpiCell(c) ? c.value : c && c.state === "entered" ? (c.value ?? null) : null);
  // Jumlah uang satu skala di seluruh tabel; metrik lain (persen, kali, satuan fisik) tidak memakai skala.
  const scale = metric.unit === "ccy" && amountScale ? amountScale : metricScale(metric.unit, [value(row.latest), value(row.prior)], currency);
  // KPI uang bersatuan tertulis (`USD juta`) memakai skala tabel yang sama dengan baris uang lain (`USD rb`), bila mata uangnya sama.
  const kpiMoney = amountScale && currency && metric.layer === "kpi" ? kpiMoneyUnit(metric.unit) : null;
  const money = kpiMoney && currencyWord(kpiMoney.currency) === currencyWord(currency ?? "") ? kpiMoney : null;
  const amount = money && amountScale ? (v: number) => formatScaled(v * money.multiplier, amountScale) : null;
  const unit = money ? unitCaption("ccy", money.currency, amountScale) : unitCaption(metric.unit, currency, scale);
  // Metrik tahunan di kuartal selain Q4: kedua kolom kuartal berisi kata yang sama, jadi satu sel selebar dua kolom.
  const annualOnly = [row.latest, row.prior].every((c) => c && isKpiCell(c) && c.state === "not_applicable");
  return (
    <tr data-metric={metric.id} className="border-b border-border-hairline">
      <th scope="row" className="min-h-11 py-1 pl-0 pr-2 text-left align-middle">
        <span className="flex min-w-0 flex-col" title={`${metric.label}. ${metric.why}`}>
          <span className="break-words text-label font-semibold leading-snug text-text">{quarterTableLabel(metric)}</span>
          <span className="text-caption leading-snug text-text-2">{unit}</span>
        </span>
      </th>
      {annualOnly ? (
        <td colSpan={2} className="px-1 text-right align-middle">
          <AnnualOnly metric={metric} />
        </td>
      ) : (
        <>
          <td className="num px-1 text-right align-middle">
            <QuarterValue metric={metric} cell={row.latest} period={latestLabel} scale={scale} amount={amount} onFill={onFill} />
          </td>
          <td className="num px-1 text-right align-middle">
            <QuarterValue metric={metric} cell={row.prior} period={priorLabel} scale={scale} amount={amount} onFill={onFill} />
          </td>
        </>
      )}
      <td className="num py-0 pl-1 pr-0 text-right align-middle">
        {/* Mesin menghitung YoY untuk kuartal terakhir metrik itu sendiri; di tabel ini hanya bila itu kuartal kolom terbaru. */}
        {metric.yoy && metric.latest && periodLabel(metric.latest.label) === latestLabel ? (
          <YoyValue metric={metric} scale={scale} />
        ) : (
          <Absent reason={`YoY ${metric.label}: ${metric.yoy ? `belum ada nilai ${latestLabel}` : (metric.yoy_reason ?? "belum ada pembanding tahun lalu")}`} />
        )}
      </td>
    </tr>
  );
}

/** Tabel muat di lebar layar (357 px di iPhone): tata letak tetap, tanpa kotak gulir. */
function QuarterTable({ groups, latest, currency, onFill }: { groups: QuarterGroup[]; latest: PackResponse["latest"]; currency: string | null; onFill: CellProps["onFill"] }) {
  const latestLabel = periodLabel(latest.label);
  const priorLabel = periodLabel(`${latest.fy - 1}Q${latest.q}`);
  const amountScale = tableScale(groups, currency);
  return (
    <div className="w-full min-w-0">
      <table className="w-full table-fixed border-collapse text-table">
        <caption className="sr-only-table">{`Kuartal terakhir ${latestLabel} dibanding ${priorLabel}, per metrik paket`}</caption>
        <colgroup>
          <col className="w-[40%]" />
          <col className="w-[19%]" />
          <col className="w-[19%]" />
          <col className="w-[22%]" />
        </colgroup>
        <thead>
          <tr className="border-b border-border-hairline text-table-head uppercase text-text-2">
            <th scope="col" className="h-10 pl-0 pr-2 text-left">
              Metrik
            </th>
            <th scope="col" className="px-1 text-right">{latestLabel}</th>
            <th scope="col" className="px-1 text-right">{priorLabel}</th>
            <th scope="col" className="py-0 pl-1 pr-0 text-right" title="Persen untuk nilai, poin persen untuk metrik berbentuk persen, selisih kali untuk rasio">
              YoY
            </th>
          </tr>
        </thead>
        {groups.map((g) => (
          <Fragment key={g.id}>
            <tbody data-group={g.id}>
              {/* Judul kelompok baris di dalam <tbody>: `rowgroup`, bukan `colgroup` (itu terbaca sebagai kepala kolom "LABA RUGI"). */}
              <tr>
                <th scope="rowgroup" colSpan={4} className="h-9 pl-0 pt-3 text-left text-table-head uppercase tracking-wide text-text-2">
                  {g.label}
                </th>
              </tr>
              {g.rows.map((r) => (
                <Row key={r.metric.id} row={r} latestLabel={latestLabel} priorLabel={priorLabel} currency={currency} amountScale={amountScale} onFill={onFill} />
              ))}
            </tbody>
          </Fragment>
        ))}
      </table>
    </div>
  );
}

/**
 * Blok 3 "Kuartal terakhir" (ui-slicing.md §1): metrik paket dikelompokkan menurut dimensi, kuartal standalone terbaru
 * dibanding kuartal yang sama tahun lalu dan YoY. KPI yang belum diisi menampilkan **Isi** yang membuka isian dengan
 * "Tidak diketahui" dan unggahan dokumen. Kotak tabel menggulir sendiri; kolom metrik tetap di kiri.
 */
export function KuartalTerakhir({ ticker }: { ticker: string }) {
  const pack = usePack(ticker);
  const fin = useFinancials(ticker, "standalone", 12);
  const fill = useSheetTrigger<FillTarget>();
  const taxonomy = fin.data?.taxonomy ?? null;
  const groups = useMemo(() => (pack.data ? lastQuarterGroups(pack.data, taxonomy) : []), [pack.data, taxonomy]);
  const currency = fin.data?.currency ?? null;

  let body;
  if (pack.status === "error") body = <LoadError message={pack.error?.message ?? "Gagal memuat paket"} onRetry={pack.reload} />;
  else if (pack.status === "loading" || !pack.data) body = <Loading label="Memuat kuartal terakhir…" rows={5} />;
  else if (groups.length === 0) body = <Empty>Belum ada metrik untuk emiten ini. Tentukan paket industri di bagian atas halaman.</Empty>;
  else
    body = (
      <>
        {pack.data.latest.from_filing ? null : <p className="text-caption text-text-2">{`Laporan ${periodLabel(pack.data.latest.label)} belum terbit; kolom ini kosong sampai laporan masuk.`}</p>}
        <QuarterTable groups={groups} latest={pack.data.latest} currency={currency} onFill={(t, trigger) => fill.open(t, trigger)} />
      </>
    );

  return (
    <Block id="kuartal" title={pack.data ? `Kuartal terakhir · ${periodLabel(pack.data.latest.label)}` : "Kuartal terakhir"}>
      {body}
      {fill.value ? <KpiEntrySheet ticker={ticker} metric={fill.value.metric} cell={fill.value.cell} currency={currency} onClose={fill.close} /> : null}
    </Block>
  );
}
