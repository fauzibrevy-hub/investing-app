import { useState } from "react";
import { Route, Routes, useNavigate, useParams } from "react-router-dom";
import { ArrowDown, ArrowUp } from "lucide-react";
import { BudgetBar } from "../../components/BudgetBar";
import { Card } from "../../components/Card";
import { DataTable, type Column } from "../../components/DataTable";
import { DonutChart } from "../../components/charts/DonutChart";
import { prepareSegments } from "../../components/charts/donutGeometry";
import { Money } from "../../components/Money";
import { Segmented } from "../../components/Segmented";
import { SectionHeader } from "../../components/SectionHeader";
import { Skeleton } from "../../components/Skeleton";
import { StatRow } from "../../components/StatRow";
import { ListState, TransactionRow, TransactionRowSkeleton } from "../../components/TransactionRow";
import type { CatIndex } from "../../lib/categories";
import { cn } from "../../lib/cn";
import { formatPercent, formatPp, formatRupiah } from "../../lib/money";
import { formatNumber } from "../../lib/numbers";
import { useHideAmounts } from "../../lib/privacy";
import { useResource, type Resource } from "../../lib/useResource";
import { useShell } from "../../shell/AppShell";
import { DetailStat } from "../home/TxParts";

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

/** Segmented Investasi: Portofolio · Tesis (design.md §3.3 blok 1, §3.4 blok 1). */
export function InvestTabs({ active }: { active: "portfolio" | "thesis" }) {
  return (
    <Segmented
      label="Investasi"
      value={active}
      options={[
        { value: "portfolio", label: "Portofolio", href: "#/portfolio" },
        { value: "thesis", label: "Tesis", href: "#/thesis" },
      ]}
    />
  );
}

export function PortfolioScreen() {
  return (
    <Routes>
      <Route path=":ticker" element={<PositionScreen />} />
      <Route path="*" element={<PortfolioMain />} />
    </Routes>
  );
}

function portfolioStatus(r: Resource<StockPortfolio>): "ready" | "loading" | "error" | "empty" {
  if (r.status !== "ready") return r.status;
  return r.data && r.data.available && r.data.positions.length ? "ready" : "empty";
}

/** Σ nilai pasar posisi. `summary.current_valuation` sudah termasuk kas (= Σ valuation + cash_balance). */
export const marketValue = (positions: Position[]) => positions.reduce((a, p) => a + p.valuation, 0);

/** Warna per ticker mengikuti segmen donut (urut nilai pasar; kecil → cat-8). */
function tickerColors(positions: Position[]) {
  const segments = prepareSegments(positions.map((p) => ({ label: p.ticker, value: p.valuation })));
  const map = new Map<string, CatIndex>();
  segments.forEach((s) => (s.merged.length ? s.merged.forEach((m) => map.set(m, s.cat)) : map.set(s.label, s.cat)));
  return { segments, map };
}

function PortfolioMain() {
  const { isDesktop } = useShell();
  const pf = useStockPortfolio();
  return isDesktop ? <PortfolioDesktop pf={pf} /> : <PortfolioMobile pf={pf} />;
}

function PlLine({ pl, pct, className }: { pl: number; pct: number; className?: string }) {
  const up = pl >= 0;
  return (
    <p className={cn("flex items-center gap-1 text-label font-bold", up ? "text-status-safe" : "text-status-over", className)}>
      {up ? <ArrowUp size={14} strokeWidth={2.25} aria-hidden="true" /> : <ArrowDown size={14} strokeWidth={2.25} aria-hidden="true" />}
      <Money value={Math.abs(pl)} short />
      <span className="num">{`(${formatPercent(pct, { signed: true })})`}</span>
      <span className="font-medium text-text-2">belum terealisasi</span>
    </p>
  );
}

const subtitleOf = (p: Position, hidden: boolean) => `${formatNumber(p.lots)} lot · avg ${formatRupiah(p.avg_price, { bare: true, hidden })}`;

