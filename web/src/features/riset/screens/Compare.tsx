import { useMemo } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { Button } from "../../../components/Button";
import { cn } from "../../../lib/cn";
import { BackHeader } from "../../../shell/BackHeader";
import { useCompare, usePacks, useUniverse } from "../../research/api";
import { FactDrawerProvider } from "../../research/components/FactDrawer";
import { FactChip } from "../../research/components/FactChip";
import { gapReasonText } from "../../research/components/gapReason";
import { ScreenFooter } from "../../research/desk/DeskFooter";
import { chooseScale, formatMultiple, formatPct, formatScaled, formatValue, periodLabel, unitHeader, type Scale } from "../../research/format";
import type { CellFlag, CompareCell, CompareResponse, CompareRow, PackDefinition, Role } from "../../research/types";
import { MIXED_HEADER, columnScales, columnUnit, moneyRowHeader } from "../compareScale";
import { isBankPack, isNonBankCore, kpiMoneyUnit, quarterTableLabel } from "../lastQuarter";
import { Empty, FIELD, LoadError, Loading, PAGE } from "../ui";

const MAX_TICKERS = 12;
const ROLE_WORDS: Record<Role, string> = { held: "dipegang", manual: "manual", peer: "pembanding" };
const KNOWN_FLAGS: readonly CellFlag[] = ["derived_fy", "from_comparative", "restated", "gap", "instant", "reconcile_failed", "basis_break", "sign_conflict", "per_share_approx"];

/** Baris inti yang tidak dibawa laporan bank disembunyikan dengan daftar yang sama seperti halaman emiten. */
export function visibleRows(data: CompareResponse): CompareRow[] {
  const bank = isBankPack(data.pack?.id);
  return data.rows.filter((r) => !isNonBankCore(r.metric.id, r.group === "core", bank));
}

/** Uang memakai satu skala per kolom emiten (N8); satuan lain satu format per baris. `format` menerima kode emiten selnya. */
function rowFormat(row: CompareRow, scales: ReturnType<typeof columnScales>): { header: string; format: (v: number, ticker: string) => string } {
  const { unit } = row.metric;
  // Metrik uang (`ccy`, atau KPI bersatuan `USD juta` di kolom bermata uang itu) memakai skala kolomnya.
  const kpiMoney = kpiMoneyUnit(unit);
  const kpiFits = kpiMoney !== null && [...scales.values()].every((s) => s.currency === kpiMoney.currency);
  if (unit === "ccy" || kpiFits) {
    const multiplier = kpiFits ? kpiMoney.multiplier : 1;
    return { header: moneyRowHeader(scales), format: (v, ticker) => formatScaled(v * multiplier, (scales.get(ticker)?.scale ?? chooseScale([v * multiplier], "IDR")) as Scale) };
  }
  const header = unitHeader(unit);
  if (unit === "%" || unit === "pp") return { header, format: formatPct };
  if (unit === "x" || unit === "×") return { header, format: formatMultiple };
  const whole = row.cells.every((c) => c.value === null || Number.isInteger(c.value) || Math.abs(c.value) >= 100);
  return { header, format: (v) => formatValue(v, whole ? 0 : 1) };
}

/** Alasan sel kosong dan aksinya; dipakai tooltip, teks pembaca layar, dan daftar sel kosong di bawah tabel. */
export function gapOf(cell: CompareCell, row: CompareRow): { why: string; action: string } {
  if (cell.state === "unknown") return { why: "Tidak diketahui", action: "Ubah" };
  if (cell.reason === "not_defined") return { why: `Paket ${cell.ticker} tidak mendefinisikan ${row.metric.label}`, action: "Tentukan paket" };
  if (cell.reason === "not_entered") return { why: "Belum diisi", action: "Isi" };
  return { why: cell.reason ? gapReasonText(cell.reason, row.metric) : "Tidak ada di laporan", action: "Isi" };
}

