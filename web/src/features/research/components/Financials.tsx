import { useId, useMemo, useState } from "react";
import { Card } from "../../../components/Card";
import { Segmented } from "../../../components/Segmented";
import { Skeleton } from "../../../components/Skeleton";
import { useFilings, useFinancials } from "../api";
import { useFactDrawer } from "./factDrawerContext";
import { ProvenanceBadge, badgeLegend, badgesForFlags, type ProvenanceKind } from "./ProvenanceBadge";
import { QuarterTable } from "./QuarterTable";
import { quarterTableFromFinancials } from "./quarterTableData";
import { formatDate, formatScaled, periodLabel, scaleTitle, type Scale } from "../format";
import type { FactTarget, FilingRow, FinancialsGroup, FinancialsResponse, FinancialsRow, FinancialsView, SectionProps, ShareCount } from "../types";

export const VIEWS: Array<{ value: FinancialsView; label: string }> = [
  { value: "standalone", label: "Kuartal" },
  { value: "ttm", label: "TTM" },
  { value: "annual", label: "Tahunan" },
];
const VIEW_WORDS: Record<FinancialsView, string> = { standalone: "kuartal", ttm: "TTM", annual: "tahunan" };

type PeriodCount = "8" | "12" | "20";
export const PERIOD_COUNTS: Array<{ value: PeriodCount; label: string }> = [
  { value: "8", label: "8" },
  { value: "12", label: "12" },
  { value: "20", label: "20" },
];

// ---- pengelompokan bank (design.md §4.2 Keuangan) ----

const BANK_TAXONOMY = /financial|sharia|syariah/i;

interface BankGroup {
  id: string;
  label: string;
  match: (row: FinancialsRow, group: string) => boolean;
}

/** Urutan cocok: kualitas aset sebelum pembiayaan (elemen cadangan pembiayaan masuk kualitas aset). */
const BANK_MATCH: BankGroup[] = [
  { id: "kualitas_aset", label: "Kualitas aset", match: (r) => /Allowance|Impairment|NonPerforming|Provision/i.test(r.element) },
  {
    id: "pembiayaan",
    label: "Pembiayaan dan pendanaan",
    match: (r) => ["deposits", "borrowings", "debt_short", "debt_long", "bonds"].includes(r.id) || /Loan|Financing|Deposit|ThirdPartyFund|Murabahah|Musyarakah|Mudharabah|Ijarah|Qardh/i.test(r.element),
  },
  { id: "pendapatan", label: "Pendapatan", match: (_r, g) => g === "laba_rugi" },
  { id: "neraca", label: "Neraca", match: (_r, g) => g === "neraca" },
  { id: "arus_kas", label: "Arus kas", match: () => true },
];
const BANK_ORDER = ["pendapatan", "pembiayaan", "kualitas_aset", "neraca", "arus_kas"];

/** Bank (taksonomi financial atau sharia): Pendapatan, Pembiayaan dan pendanaan, Kualitas aset, Neraca, lalu Arus kas. */
export function groupFinancials(fin: FinancialsResponse): FinancialsGroup[] {
  if (!fin.taxonomy || !BANK_TAXONOMY.test(fin.taxonomy)) return fin.groups.filter((g) => g.rows.length);
  const buckets = new Map<string, FinancialsRow[]>();
  for (const g of fin.groups) {
    for (const r of g.rows) {
      const target = BANK_MATCH.find((b) => b.match(r, g.id))!;
      buckets.set(target.id, [...(buckets.get(target.id) ?? []), r]);
    }
  }
  return BANK_ORDER.filter((id) => buckets.get(id)?.length).map((id) => ({ id, label: BANK_MATCH.find((b) => b.id === id)!.label, rows: buckets.get(id)! }));
}

// ---- saham beredar ----

/** Target fakta untuk hitungan saham yang dibaca dari laporan; `null` untuk manual dan turunan. */
export function shareTarget(share: ShareCount): FactTarget | null {
  const s = share.source;
  if (s.kind === "filing") return { filing_id: s.filing_id, element: s.element, ctx_kind: "instant", start: null, end: s.period_end, dims: "", ctx: s.period_end };
  if (s.kind === "latest_instant") return { filing_id: s.filing_id, element: s.element, ctx_kind: "instant", start: null, end: s.as_of, dims: "", ctx: s.as_of };
  return null;
}