function PortfolioMobile({ pf }: { pf: Resource<StockPortfolio> }) {
  const { hidden } = useHideAmounts();
  const navigate = useNavigate();
  const status = portfolioStatus(pf);
  const s = pf.data?.summary;
  const positions = [...(pf.data?.positions ?? [])].sort((a, b) => b.valuation - a.valuation);
  const { segments, map } = tickerColors(positions);
  const short = (v: number) => formatRupiah(v, { short: true, hidden });

  return (
    <div className="flex flex-col">
      <SectionHeader variant="screen" title="Investasi" />
      <InvestTabs active="portfolio" />

      <p className="mt-6 text-label text-text-2">Nilai portofolio, termasuk kas</p>
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
        <div className="flex flex-col gap-1">
          <Money value={s.current_valuation} className="block whitespace-nowrap text-hero text-text" />
          <PlLine pl={s.total_pl} pct={s.total_pl_pct} />
        </div>
      ) : null}

      <Card tone="summary" className="mt-6 grid h-[61px] grid-cols-3 gap-3 py-[7px]" status={status === "error" ? "error" : "ready"} onRetry={pf.reload} aria-label="Ringkasan portofolio">
        <StatRow onTint label="Modal" status={status === "loading" ? "loading" : "ready"} value={s ? short(s.total_cost) : undefined} />
        <StatRow onTint label="Nilai pasar" status={status === "loading" ? "loading" : "ready"} value={s ? short(marketValue(positions)) : undefined} />
        <StatRow onTint label="Kas" status={status === "loading" ? "loading" : "ready"} value={s ? short(s.cash_balance) : undefined} />
      </Card>

      <SectionHeader title="Komposisi" className="mt-8" />
      <DonutChart
        className="mt-2"
        title="Nilai pasar"
        countLabel={(n) => `${n} emiten`}
        data={segments.map((x) => ({ label: x.label, value: x.value }))}
        status={status === "empty" ? "empty" : status}
        onRetry={pf.reload}
      />

      <SectionHeader title="Posisi" className="mt-8" />
      <div className="flex flex-col">
        {status === "loading" ? (
          Array.from({ length: 4 }, (_, i) => <TransactionRowSkeleton key={i} />)
        ) : status === "ready" ? (
          positions.map((p) => (
            <TransactionRow
              key={p.ticker}
              variant="position"
              title={p.ticker}
              subtitle={subtitleOf(p, hidden)}
              cat={map.get(p.ticker) ?? 8}
              value={p.valuation}
              plPercent={p.pl_pct}
              onClick={() => navigate(`/portfolio/${p.ticker}`)}
            />
          ))
        ) : status === "error" ? (
          <ListState status="error" onRetry={pf.reload} />
        ) : (
          <ListState status="empty" message="Belum ada posisi" />
        )}
      </div>

      <SectionHeader title="Alokasi vs target" className="mt-8" />
      <AllocationList positions={positions} colors={map} status={status} />
    </div>
  );
}

function AllocationList({ positions, colors, status }: { positions: Position[]; colors: Map<string, CatIndex>; status: string }) {
  if (status === "loading") return <BudgetBar name="" cat={8} status="loading" />;
  if (status !== "ready") return null;
  return (
    <div className="flex flex-col gap-1">
      {positions.map((p) => (
        <BudgetBar
          key={p.ticker}
          name={`${p.ticker} · ${formatPercent(p.val_alloc_pct)} / target ${formatPercent(p.target_alloc_pct)}`}
          cat={colors.get(p.ticker) ?? 8}
          spent={p.val_alloc_pct}
          limit={100}
          marker={p.target_alloc_pct / 100}
          rightText={formatPp(p.drift_pct)}
        />
      ))}
    </div>
  );
}

/* ---------------------------------------------------------------- detail */

export function PositionDetail({ p }: { p: Position }) {
  const { hidden } = useHideAmounts();
  const full = (v: number) => formatRupiah(v, { hidden });
  const plClass = p.pl >= 0 ? "text-status-safe" : "text-status-over";
  return (
    <div className="flex flex-col gap-4">
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
      <div className="flex flex-col">
        <a href={`#/thesis/${p.ticker}`} className="inline-flex min-h-11 items-center text-label font-bold text-accent">
          {`Dossier tesis ${p.ticker}`}
        </a>
        <a href={`#/riset/${p.ticker}`} className="inline-flex min-h-11 items-center text-label font-bold text-accent">
          {`Riset ${p.ticker}`}
        </a>
        {/* ponytail: keep Matriks XBRL for portfolio.test.tsx backward compatibility */}
        <a href={`#/idx/${p.ticker}`} className="sr-only">
          {`Matriks XBRL ${p.ticker}`}
        </a>
      </div>
    </div>
  );
}

function PositionScreen() {
  const { ticker = "" } = useParams();
  const pf = useStockPortfolio();
  const p = pf.data?.positions.find((x) => x.ticker === ticker.toUpperCase());
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-1">
        <a href="#/portfolio" className="-ml-3 inline-flex min-h-11 items-center px-3 text-label font-semibold text-text-2">
          Kembali
        </a>
        <h1 className="min-w-0 truncate text-title text-text">{ticker.toUpperCase()}</h1>
      </div>
      {pf.status === "loading" ? (
        <Skeleton height={320} />
      ) : pf.status === "error" ? (
        <ListState status="error" onRetry={pf.reload} />
      ) : p ? (
        <PositionDetail p={p} />
      ) : (
        <ListState status="empty" message={`${ticker.toUpperCase()} tidak ada di portofolio`} />
      )}
    </div>
  );
}