function Cell({ cell, row, format }: { cell: CompareCell; row: CompareRow; format: (v: number) => string }) {
  const label = `${row.metric.label} ${cell.ticker}`;
  const period = cell.period ? periodLabel(cell.period) : "";
  if (cell.state !== "value" || cell.value === null) {
    const { why } = gapOf(cell, row);
    return (
      <span title={why} data-gap="" className="inline-flex min-h-11 items-center justify-end text-text-3">
        <span aria-hidden="true">—</span>
        <span className="sr-only">{`${label}: ${why}`}</span>
      </span>
    );
  }
  const flags = cell.flags.filter((f): f is CellFlag => (KNOWN_FLAGS as readonly string[]).includes(f));
  if (cell.fact) return <FactChip value={cell.value} text={format(cell.value)} target={cell.fact} label={label} period={period} flags={flags} badges={row.metric.layer === "kpi" ? ["manual"] : []} />;
  return (
    <Link
      to={`/perusahaan/${cell.ticker}?section=kuartal&from=alat`}
      aria-label={`Asal angka ${label} ${period}: ${format(cell.value)}, rincian di halaman ${cell.ticker}`}
      className="num inline-flex min-h-11 min-w-11 items-center justify-end px-1 text-right text-text underline decoration-dotted underline-offset-4 hover:decoration-solid"
    >
      {format(cell.value)}
    </Link>
  );
}

const STICKY = "sticky left-0 z-[1] bg-bg shadow-[inset_-1px_0_0_var(--border-hairline)]";
/** Kolom metrik mengikuti isi (label membungkus), tidak pernah dipotong; selebar-lebarnya separuh layar kecil. */
const METRIC_COL = "w-auto min-w-[9rem] max-w-[12rem] pr-2";

/** Tabel: baris = metrik (inti dulu, lalu dimensi paket), kolom = emiten; kolom metrik tetap, kotaknya menggulir sendiri. */
function CompareTable({ data }: { data: CompareResponse }) {
  const groups = useMemo(() => {
    const out: Array<{ id: string; label: string; rows: CompareRow[] }> = [];
    for (const r of data.rows) {
      const last = out[out.length - 1];
      if (last && last.id === r.group) last.rows.push(r);
      else out.push({ id: r.group, label: r.group_label, rows: [r] });
    }
    return out;
  }, [data.rows]);
  const scales = useMemo(() => columnScales(data), [data]);
  const mixedMoney = moneyRowHeader(scales) === MIXED_HEADER;
  const cols = data.tickers.length + 1;
  return (
    <div role="region" tabIndex={0} aria-label="Tabel perbandingan, geser untuk melihat kolom lain" className="max-w-full overflow-x-auto overscroll-x-contain">
      <table className="w-full min-w-max border-collapse text-table">
        <caption className="sr-only">{`Perbandingan ${data.pack?.label ?? "metrik inti"}: metrik pada baris, emiten pada kolom`}</caption>
        <thead>
          <tr className="border-b border-border-hairline">
            <th scope="col" className={cn(STICKY, METRIC_COL, "py-2 text-left text-table-head uppercase text-text-2")}>
              Metrik
            </th>
            {data.tickers.map((t) => (
              <th key={t.ticker} scope="col" className="min-w-[72px] px-2 py-1 text-right align-bottom">
                <Link to={`/perusahaan/${t.ticker}?from=alat`} title={t.name ?? ROLE_WORDS[t.role]} className="inline-flex min-h-11 min-w-11 flex-col items-end justify-center text-table font-bold text-accent">
                  {t.ticker}
                  {mixedMoney && scales.get(t.ticker) ? <span className="text-table-head font-normal uppercase text-text-2">{columnUnit(scales.get(t.ticker)!)}</span> : null}
                </Link>
              </th>
            ))}
          </tr>
        </thead>
        {groups.map((g) => (
          <tbody key={g.id}>
            <tr>
              <th scope="rowgroup" colSpan={cols} className="sticky left-0 h-9 bg-bg pt-3 text-left text-table-head uppercase tracking-wide text-text-2">
                {g.label}
              </th>
            </tr>
            {g.rows.map((row) => {
              const { header, format } = rowFormat(row, scales);
              return (
                <tr key={`${g.id}-${row.metric.id}`} className="h-11 border-b border-border-hairline">
                  <th scope="row" className={cn(STICKY, METRIC_COL, "py-1 text-left align-middle")}>
                    <span className="flex min-w-0 flex-col">
                      <span className="break-words text-label font-semibold leading-snug text-text">{quarterTableLabel(row.metric)}</span>
                      <span className="text-table-head uppercase text-text-2">{header}</span>
                    </span>
                  </th>
                  {row.cells.map((cell) => (
                    <td key={cell.ticker} className="num whitespace-nowrap px-2 text-right align-middle">
                      <Cell cell={cell} row={row} format={(v) => format(v, cell.ticker)} />
                    </td>
                  ))}
                </tr>
              );
            })}
          </tbody>
        ))}
      </table>
    </div>
  );
}

