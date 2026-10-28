import { useState } from "react";
import { Link } from "react-router-dom";
import { ArrowDown, ArrowUp, Eye, EyeOff } from "lucide-react";
import { BudgetBar } from "../../components/BudgetBar";
import { Button } from "../../components/Button";
import { Card } from "../../components/Card";
import { DonutChart } from "../../components/charts/DonutChart";
import { prepareSegments } from "../../components/charts/donutGeometry";
import { DetailStat } from "../../components/DetailStat";
import { Money } from "../../components/Money";
import { SectionHeader } from "../../components/SectionHeader";
import { Sheet } from "../../components/Sheet";
import { Skeleton } from "../../components/Skeleton";
import { StatRow } from "../../components/StatRow";
import { ListState, TransactionRowSkeleton } from "../../components/TransactionRow";
import { catColor, DISTINCT_HUE_REMAP, DISTINCT_HUE_SOURCE, type CatIndex } from "../../lib/categories";
import { cn } from "../../lib/cn";
import { formatPercent, formatPp, formatRupiah, HIDDEN_FULL } from "../../lib/money";
import { formatNumber } from "../../lib/numbers";
import { HIDDEN_ARIA_LABEL, useHideAmounts } from "../../lib/privacy";
import { useResource, type Resource } from "../../lib/useResource";
import { PAGE } from "../../features/riset/ui";

/** Bentuk `GET /api/finance/stock-portfolio` (= MC, tab `PORTOFOLIO`). */
export interface Position {
  no: number;
  category: string;
  ticker: string;
  company_name: string;
  shares: number;
  lots: number;
  avg_price: number;
  current_price: number;
  cost: number;
  valuation: number;
  pl: number;
  pl_pct: number;
  val_alloc_pct: number;
  target_alloc_pct: number;
  drift_pct: number;
}
export interface StockPortfolio {
  summary: { current_valuation: number; total_cost: number; total_pl: number; total_pl_pct: number; cash_balance: number; updated_at?: string };
  positions: Position[];
  available: boolean;
}

export const useStockPortfolio = () => useResource<StockPortfolio>("/api/finance/stock-portfolio");

function portfolioStatus(r: Resource<StockPortfolio>): "ready" | "loading" | "error" | "empty" {
  if (r.status !== "ready") return r.status;
  return r.data && r.data.available && r.data.positions.length ? "ready" : "empty";
}

/** Σ nilai pasar posisi. `summary.current_valuation` sudah termasuk kas (= Σ valuation + cash_balance). */
export const marketValue = (positions: Position[]) => positions.reduce((a, p) => a + p.valuation, 0);

/** Warna dan indeks segmen per ticker mengikuti donut (urut nilai pasar; kecil → "Lainnya", cat-8). */
function tickerColors(positions: Position[]) {
  const segments = prepareSegments(positions.map((p) => ({ label: p.ticker, value: p.valuation })));
  const map = new Map<string, CatIndex>();
  const segmentOf = new Map<string, number>();
  segments.forEach((s, i) => {
    for (const t of s.merged.length ? s.merged : [s.label]) {
      map.set(t, s.cat);
      segmentOf.set(t, i);
    }
  });
  return { segments, map, segmentOf };
}

const dataAge = (s: StockPortfolio["summary"] | undefined) => (s?.updated_at ? `Harga per ${s.updated_at.slice(0, 16).replace("T", " ")}` : null);

/** Satu baris P/L bertanda ("+Rp1,2 jt · +4,1%"): satu-satunya `Rp` di blok hero (design.md §3.2 blok 2). */
function PlLine({ pl, pct }: { pl: number; pct: number }) {
  const { hidden } = useHideAmounts();
  const up = pl >= 0;
  const rupiah = hidden ? HIDDEN_FULL : formatRupiah(pl, { short: true, signed: true });
  const percent = formatPercent(pct, { signed: true });
  return (
    <p className={cn("flex flex-wrap items-center gap-x-1 text-label font-bold", up ? "text-status-safe" : "text-status-over")}>
      {up ? <ArrowUp size={14} strokeWidth={2.25} aria-hidden="true" /> : <ArrowDown size={14} strokeWidth={2.25} aria-hidden="true" />}
      <span className="num" aria-label={hidden ? `${HIDDEN_ARIA_LABEL}, ${percent}` : undefined}>{`${rupiah} · ${percent}`}</span>
      <span className="font-medium text-text-2">belum terealisasi</span>
    </p>
  );
}

