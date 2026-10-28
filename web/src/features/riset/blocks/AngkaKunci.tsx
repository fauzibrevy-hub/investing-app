import { useMemo } from "react";
import { Skeleton } from "../../../components/Skeleton";
import { useDossier, useFinancials, usePack, useValuation } from "../../research/api";
import { SourcedStat } from "../../research/components/SourcedStat";
import { formatDelta, formatValue, periodLabel } from "../../research/format";
import { relativeChange, signChangePhrase } from "../../research/change";
import type { FinancialsResponse, PackResponse, YoY } from "../../research/types";
import { formatMetric } from "../../research/components/metricFormat";
import { isBankTaxonomy, unitCaption } from "../lastQuarter";
import { formatCompactMoney } from "../money";
import { Block } from "../ui";

export interface NetIncome {
  period: string;
  value: number;
  /** Persen terhadap kuartal yang sama tahun lalu, atau pergantian tanda; `null` bila tidak ada pembanding. */
  yoy: YoY | null;
}

/** Laba bersih kuartal standalone terbaru dan YoY terhadap kuartal yang sama tahun lalu; `null` bila baris tidak ada. */
export function latestNetIncome(fin: FinancialsResponse): NetIncome | null {
  const row = fin.groups.flatMap((g) => g.rows).find((r) => r.id === "net_income");
  const last = fin.periods.length - 1;
  const cur = row?.cells[last]?.value;
  if (!row || last < 0 || cur === null || cur === undefined) return null;
  const p = fin.periods[last];
  const prevIndex = fin.periods.findIndex((x) => x.fy === p.fy - 1 && x.q === p.q);
  const prev = prevIndex >= 0 ? row.cells[prevIndex]?.value : null;
  return { period: periodLabel(p.label), value: cur, yoy: prev === null || prev === undefined ? null : relativeChange(cur, prev) };
}

/** Keterangan di bawah laba bersih: pergantian tanda ditulis, pembanding yang tidak ada disebut. */
function netIncomeDetail(yoy: YoY | null): string[] {
  if (yoy === null) return ["Belum ada kuartal yang sama tahun lalu untuk YoY"];
  if (yoy.kind === "sign_change") return [`YoY: berbalik ${signChangePhrase(yoy.from, yoy.to, true)}`];
  return [];
}

/** Utang bersih terbaru dari metrik inti `net_debt` di payload paket. */
export function latestNetDebt(pack: PackResponse): { value: number; period: string } | null {
  const metric = pack.dimensions.flatMap((d) => d.metrics).find((m) => m.id === "net_debt");
  return metric?.latest ? { value: metric.latest.value, period: periodLabel(metric.latest.label) } : null;
}

export interface Headline {
  label: string;
  /** Nilai sudah diformat; satuan `%` ada di label. */
  value: string;
  period: string;
}

/**
 * Angka utama paket untuk bank, pengganti utang bersih: metrik pertama di dimensi `pembiayaan` atau `profitabilitas`
 * yang punya nilai terbaru. `null` bila paket tidak punya satu pun.
 */
export function latestHeadline(pack: PackResponse, currency: string | null): Headline | null {
  const metrics = pack.dimensions.filter((d) => d.id === "pembiayaan" || d.id === "profitabilitas").flatMap((d) => d.metrics);
  const metric = metrics.find((m) => m.latest !== null);
  if (!metric?.latest) return null;
  const period = periodLabel(metric.latest.label);
  const unit = metric.unit === "ccy" ? "" : ` (${unitCaption(metric.unit, currency, null)})`;
  const value = metric.unit === "ccy" ? formatCompactMoney(metric.latest.value, currency) : formatMetric(metric.latest.value, metric.unit);
  return { label: `${metric.label} ${period}${unit}`, value, period };
}