/** Sel kosong tidak boleh hanya strip: daftar alasan dan aksinya (halaman emiten itu) di bawah tabel. */
function Gaps({ data }: { data: CompareResponse }) {
  const items = data.rows.flatMap((row) => row.cells.filter((c) => c.state !== "value" || c.value === null).map((cell) => ({ row, cell, ...gapOf(cell, row) })));
  if (items.length === 0) return null;
  return (
    <details className="rounded-md border border-border-hairline px-3">
      <summary className="flex min-h-11 cursor-pointer items-center text-label font-semibold text-text">{`Sel kosong (${items.length})`}</summary>
      <ul className="flex flex-col pb-2">
        {items.map(({ row, cell, why, action }) => (
          <li key={`${row.metric.id}-${cell.ticker}`} className="flex items-center justify-between gap-2 border-b border-border-hairline last:border-b-0">
            <span className="min-w-0 py-2 text-caption text-text-2">
              <span className="font-semibold text-text">{`${row.metric.label} · ${cell.ticker}`}</span>
              {`: ${why}`}
            </span>
            <Link to={`/perusahaan/${cell.ticker}?section=kuartal&from=alat`} aria-label={`${action} ${row.metric.label} ${cell.ticker}`} className="inline-flex min-h-11 min-w-11 shrink-0 items-center justify-center px-2 text-caption font-bold text-accent">
              {action}
            </Link>
          </li>
        ))}
      </ul>
    </details>
  );
}

/** Calon kolom untuk satu paket: anggota semesta pada paket itu, lalu pembanding mereka. */
function candidates(packId: string | undefined, members: Array<{ ticker: string; pack: { id: string | null }; peers: string[]; peer_of: string | null }>, shown: string[]): string[] {
  const own = members.filter((m) => packId && m.pack.id === packId).map((m) => m.ticker);
  const peers = members.filter((m) => own.includes(m.peer_of ?? "") || own.some((t) => members.find((x) => x.ticker === t)?.peers.includes(m.ticker))).map((m) => m.ticker);
  return [...new Set([...shown, ...own, ...peers])];
}

/**
 * Bandingkan `#/alat/bandingkan` (ui-slicing.md §3): pilih paket dan emiten (chip yang membungkus, tidak menggulir), lalu
 * satu tabel yang menggulir di dalam kotaknya dengan kolom metrik tetap. Data: `GET /compare`.
 */
