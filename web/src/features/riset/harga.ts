import { dayIndex, type DatedPoint } from "../../components/charts/driverGeometry";
import { isKpiCell } from "../research/components/MetricRow";
import type { PackMetric, PackResponse, PricesDriver, PricesResponse } from "../research/types";

/** Akhir kuartal kalender sebagai `YYYY-MM-DD`. */
export function quarterEnd(fy: number, q: number): string {
  const [month, day] = [
    [3, 31],
    [6, 30],
    [9, 30],
    [12, 31],
  ][q - 1];
  return `${fy}-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
}

/** Driver yang dibandingkan dengan satu metrik paket di blok Harga. */
export interface CompareDriver {
  id: string;
  label: string;
  unit: string | null;
  /** Id metrik paket yang dibandingkan (`compare_metric`). */
  metricId: string;
  series: DatedPoint[];
  /** Label seri pengganti bila yang digambar bukan driver itu sendiri. */
  proxyLabel: string | null;
  status: PricesDriver["status"];
}

/**
 * Driver pertama yang punya `compare_metric` (G2); `null` bila paket tidak menautkan satu pun. Seri kosong tetap
 * dikembalikan agar layar menulis celahnya (belum diambil atau belum ada sumber) dan menawarkan **Perbarui data**.
 */
export function compareDriver(prices: PricesResponse): CompareDriver | null {
  const d = prices.drivers.find((x) => x.compare_metric);
  if (!d || !d.compare_metric) return null;
  const own = d.series.length > 0;
  const series = own ? d.series : (d.proxy?.series ?? []);
  return { id: d.id, label: d.label, unit: own ? d.unit : (d.proxy?.unit ?? d.unit), metricId: d.compare_metric, series, proxyLabel: own || !d.proxy?.series.length ? null : d.proxy.label, status: d.status };
}

/** Driver untuk grafik harga: yang ditautkan ke metrik, atau driver pertama yang punya seri. */
export function priceDriver(prices: PricesResponse): { label: string; series: DatedPoint[]; proxyLabel: string | null } | null {
  const linked = compareDriver(prices);
  if (linked && linked.series.length) return linked;
  for (const d of prices.drivers) {
    if (d.series.length) return { label: d.label, series: d.series, proxyLabel: null };
    if (d.proxy?.series.length) return { label: d.label, series: d.proxy.series, proxyLabel: d.proxy.label };
  }
  return null;
}

export type DotState = "entered" | "unknown" | "empty";

export interface QuarterDot {
  /** `2026Q2`. */
  label: string;
  /** Akhir kuartal, posisi X titik. */
  date: string;
  value: number | null;
  state: DotState;
}

/**
 * Nilai metrik paket per kuartal standalone yang jatuh di dalam `[from, to]`. Titik diisi hanya dari nilai yang ada:
 * KPI yang belum diisi atau "Tidak diketahui" dan sel XBRL tanpa nilai menjadi penanda kosong, tidak pernah diperkirakan.
 */
export function quarterDots(pack: PackResponse, metric: PackMetric, from: string, to: string): QuarterDot[] {
  const a = dayIndex(from);
  const b = dayIndex(to);
  const dots: QuarterDot[] = [];
  for (const q of pack.quarters) {
    const date = quarterEnd(q.fy, q.q);
    const day = dayIndex(date);
    if (day < a || day > b) continue;
    const cell = metric.cells.find((c) => c.fy === q.fy && c.q === q.q);
    let value: number | null = null;
    let state: DotState = "empty";
    if (cell && isKpiCell(cell)) {
      if (cell.state === "entered" && typeof cell.value === "number") {
        value = cell.value;
        state = "entered";
      } else if (cell.state === "unknown") state = "unknown";
      else if (cell.state === "not_applicable") continue;
    } else if (cell && cell.value !== null) {
      value = cell.value;
      state = "entered";
    }
    dots.push({ label: q.label, date, value, state });
  }
  return dots;
}

/** Rata-rata nilai driver di kuartal itu; tanpa titik di dalamnya, nilai terakhir sebelum akhir kuartal (seri bulanan). */
export function driverAtQuarter(series: readonly DatedPoint[], fy: number, q: number): number | null {
  const end = dayIndex(quarterEnd(fy, q));
  const start = end - 92;
  const inside = series.filter(([d]) => {
    const day = dayIndex(d);
    return day > start && day <= end;
  });
  if (inside.length) return inside.reduce((s, p) => s + p[1], 0) / inside.length;
  const before = series.filter(([d]) => dayIndex(d) <= end);
  const last = before[before.length - 1];
  return last && end - dayIndex(last[0]) <= 62 ? last[1] : null;
}

export interface RatioStats {
  /** Kuartal terbaru yang punya rasio. */
  latest: { label: string; ratio: number } | null;
  /** Rata-rata rasio pada sampai `window` kuartal terakhir yang punya rasio. */
  average: { n: number; ratio: number } | null;
}

export const RATIO_WINDOW = 8;

/** Rasio nilai terlapor ÷ driver per kuartal (mis. ASP ÷ Newcastle): hanya kuartal yang punya keduanya. */
export function ratioStats(dots: readonly QuarterDot[], series: readonly DatedPoint[]): RatioStats {
  const ratios: Array<{ label: string; ratio: number }> = [];
  for (const dot of dots) {
    if (dot.value === null) continue;
    const m = /^(\d{4})Q([1-4])$/.exec(dot.label);
    if (!m) continue;
    const drv = driverAtQuarter(series, Number(m[1]), Number(m[2]));
    if (drv && drv > 0) ratios.push({ label: dot.label, ratio: dot.value / drv });
  }
  if (ratios.length === 0) return { latest: null, average: null };
  const recent = ratios.slice(-RATIO_WINDOW);
  return { latest: ratios[ratios.length - 1], average: { n: recent.length, ratio: recent.reduce((s, r) => s + r.ratio, 0) / recent.length } };
}
