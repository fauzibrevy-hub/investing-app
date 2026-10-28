import { useMemo, useState } from "react";
import { Segmented } from "../../../components/Segmented";
import { StatRow } from "../../../components/StatRow";
import { usePack, usePrices } from "../../research/api";
import { SourcedValue } from "../../research/components/SourcedValue";
import { SourcedStat } from "../../research/components/SourcedStat";
import { formatDelta, formatPct, formatValue, periodLabel } from "../../research/format";
import type { PackResponse, PriceRange, PricesResponse } from "../../research/types";
import { DriverVsReported, PriceVsDriver } from "../HargaCharts";
import { compareDriver, priceDriver, quarterDots, ratioStats } from "../harga";
import { RefreshButton } from "../RefreshButton";
import { Block, Empty, LoadError, Loading } from "../ui";

/** Rentang mobile (design.md §5.2): tanpa 10T. */
const RANGES: Array<{ value: PriceRange; label: string }> = [
  { value: "1y", label: "1T" },
  { value: "3y", label: "3T" },
  { value: "5y", label: "5T" },
];

const norm = (unit: string | null) => (unit ?? "").toLowerCase().replace(/\s+/g, "");
const fmtPrice = (v: number) => formatValue(v, 0);
const NO_HISTORY = "Belum cukup riwayat harga";

function SubStat({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <span className="flex items-baseline justify-between gap-2 text-caption text-text-2">
      <span className="truncate">{label}</span>
      {children}
    </span>
  );
}

/** Enam statistik harga dalam grid 2 × 3; tiap angka membuka asalnya. */
function PriceStats({ ticker, prices, held }: { ticker: string; prices: PricesResponse; held: boolean }) {
  const s = prices.stats;
  const closeDate = s.close.date;
  const base = `Harga penutupan harian ${ticker} (disesuaikan split), GET /${ticker}/prices`;
  return (
    <div data-testid="price-stats" className="grid grid-cols-2 items-start gap-x-4 gap-y-3">
      <SourcedStat stacked labelFirst label="Close (Rp)" name="Close" value={s.close.value === null ? null : fmtPrice(s.close.value)} source={base} date={closeDate} gapReason="Belum ada harga" />
      <SourcedStat stacked labelFirst
        label="Tertinggi 52 minggu (Rp)"
        name="Tertinggi 52 minggu"
        value={s.high_52w ? fmtPrice(s.high_52w.value) : null}
        source={base}
        date={s.high_52w?.date}
        detail={["Penutupan tertinggi 52 minggu terakhir"]}
        gapReason={NO_HISTORY}
        sub={
          s.high_52w ? (
            <SubStat label="Dari puncak (%)">
              <SourcedValue text={formatPct(s.high_52w.drawdown_pct)} label="Jarak close dari puncak 52 minggu" source={base} date={closeDate} detail={["(close ÷ tertinggi 52 minggu − 1) × 100"]} gapReason={NO_HISTORY} />
            </SubStat>
          ) : undefined
        }
      />
      <SourcedStat stacked labelFirst
        label="Volatilitas 1 tahun (%)"
        name="Volatilitas 1 tahun"
        value={s.volatility_1y_pct === null ? null : formatPct(s.volatility_1y_pct)}
        source={base}
        date={closeDate}
        detail={["Simpangan baku log-return harian 1 tahun, disetahunkan"]}
        gapReason={NO_HISTORY}
      />
      <SourcedStat stacked labelFirst
        label={`vs ${prices.index_symbol} 6 bulan (PP)`}
        name={`Selisih return 6 bulan terhadap ${prices.index_symbol}`}
        value={s.vs_index_6m_pct === null ? null : formatDelta(s.vs_index_6m_pct)}
        source={`${base}; indeks ${prices.index_symbol}`}
        date={closeDate}
        detail={[`Return 6 bulan dikurangi return ${prices.index_symbol} 6 bulan, poin persen`]}
        gapReason={`Seri ${prices.index_symbol} belum tersedia`}
      />
      <SourcedStat stacked labelFirst
        label="Imbal hasil dividen TTM (%)"
        name="Imbal hasil dividen TTM"
        value={s.dividend_yield_ttm_pct === null ? null : formatPct(s.dividend_yield_ttm_pct)}
        source={`${base}; dividen dari aksi korporasi`}
        date={closeDate}
        detail={["Dividen per saham 12 bulan terakhir ÷ close × 100"]}
        gapReason={NO_HISTORY}
      />
      {held ? (
        <SourcedStat stacked labelFirst
          label="Rata-rata biaya (Rp)"
          name="Rata-rata biaya"
          value={s.avg_cost ? fmtPrice(s.avg_cost.value) : null}
          source="Harga rata-rata posisi dari portofolio (Sheets)"
          gapReason="Rata-rata biaya belum ada di portofolio"
          sub={
            s.avg_cost ? (
              <SubStat label="Close vs biaya (%)">
                <SourcedValue
                  text={s.avg_cost.gap_pct === null ? null : formatDelta(s.avg_cost.gap_pct)}
                  label="Selisih close terhadap rata-rata biaya"
                  source={`${base}; rata-rata biaya dari portofolio`}
                  date={closeDate}
                  detail={["(close ÷ rata-rata biaya − 1) × 100"]}
                  gapReason="Belum ada harga"
                />
              </SubStat>
            ) : undefined
          }
        />
      ) : (
        <StatRow label="Rata-rata biaya (Rp)" value="Tidak dipegang" wrapLabel className="min-h-11" />
      )}
    </div>
  );
}