export function Compare() {
  const [params, setParams] = useSearchParams();
  const packs = usePacks();
  const universe = useUniverse();
  const ticker = params.get("ticker")?.toUpperCase() || undefined;
  const tickersParam = params.get("tickers")?.split(",").filter(Boolean);
  const industry: PackDefinition[] = (packs.data?.packs ?? []).filter((p) => p.id !== "core");
  const packId = params.get("pack") ?? (ticker ? undefined : (industry.find((p) => p.members.length > 0) ?? industry[0])?.id);
  const ready = packId !== undefined || ticker !== undefined;
  const compare = useCompare(ready ? { pack: packId, ticker: packId ? undefined : ticker, tickers: tickersParam } : null);
  const raw = compare.data;
  const data = useMemo(() => (raw ? { ...raw, rows: visibleRows(raw) } : undefined), [raw]);

  const set = (next: Record<string, string | undefined>) => {
    const p = new URLSearchParams(params);
    for (const [k, v] of Object.entries(next)) {
      if (v) p.set(k, v);
      else p.delete(k);
    }
    setParams(p);
  };
  const shown = data?.tickers.map((t) => t.ticker) ?? [];
  const pool = candidates(data?.pack?.id ?? packId, universe.data?.items ?? [], shown);
  const toggle = (t: string) => {
    const next = shown.includes(t) ? shown.filter((x) => x !== t) : pool.filter((x) => x === t || shown.includes(x));
    if (next.length === 0 || next.length > MAX_TICKERS) return;
    set({ tickers: next.join(",") });
  };

  let body;
  if (packs.status === "error" && !ready) body = <LoadError message="Gagal memuat daftar paket" onRetry={packs.reload} />;
  else if (!ready && packs.status === "loading") body = <Loading label="Memuat paket…" rows={1} height={240} />;
  else if (!ready) body = <Empty action={<Link to="/alat/paket" className="inline-flex min-h-11 items-center text-label font-bold text-accent">Lihat paket</Link>}>Belum ada paket untuk dibandingkan.</Empty>;
  else if (compare.status === "error" && !data) body = <LoadError message="Gagal memuat perbandingan" onRetry={compare.reload} />;
  else if (!data) body = <Loading label="Memuat perbandingan…" rows={1} height={240} />;
  else if (data.rows.length === 0) body = <Empty>Belum ada metrik untuk dibandingkan.</Empty>;
  else
    body = (
      <>
        <CompareTable data={data} />
        <Gaps data={data} />
      </>
    );

  return (
    <FactDrawerProvider>
      <div className={PAGE}>
        <header className="flex flex-col gap-1">
          <BackHeader />
          <h1 className="text-title text-text">Bandingkan</h1>
        </header>
        <div className="flex flex-col gap-3">
          <label className="flex flex-col gap-1 text-label font-semibold text-text">
            Paket
            <select value={data?.pack?.id ?? packId ?? ""} onChange={(e) => set({ pack: e.target.value, ticker: undefined, tickers: undefined })} disabled={industry.length === 0} className={FIELD}>
              {packId === undefined ? <option value="">{data ? "Tanpa paket (metrik inti)" : "Pilih paket"}</option> : null}
              {industry.map((p) => (
                <option key={p.id} value={p.id}>{`${p.label} · ${p.version}`}</option>
              ))}
            </select>
          </label>
          {pool.length > 0 ? (
            <div role="group" aria-label="Emiten yang dibandingkan" className="flex flex-wrap gap-2">
              {pool.map((t) => (
                <Button key={t} variant="chip" selected={shown.includes(t)} disabled={!shown.includes(t) && shown.length >= MAX_TICKERS} onClick={() => toggle(t)}>
                  {t}
                </Button>
              ))}
            </div>
          ) : null}
        </div>
        {data?.truncated ? <p className="text-caption text-text-2">{`Daftar bawaan dipotong pada ${MAX_TICKERS} emiten`}</p> : null}
        {compare.error && data ? <LoadError message="Gagal memuat ulang perbandingan" onRetry={compare.reload} /> : null}
        <p role="status" className="sr-only">
          {data ? `${data.rows.length} metrik, ${data.tickers.length} emiten` : ""}
        </p>
        {body}
        <ScreenFooter asOf={data?.as_of} />
      </div>
    </FactDrawerProvider>
  );
}