const subtitleOf = (p: Position, hidden: boolean) => `${formatNumber(p.lots)} lot · avg ${formatRupiah(p.avg_price, { bare: true, hidden })}`;

function Swatch({ cat, className }: { cat: CatIndex; className?: string }) {
  return <span data-swatch={cat} className={cn("h-3 w-3 shrink-0 rounded-pill", className)} style={{ background: catColor(cat) }} aria-hidden="true" />;
}

export function PortfolioScreen() {
  const pf = useStockPortfolio();
  const { hidden, toggle } = useHideAmounts();
  const [openTicker, setOpenTicker] = useState<string | null>(null);
  const status = portfolioStatus(pf);
  const s = pf.data?.summary;
  const positions = [...(pf.data?.positions ?? [])].sort((a, b) => b.valuation - a.valuation);
  const { segments, map, segmentOf } = tickerColors(positions);
  const [selectedTicker, setSelectedTicker] = useState<string | null>(null);
  const selectedSegment = selectedTicker !== null ? (segmentOf.get(selectedTicker) ?? null) : null;
  const statState = status === "ready" ? "ready" : status;
  const short = (v: number) => formatRupiah(v, { short: true, bare: true, hidden });
  const age = dataAge(s);
  const opened = positions.find((p) => p.ticker === openTicker) ?? null;

  return (
    <div className="contents" style={DISTINCT_HUE_SOURCE}>
      <div className={PAGE} style={DISTINCT_HUE_REMAP}>
        <header>
          <SectionHeader
            variant="screen"
            title="Portofolio"
            trailing={<Button variant="icon" aria-label={hidden ? "Tampilkan nominal" : "Sembunyikan nominal"} aria-pressed={hidden} icon={hidden ? <EyeOff /> : <Eye />} onClick={toggle} className="-mr-1.5" />}
          />
          {age ? <p className="text-caption text-text-2">{age}</p> : null}
        </header>

        <section aria-label="Nilai portofolio" className="flex flex-col gap-1">
          <p className="text-label text-text-2">Nilai portofolio (Rp), termasuk kas</p>
          {status === "loading" ? (
            <div className="mt-1 flex flex-col gap-2">
              <Skeleton width={240} height={40} />
              <Skeleton width={200} height={20} />
            </div>
          ) : status === "error" ? (
            <ListState status="error" onRetry={pf.reload} className="items-start" />
          ) : status === "empty" ? (
            <ListState status="empty" message="Belum ada posisi di tab PORTOFOLIO" className="items-start" />
          ) : s ? (
            <>
              <Money value={s.current_valuation} bare className="block whitespace-nowrap text-hero text-text" />
              <PlLine pl={s.total_pl} pct={s.total_pl_pct} />
            </>
          ) : null}
        </section>

        <section className="flex flex-col gap-2">
          <SectionHeader title="Ringkasan (Rp)" />
          <div role="group" aria-label="Ringkasan portofolio" className="grid grid-cols-3 gap-2">
            {[
              ["Modal", s ? short(s.total_cost) : undefined],
              ["Nilai pasar", s ? short(marketValue(positions)) : undefined],
              ["Kas", s ? short(s.cash_balance) : undefined],
            ].map(([label, value]) => (
              <Card key={label} tone="summary" className="!p-3">
                <StatRow onTint label={label as string} value={value} status={statState} />
              </Card>
            ))}
          </div>
        </section>

        <section className="flex flex-col gap-2">
          <SectionHeader title="Komposisi" />
          {/*
            Donut hanya visual: busurnya (role="button", kecil untuk irisan tipis) tidak ikut dijangkau ketuk, papan ketik
            atau pembaca layar. Aksi pilih ada di baris legenda (44 px) dan menyorot irisan lewat `selected`; angkanya
            tetap terbaca di legenda dan daftar Posisi. React 18 belum mengenal `inert` sebagai prop bertipe.
          */}
          <div aria-hidden="true" {...{ inert: "" }}>
            <DonutChart
              title="Nilai pasar"
              countLabel={(n) => `${n} emiten`}
              data={segments.map((x) => ({ label: x.label, value: x.value }))}
              status={status}
              onRetry={pf.reload}
              selected={selectedSegment}
              onSelectedChange={() => undefined}
            />
          </div>
          {status === "ready" ? (
            <ul aria-label="Legenda komposisi" className="flex flex-col">
              {positions.map((p) => (
                <li key={p.ticker}>
                  <button
                    type="button"
                    aria-pressed={selectedTicker === p.ticker}
                    onClick={() => setSelectedTicker((t) => (t === p.ticker ? null : p.ticker))}
                    className="-mx-2 flex min-h-11 w-[calc(100%+16px)] items-center gap-2 rounded-sm px-2 text-left text-label active:opacity-[0.85] aria-pressed:bg-surface-sunken"
                  >
                    <Swatch cat={map.get(p.ticker) ?? 8} />
                    <span className="min-w-0 flex-1 truncate font-bold text-text">{p.ticker}</span>
                    <span className="num text-text-2">{formatPercent(p.val_alloc_pct)}</span>
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
        </section>

        <section className="flex flex-col gap-2">
          <SectionHeader title="Posisi (Rp)" />
          <div className="flex flex-col">
            {status === "loading" ? (
              Array.from({ length: 4 }, (_, i) => <TransactionRowSkeleton key={i} />)
            ) : status === "ready" ? (
              positions.map((p) => (
                <PositionRow key={p.ticker} p={p} cat={map.get(p.ticker) ?? 8} subtitle={subtitleOf(p, hidden)} onClick={() => setOpenTicker(p.ticker)} />
              ))
            ) : status === "error" ? (
              <ListState status="error" onRetry={pf.reload} />
            ) : (
              <ListState status="empty" message="Belum ada posisi" />
            )}
          </div>
        </section>

        <section className="flex flex-col gap-2">
          <SectionHeader title="Alokasi vs target" />
          <AllocationList positions={positions} colors={map} status={status} />
        </section>

        <footer className="flex flex-col gap-1 border-t border-border-hairline pt-3 text-caption text-text-2">
          {age ? <p>{age}</p> : null}
          <p>Bukan saran investasi</p>
        </footer>

        <PositionSheet p={opened} onClose={() => setOpenTicker(null)} />
      </div>
    </div>
  );
}

/**
 * Baris posisi: ukuran dan pitch seperti `TransactionRow` (57 px), tetapi lokal karena tiga hal: nilai pasar tanpa `Rp`
 * (satuan ada di judul blok; tidak ada `Rp` berulang di dalam kartu, design.md §6), subjudul "200 lot · avg 4.150" turun
 * baris alih-alih dipotong (angka tidak pernah dipotong, finance-app design.md §7), dan swatch hanya selebar dirinya
 * (sejajar dengan legenda di atasnya).
 */
function PositionRow({ p, cat, subtitle, onClick }: { p: Position; cat: CatIndex; subtitle: string; onClick: () => void }) {
  const gain = p.pl_pct >= 0;
  return (
    <button type="button" onClick={onClick} className="-mx-2 flex min-h-[57px] w-[calc(100%+16px)] items-center gap-2 rounded-sm px-2 py-1 text-left active:opacity-[0.85]">
      <Swatch cat={cat} />
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="text-body-strong text-text [overflow-wrap:anywhere]">{p.ticker}</span>
        <span className="text-label text-text-2">{subtitle}</span>
      </span>
      <span className="flex shrink-0 flex-col items-end text-right">
        <Money value={p.valuation} bare className="text-body-strong text-text" />
        <span className={cn("num text-caption font-bold", gain ? "text-status-safe" : "text-status-over")}>{formatPercent(p.pl_pct, { signed: true })}</span>
      </span>
    </button>
  );
}

function AllocationList({ positions, colors, status }: { positions: Position[]; colors: Map<string, CatIndex>; status: string }) {
  if (status === "loading") return <BudgetBar name="" cat={8} status="loading" />;
  if (status !== "ready") return null;
  return (
    <div className="flex flex-col gap-1">
      {positions.map((p) => (
        <div key={p.ticker} className="flex flex-col">
          {/* Nama baris `BudgetBar` dipotong elipsis; angka bobot/target ada di keterangan yang boleh turun baris. */}
          <BudgetBar name={p.ticker} cat={colors.get(p.ticker) ?? 8} spent={p.val_alloc_pct} limit={100} marker={p.target_alloc_pct / 100} rightText={formatPp(p.drift_pct)} />
          <p className="num text-caption text-text-2">{`${formatPercent(p.val_alloc_pct)} / target ${formatPercent(p.target_alloc_pct)}`}</p>
        </div>
      ))}
    </div>
  );
}

/* ---------------------------------------------------------------- detail */

/** Sheet posisi (design.md §3.2, D9): `PositionDetail` lalu **Buka halaman perusahaan** (aturan 4 §2.4: `from=portofolio`). */
function PositionSheet({ p, onClose }: { p: Position | null; onClose: () => void }) {
  return (
    <Sheet
      open={p !== null}
      title={p?.ticker ?? "Posisi"}
      onClose={onClose}
      footer={
        p ? (
          <Link
            to={`/perusahaan/${encodeURIComponent(p.ticker)}?from=portofolio`}
            className="inline-flex min-h-11 w-full items-center justify-center rounded-lg bg-surface-sunken px-6 text-body-strong text-accent active:opacity-[0.85]"
          >
            Buka halaman perusahaan
          </Link>
        ) : undefined
      }
    >
      {p ? <PositionDetail p={p} /> : null}
    </Sheet>
  );
}

export function PositionDetail({ p }: { p: Position }) {
  const { hidden } = useHideAmounts();
  const full = (v: number) => formatRupiah(v, { hidden });
  const plClass = p.pl >= 0 ? "text-status-safe" : "text-status-over";
  return (
    // Badan Sheet (`overflow-y-auto`) bisa tergulir di font besar; wilayah ini bisa difokus dan bernama, jadi papan ketik
    // dapat menggulirnya (WCAG 2.1.1; axe scrollable-region-focusable).
    <div tabIndex={0} role="group" aria-label={`Rincian posisi ${p.ticker}`} className="flex flex-col gap-4 focus-visible:[outline-offset:-2px]">
      <div className="flex flex-col">
        <p className="text-label text-text-2">{p.company_name}</p>
        <p className="text-caption text-text-2">{p.category}</p>
      </div>
      <div className="flex flex-col">
        <DetailStat label="Lot" value={formatNumber(p.lots)} />
        <DetailStat label="Lembar" value={formatNumber(p.shares)} />
        <DetailStat label="Harga rata-rata" value={formatRupiah(p.avg_price, { hidden })} />
        <DetailStat label="Harga close" value={formatRupiah(p.current_price, { hidden })} />
        <DetailStat label="Modal" value={full(p.cost)} />
        <DetailStat label="Nilai pasar" value={full(p.valuation)} />
        <DetailStat label="P/L" value={formatRupiah(p.pl, { hidden, signed: true })} valueClassName={plClass} />
        <DetailStat label="P/L %" value={formatPercent(p.pl_pct, { signed: true })} valueClassName={plClass} />
        <DetailStat label="Bobot" value={formatPercent(p.val_alloc_pct)} />
        <DetailStat label="Target" value={formatPercent(p.target_alloc_pct)} />
        <DetailStat label="Drift" value={formatPp(p.drift_pct)} />
      </div>
    </div>
  );
}