function shareTitle(share: ShareCount): string {
  const s = share.source;
  if (s.kind === "manual") return `Diisi pemilik per ${formatDate(s.as_of)}: ${s.source_note}`;
  if (s.kind === "derived") return `Laba ÷ laba per saham dasar, laporan ${s.filing_id}`;
  return share.source_label;
}

const SHARE_STEPS: Array<{ divisor: number; word: string }> = [
  { divisor: 1, word: "" },
  { divisor: 1e3, word: "ribu" },
  { divisor: 1e6, word: "juta" },
  { divisor: 1e9, word: "miliar" },
];

/**
 * Satu skala untuk seluruh tabel saham, satuannya di header (`(JUTA LEMBAR)`): skala terbesar yang menyisakan median
 * empat digit atau lebih. Satu langkah lebih rinci dari tabel uang (median ≥ 100), karena perubahan jumlah saham (buyback,
 * right issue) biasanya kecil dan hilang bila 371.335.392.068 dibulatkan menjadi 371 miliar.
 */
export function shareScale(values: ReadonlyArray<number | null>): Scale {
  const present = values.filter((v): v is number => typeof v === "number" && Number.isFinite(v)).map(Math.abs).sort((a, b) => a - b);
  const mid = present.length ? present[present.length >> 1] : 0;
  let chosen = SHARE_STEPS[0];
  for (const step of SHARE_STEPS) if (mid / step.divisor >= 1000) chosen = step;
  return { divisor: chosen.divisor, word: chosen.word, header: chosen.word ? `(${chosen.word.toUpperCase()} LEMBAR)` : "(LEMBAR)" };
}

/** Jalur periode berurutan dengan sumber yang sama, mis. `Q3 23 – Q2 26`. */
export interface ShareRun {
  from: string;
  to: string;
  /** Indeks periode pertama dan terakhir jalur ini. */
  first: number;
  last: number;
  share: ShareCount | null;
}

const sameSource = (a: ShareCount | null, b: ShareCount | null) => (a === null || b === null ? a === b : a.source_label === b.source_label && shareTitle(a) === shareTitle(b));

/** Sumber per periode, periode berurutan dengan sumber yang sama digabung jadi satu jalur. */
export function shareRuns(fin: FinancialsResponse): ShareRun[] {
  const runs: ShareRun[] = [];
  fin.periods.forEach((p, i) => {
    const share = fin.shares[i] ?? null;
    const last = runs[runs.length - 1];
    if (last && sameSource(last.share, share)) Object.assign(last, { to: periodLabel(p.label), last: i });
    else runs.push({ from: periodLabel(p.label), to: periodLabel(p.label), first: i, last: i, share });
  });
  return runs;
}

/**
 * Saham beredar lewat `QuarterTable`, jadi ikut lebar kolom sticky yang sama dengan tabel lain. Hitungan dari laporan
 * membuka `FactDrawer`; hitungan manual dan turunan tidak punya fakta, jadi selnya memakai target penanda lokal yang
 * membawa fokus ke sumbernya di daftar di bawah tabel (badge manual/turunan, tanggal, catatan).
 */