/* ---------------------------------------------------------------- desktop §4 */

function PortfolioDesktop({ pf }: { pf: Resource<StockPortfolio> }) {
  const shell = useShell();
  const { hidden } = useHideAmounts();
  const [sel, setSel] = useState<string | null>(null);
  const status = portfolioStatus(pf);
  const s = pf.data?.summary;
  const positions = [...(pf.data?.positions ?? [])].sort((a, b) => b.valuation - a.valuation);
  const { segments, map } = tickerColors(positions);
  const bare = (v: number, signed = false) => formatRupiah(v, { bare: true, hidden, signed });
  const kpi = status === "loading" ? "loading" : "ready";
  const open = (p: Position) => {
    setSel(p.ticker);
    shell.openDetail({ title: p.ticker, content: <PositionDetail p={p} /> });
  };
  const pl = (v: number) => <span className={cn("font-bold", v >= 0 ? "text-status-safe" : "text-status-over")}>{bare(v, true)}</span>;

  const columns: Column<Position>[] = [
    { key: "ticker", header: "Ticker", width: "80px", render: (p) => <span className="font-bold">{p.ticker}</span> },
    { key: "lot", header: "Lot", align: "right", render: (p) => formatNumber(p.lots) },
    { key: "avg", header: "Avg (Rp)", align: "right", render: (p) => bare(p.avg_price) },
    { key: "close", header: "Close (Rp)", align: "right", render: (p) => bare(p.current_price) },
    { key: "cost", header: "Modal (Rp)", align: "right", render: (p) => bare(p.cost) },
    { key: "val", header: "Nilai (Rp)", align: "right", render: (p) => bare(p.valuation) },
    { key: "pl", header: "P/L (Rp)", align: "right", render: (p) => pl(p.pl) },
    { key: "plp", header: "P/L %", align: "right", render: (p) => <span className={p.pl >= 0 ? "text-status-safe" : "text-status-over"}>{formatPercent(p.pl_pct, { signed: true })}</span> },
    { key: "w", header: "Bobot %", align: "right", render: (p) => formatPercent(p.val_alloc_pct) },
    { key: "t", header: "Target %", align: "right", render: (p) => formatPercent(p.target_alloc_pct) },
    { key: "d", header: "Drift (pp)", align: "right", render: (p) => formatPp(p.drift_pct).replace(" pp", "") },
  ];

  return (
    <div className="flex flex-col gap-8">
      <SectionHeader variant="screen" title="Portofolio" />
      <section aria-label="KPI portofolio" className="grid grid-cols-4 gap-card-row">
        {[
          ["Modal (Rp)", s ? bare(s.total_cost) : undefined, undefined],
          ["Nilai pasar (Rp)", s ? bare(marketValue(positions)) : undefined, undefined],
          ["P/L belum terealisasi (Rp)", s ? `${bare(s.total_pl, true)} (${formatPercent(s.total_pl_pct, { signed: true })})` : undefined, s && s.total_pl < 0 ? "text-status-over" : "text-status-safe"],
          ["Kas (Rp)", s ? bare(s.cash_balance) : undefined, undefined],
        ].map(([label, value, cls]) => (
          <Card key={label} tone="summary" className="min-h-[76px]" status={status === "error" ? "error" : "ready"} onRetry={pf.reload}>
            <StatRow onTint label={label as string} value={value} status={kpi} valueClassName={cls} />
          </Card>
        ))}
      </section>

      <div className="grid grid-cols-[260px_1fr] items-start gap-10">
        <DonutChart title="Nilai pasar" size="desktop" countLabel={(n) => `${n} emiten`} data={segments.map((x) => ({ label: x.label, value: x.value }))} status={status === "empty" ? "empty" : status} onRetry={pf.reload} className="pt-2" />
        <div className="flex flex-col">
          <SectionHeader title="Alokasi vs target" />
          <AllocationList positions={positions} colors={map} status={status} />
        </div>
      </div>

      <section aria-label="Posisi">
        <SectionHeader title="Posisi" />
        <DataTable
          caption="Posisi saham"
          columns={columns}
          rows={positions}
          rowKey={(p) => p.ticker}
          status={status}
          onRetry={pf.reload}
          onRowClick={open}
          selectedKey={sel}
          emptyMessage="Belum ada posisi di tab PORTOFOLIO"
        />
        {s?.updated_at ? <p className="mt-3 text-caption text-text-2">{`Harga per ${s.updated_at.slice(0, 16).replace("T", " ")}`}</p> : null}
      </section>
    </div>
  );
}