function ChartOne({ prices, pack, from, to }: { prices: PricesResponse; pack: PackResponse | undefined; from: string; to: string }) {
  const link = compareDriver(prices);
  const metric = link && pack ? pack.dimensions.flatMap((d) => d.metrics).find((m) => m.id === link.metricId) : undefined;
  const dots = useMemo(() => (pack && metric ? quarterDots(pack, metric, from, to) : []), [pack, metric, from, to]);
  if (!link) return <p className="text-label text-text-2">Paket ini belum menautkan pendorong harga ke metrik laporan, jadi perbandingan per kuartal belum bisa digambar.</p>;
  if (link.series.length === 0) {
    return (
      <div className="flex flex-col items-start gap-1 rounded-md border border-dashed border-border-hairline p-4">
        <p className="text-label text-text-2">
          {link.status === "TBD" ? `${link.label}: belum ada sumber, jadi perbandingan dengan ${metric?.label ?? link.metricId} belum bisa digambar.` : `${link.label}: belum diambil, jadi perbandingan dengan ${metric?.label ?? link.metricId} belum bisa digambar.`}
        </p>
        {link.status === "TBD" ? null : <RefreshButton scope="market" />}
      </div>
    );
  }
  if (!metric) return <p className="text-label text-text-2">{`Metrik ${link.metricId} tidak ada di paket ini.`}</p>;
  const sameUnit = norm(link.unit) === norm(metric.unit);
  const ratio = sameUnit ? ratioStats(dots, link.series) : null;
  return (
    <>
      <DriverVsReported
        from={from}
        to={to}
        driverLabel={link.label}
        driverUnit={link.unit}
        proxyLabel={link.proxyLabel}
        driver={link.series}
        metricLabel={metric.label}
        metricUnit={metric.unit}
        dots={dots}
        sameUnit={sameUnit}
      />
      {ratio?.latest && ratio.average ? (
        <p data-testid="ratio-caption" className="text-caption text-text-2">{`${metric.label} / ${link.label} ${periodLabel(ratio.latest.label)}: ${formatValue(ratio.latest.ratio, 2)} · rata-rata ${ratio.average.n} kuartal: ${formatValue(ratio.average.ratio, 2)}`}</p>
      ) : (
        <p className="text-caption text-text-2">{sameUnit ? `Belum ada kuartal dengan ${metric.label} terisi, jadi rasio belum dihitung.` : `Satuan ${link.label} (${link.unit ?? "?"}) berbeda dari ${metric.label} (${metric.unit}); rasio tidak dihitung.`}</p>
      )}
    </>
  );
}

function HargaBody({ ticker, held }: { ticker: string; held: boolean }) {
  const [range, setRange] = useState<PriceRange>("3y");
  const prices = usePrices(ticker, range);
  const pack = usePack(ticker);
  const data = prices.data;
  let body;
  if (prices.status === "error" && !data) body = <LoadError message={prices.error?.message ?? "Gagal memuat harga"} onRetry={prices.reload} />;
  else if (!data) body = <Loading label="Memuat harga…" rows={2} height={180} />;
  else if (data.empty || data.price.length < 2) body = <Empty>Belum cukup data harga untuk {ticker}. Perbarui data untuk mengambilnya.</Empty>;
  else {
    const from = data.from;
    const to = data.to ?? data.price[data.price.length - 1][0];
    const drv = priceDriver(data);
    body = (
      <>
        <section aria-label="Pendorong dan angka terlapor" className="flex flex-col gap-2">
          <h3 className="text-body-strong text-text">Pendorong → angka terlapor</h3>
          <ChartOne prices={data} pack={pack.data} from={from} to={to} />
        </section>
        <section aria-label="Harga saham dan pendorong" className="flex flex-col gap-2">
          <h3 className="text-body-strong text-text">Harga saham vs pendorong</h3>
          <PriceVsDriver ticker={ticker} from={from} to={to} price={data.price} driverLabel={drv?.label ?? null} proxyLabel={drv?.proxyLabel ?? null} driver={drv?.series ?? []} />
        </section>
        <PriceStats ticker={ticker} prices={data} held={held} />
      </>
    );
  }
  return (
    <>
      <Segmented<PriceRange> label="Rentang harga" options={RANGES} value={range} onChange={setRange} />
      {body}
    </>
  );
}

/** Blok 8 "Harga" (lipat; ui-slicing.md §1.1): dua grafik bersumbu waktu sama, rasio, dan enam statistik. */
export function Harga({ ticker, held, open, onToggle }: { ticker: string; held: boolean; open: boolean; onToggle: () => void }) {
  return (
    <Block id="harga" title="Harga" open={open} onToggle={onToggle}>
      <HargaBody ticker={ticker} held={held} />
    </Block>
  );
}