/** Blok 2: enam angka dalam grid 2 × 3, dua baris tiap angka; yang kosong menyebut alasannya. */
export function AngkaKunci({ ticker }: { ticker: string }) {
  const dossier = useDossier(ticker);
  const valuation = useValuation(ticker);
  const fin = useFinancials(ticker, "standalone", 12);
  const pack = usePack(ticker);
  const loading = [dossier, valuation, fin, pack].some((r) => r.status === "loading" && !r.data);

  const net = useMemo(() => (fin.data ? latestNetIncome(fin.data) : null), [fin.data]);
  const bank = isBankTaxonomy(fin.data?.taxonomy);
  const debt = useMemo(() => (pack.data && !bank ? latestNetDebt(pack.data) : null), [pack.data, bank]);
  const reportCcy = fin.data?.currency ?? null;
  const headline = useMemo(() => (pack.data && bank ? latestHeadline(pack.data, reportCcy) : null), [pack.data, bank, reportCcy]);
  const v = valuation.data;
  const close = dossier.data?.latest_close ?? null;
  const pe = v?.multiples.pe.ttm.value ?? null;
  const pb = v?.multiples.pb.ttm.value ?? null;
  const failed = (r: { status: string }) => r.status === "error";

  if (loading) {
    return (
      <Block id="angka" title="Angka kunci">
        <div role="status" aria-busy="true" className="grid grid-cols-2 gap-x-4 gap-y-2">
          <span className="sr-only">Memuat angka kunci…</span>
          {Array.from({ length: 6 }, (_, i) => (
            <div key={i} className="flex min-h-11 flex-col gap-1">
              <Skeleton width={72} height={14} />
              <Skeleton width={56} height={18} />
            </div>
          ))}
        </div>
      </Block>
    );
  }

  const base = `GET /${ticker}`;
  return (
    <Block id="angka" title="Angka kunci">
      <div data-testid="angka-kunci" className="grid grid-cols-2 gap-x-4 gap-y-2">
        <SourcedStat
          labelFirst
          label="Harga (Rp)"
          name="Harga"
          value={close ? formatValue(close.close) : null}
          source={`Penutupan harian, ${base}`}
          date={close?.date}
          gapReason={failed(dossier) ? "Gagal memuat harga" : "Belum ada harga"}
        />
        <SourcedStat
          labelFirst
          label="Kap. pasar"
          name="Kapitalisasi pasar"
          value={v?.market_cap != null ? formatCompactMoney(v.market_cap, "IDR") : null}
          source={`Harga penutupan × jumlah saham, ${base}/valuation`}
          date={v?.price.date}
          detail={v?.shares ? [`Saham beredar ${formatValue(v.shares.count)} · ${v.shares.source_label}`] : []}
          gapReason={failed(valuation) ? "Gagal memuat valuasi" : "Jumlah saham belum ada"}
        />
        <SourcedStat
          labelFirst
          label="P/E TTM (×)"
          name="P/E TTM"
          value={pe !== null ? formatValue(pe, 1) : null}
          source={`Harga ÷ laba bersih 12 bulan, ${base}/valuation`}
          date={v?.price.date}
          detail={v?.multiples.pe.ttm.caption ? [v.multiples.pe.ttm.caption] : []}
          gapReason={failed(valuation) ? "Gagal memuat valuasi" : "Belum ada laba 12 bulan"}
        />
        <SourcedStat
          labelFirst
          label="P/B (×)"
          name="P/B"
          value={pb !== null ? formatValue(pb, 1) : null}
          source={`Harga ÷ ekuitas per saham, ${base}/valuation`}
          date={v?.price.date}
          detail={v?.multiples.pb.ttm.caption ? [v.multiples.pb.ttm.caption] : []}
          gapReason={failed(valuation) ? "Gagal memuat valuasi" : "Belum ada ekuitas per saham"}
        />
        <SourcedStat
          labelFirst
          label={net ? `Laba bersih ${net.period} (YoY)` : "Laba bersih kuartal terakhir"}
          name="Laba bersih kuartal terakhir"
          value={net ? `${formatCompactMoney(net.value, reportCcy)}${net.yoy?.kind === "pct" ? ` · ${formatDelta(net.yoy.value)}%` : ""}` : null}
          source={`Kuartal standalone, ${base}/financials`}
          detail={net ? netIncomeDetail(net.yoy) : []}
          gapReason={failed(fin) ? "Gagal memuat laporan" : "Laba bersih belum ada di laporan"}
        />
        {bank ? (
          <SourcedStat
            labelFirst
            label={headline?.label ?? "Angka utama paket"}
            name={headline?.label ?? "Angka utama paket"}
            value={headline?.value ?? null}
            source={`Metrik paket ${pack.data?.pack.label ?? "industri"}, ${base}/pack`}
            gapReason={failed(pack) ? "Gagal memuat paket" : "Paket belum punya angka pembiayaan atau profitabilitas"}
          />
        ) : (
          <SourcedStat
            labelFirst
            label={debt ? `Utang bersih ${debt.period}` : "Utang bersih"}
            name="Utang bersih"
            value={debt ? formatCompactMoney(debt.value, reportCcy) : null}
            source={`Utang berbunga dikurangi kas, ${base}/pack`}
            detail={debt && debt.value < 0 ? ["Negatif berarti kas bersih"] : []}
            gapReason={failed(pack) ? "Gagal memuat paket" : "Belum ada neraca"}
          />
        )}
      </div>
    </Block>
  );
}