function SharesTable({ fin }: { fin: FinancialsResponse }) {
  const drawer = useFactDrawer();
  const listId = useId();
  const label = "Saham beredar";
  const { table, markers, runs, scale } = useMemo(() => {
    const runs = shareRuns(fin);
    const scale = shareScale(fin.shares.map((s) => s?.shares ?? null));
    // Target penanda (identitas objek) → indeks jalur sumbernya; tidak pernah sampai ke drawer.
    const markers = new Map<FactTarget, number>();
    const cells = fin.periods.map((p, i) => {
      const share = fin.shares[i] ?? null;
      if (!share) return { value: null, fact: null };
      let fact = shareTarget(share);
      if (!fact) {
        fact = { filing_id: "", element: "", ctx_kind: "instant", start: null, end: p.label, dims: "", ctx: p.label };
        markers.set(fact, runs.findIndex((r) => r.first <= i && i <= r.last));
      }
      return { value: share.shares, text: formatScaled(share.shares, scale), fact };
    });
    const table = {
      periods: fin.periods.map((p) => ({ key: p.label, label: periodLabel(p.label) })),
      groups: [{ id: "saham", label: "Per periode", rows: [{ id: "shares", label, cells }] }],
    };
    return { table, markers, runs, scale };
  }, [fin]);

  const open = (target: FactTarget, trigger: HTMLElement) => {
    const run = markers.get(target);
    if (run === undefined) drawer?.open(target, trigger);
    else document.getElementById(`${listId}-${run}`)?.focus();
  };

  return (
    <div className="flex min-w-0 flex-col gap-2">
      <QuarterTable
        caption={`${label}, ${VIEW_WORDS[fin.view]}`}
        title={scaleTitle(label, scale)}
        unitHeader={scale.header}
        periods={table.periods}
        groups={table.groups}
        onOpenFact={open}
        opensDialog={(target) => !markers.has(target)}
      />
      <ul aria-label="Sumber saham beredar" className="flex flex-col px-4 text-caption text-text-2">
        {runs.map((r, i) => {
          const kind = r.share?.source.kind;
          const badge = kind === "manual" ? "manual" : kind === "derived" ? "turunan" : null;
          // Fokus datang dari klik sel (programatik), jadi cincinnya `focus:`, bukan `focus-visible:`.
          return (
            <li key={`${r.from}-${i}`} id={`${listId}-${i}`} tabIndex={-1} className="flex min-h-11 flex-wrap items-center gap-x-2 gap-y-1 rounded-sm py-1 focus:outline focus:outline-2 focus:outline-accent">
              <span className="font-medium text-text">{r.from === r.to ? r.from : `${r.from} – ${r.to}`}</span>
              {badge ? <ProvenanceBadge kind={badge} text={badge} /> : null}
              <span>{r.share ? shareTitle(r.share) : "tidak ada"}</span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

// ---- daftar laporan ----

const filingKey = (fy: number, period: string) => `${fy}${period}`;

/** Laporan untuk periode yang tampil: kuartal `2026Q2` → laporan Q2 2026, Q4 dan tahunan → laporan FY. */
export function filingsForPeriods(items: readonly FilingRow[], fin: FinancialsResponse): FilingRow[] {
  const wanted = new Set(fin.periods.map((p) => filingKey(p.fy, fin.view === "annual" || p.q === 4 ? "FY" : `Q${p.q}`)));
  return items.filter((f) => wanted.has(filingKey(f.fy, f.period))).sort((a, b) => b.period_end.localeCompare(a.period_end));
}

function FilingList({ ticker, fin }: { ticker: string; fin: FinancialsResponse }) {
  const filings = useFilings(ticker);
  if (filings.status === "error") return <Card as="section" aria-label="Laporan" status="error" errorMessage={filings.error?.message ?? "Gagal memuat laporan"} onRetry={filings.reload} minHeight={96} />;
  if (filings.status === "loading" || !filings.data) {
    return (
      <div role="status" aria-busy="true">
        <span className="sr-only">Memuat daftar laporan…</span>
        <Skeleton height={160} />
      </div>
    );
  }
  const rows = filingsForPeriods(filings.data.items, fin);
  if (!rows.length) return <Card as="section" aria-label="Laporan" status="empty" emptyMessage="Belum ada laporan untuk periode ini" minHeight={96} />;
  // Satu kolom, satu baris per laporan: tabel enam kolom tidak muat di ponsel (design.md: halaman tidak menggulir ke samping).
  return (
    <section aria-labelledby="filings-title" className="relative min-w-0 max-w-full rounded-md border border-border-hairline bg-bg">
      <h3 id="filings-title" className="px-4 pb-2 pt-4 text-body-strong text-text">
        Laporan
      </h3>
      <ul aria-label="Laporan untuk periode yang tampil" className="flex flex-col">
        {rows.map((f) => (
          <li key={f.id} className="flex min-h-11 flex-col justify-center gap-0.5 border-t border-border-hairline px-4 py-2 text-caption text-text-2">
            <span className="text-body-strong text-text">{`${f.period} ${f.fy}`}</span>
            <span>{`Tersedia ${formatDate(f.available_from)}${f.available_source === "estimated" ? " · perkiraan" : ""}`}</span>
            <span>{[f.audited ? "diaudit" : "belum diaudit", f.taxonomy, f.rounding, f.currency].join(" · ")}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

// ---- tabel per grup ----

/**
 * Tampilan tahunan: setiap kolom adalah tahun penuh, jadi bendera `derived_fy` tidak membedakan sel apa pun dan badge
 * `FY`-nya hanya melebarkan kolom (di ponsel tinggal satu kolom tahun yang terlihat).
 */
export function withoutAnnualDerived(fin: FinancialsResponse): FinancialsResponse {
  if (fin.view !== "annual") return fin;
  return { ...fin, groups: fin.groups.map((g) => ({ ...g, rows: g.rows.map((r) => ({ ...r, cells: r.cells.map((c) => ({ ...c, flags: c.flags.filter((f) => f !== "derived_fy") })) })) })) };
}

function GroupTables({ fin }: { fin: FinancialsResponse }) {
  const { tables, legend } = useMemo(() => {
    const shown = withoutAnnualDerived(fin);
    const tables = groupFinancials(shown).map((g) => {
      // Skala per tabel: median grup itu sendiri (design.md §4), judul menyebut skalanya.
      const data = quarterTableFromFinancials({ ...shown, groups: [g] });
      return { id: g.id, label: g.label, data };
    });
    // Badge ringkas yang benar-benar tampil: hanya sel berangka dengan asal angka (FactChip) memuat badge.
    const kinds = new Set<ProvenanceKind>();
    for (const t of tables) for (const g of t.data.groups) for (const r of g.rows) for (const c of r.cells) if (c.value !== null && c.fact) badgesForFlags(c.flags).forEach((k) => kinds.add(k));
    return { tables, legend: badgeLegend(kinds) };
  }, [fin]);
  return (
    <>
      {tables.map((t) => (
        <QuarterTable key={t.id} caption={`${t.label}, ${VIEW_WORDS[fin.view]}`} title={scaleTitle(t.label, t.data.scale)} unitHeader={t.data.unitHeader} periods={t.data.periods} groups={t.data.groups} />
      ))}
      {legend ? (
        <p data-badge-legend="" className="-mt-3 px-4 text-caption text-text-2">
          {legend}
        </p>
      ) : null}
    </>
  );
}

/**
 * Keuangan (design.md §4.2): `Segmented` Kuartal · TTM · Tahunan dan jumlah periode 8 · 12 · 20; `QuarterTable`
 * per grup (bank dikelompokkan ulang) dengan sel `FactChip` dan badge §3.2; saham beredar (juga `QuarterTable`) dengan
 * sumber per periode; daftar laporan satu kolom untuk periode yang tampil dengan tanda audit.
 */
export function Financials({ ticker }: SectionProps) {
  const [view, setView] = useState<FinancialsView>("standalone");
  const [count, setCount] = useState<PeriodCount>("12");
  const fin = useFinancials(ticker, view, Number(count));
  const data = fin.data;

  let body;
  if (fin.status === "error") body = <Card status="error" errorMessage={fin.error?.message ?? "Gagal memuat laporan"} onRetry={fin.reload} minHeight={160} />;
  else if (fin.status === "loading" || !data) body = <QuarterTable caption={`Keuangan, ${VIEW_WORDS[view]}`} unitHeader="" periods={[]} groups={[]} status="loading" />;
  else if (!data.periods.length || !data.groups.some((g) => g.rows.length)) body = <Card status="empty" emptyMessage="Belum ada laporan" minHeight={160} />;
  else
    body = (
      <>
        <GroupTables fin={data} />
        <SharesTable fin={data} />
        <FilingList ticker={ticker} fin={data} />
      </>
    );

  return (
    <div className="flex min-w-0 flex-col gap-6">
      {/* Ponsel: bertumpuk, masing-masing selebar kartu; desktop: berdampingan. */}
      <div className="flex flex-col gap-3 lg:flex-row">
        <Segmented label="Tampilan laporan" options={VIEWS} value={view} onChange={setView} className="lg:flex-[2]" />
        <Segmented label="Jumlah periode" options={PERIOD_COUNTS} value={count} onChange={setCount} className="lg:flex-1" />
      </div>
      {data && fin.refreshing ? <p className="sr-only" role="status">Memuat ulang…</p> : null}
      {body}
    </div>
  );
}
