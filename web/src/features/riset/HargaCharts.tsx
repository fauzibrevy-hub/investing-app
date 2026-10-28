import { useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { dayIndex, linePath, niceAxis, rebaseTo100, timeScale, timeTicks, valueScale, type DatedPoint } from "../../components/charts/driverGeometry";
import { formatAxis } from "../research/components/DriverChart";
import { formatDate, formatExact, formatValue, periodLabel } from "../research/format";
import type { QuarterDot } from "./harga";

const DEFAULT_WIDTH = 357;
const AXIS_W = 46;
const X_AXIS_H = 26;

/** Lebar kotak grafik yang terukur; sebelum terukur, lebar layar iPhone minus gutter. */
function useWidth() {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(DEFAULT_WIDTH);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const update = () => {
      if (el.clientWidth > 0) setWidth(el.clientWidth);
    };
    update();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, width] as const;
}

const decimals = (step: number) => (step >= 1 ? 0 : step >= 0.1 ? 1 : 2);
const inRange = (series: readonly DatedPoint[], from: string, to: string) => {
  const a = dayIndex(from);
  const b = dayIndex(to);
  return series.filter(([d]) => {
    const day = dayIndex(d);
    return day >= a && day <= b;
  });
};

function Swatch({ color, hollow = false, dot = false }: { color: string; hollow?: boolean; dot?: boolean }) {
  return (
    <svg width={16} height={8} aria-hidden="true" className="shrink-0">
      {dot ? <circle cx={8} cy={4} r={3.5} fill={hollow ? "var(--bg)" : color} stroke={color} strokeWidth={1.5} /> : <line x1={0} x2={16} y1={4} y2={4} stroke={color} strokeWidth={2} />}
    </svg>
  );
}

function Legend({ items }: { items: Array<{ key: string; swatch: ReactNode; text: string }> }) {
  return (
    <ul aria-label="Legenda grafik" className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-label text-text-2">
      {items.map((i) => (
        <li key={i.key} className="inline-flex min-h-6 items-center gap-2">
          {i.swatch}
          {i.text}
        </li>
      ))}
    </ul>
  );
}

export interface DriverVsReportedProps {
  from: string;
  to: string;
  driverLabel: string;
  driverUnit: string | null;
  proxyLabel: string | null;
  driver: readonly DatedPoint[];
  metricLabel: string;
  metricUnit: string;
  dots: readonly QuarterDot[];
  /** Satuan driver dan metrik sama: satu sumbu; berbeda: metrik di sumbu kanan. */
  sameUnit: boolean;
  plotHeight?: number;
}

/**
 * Grafik 1 (ui-slicing.md §1.1): rata-rata bulanan driver (garis `cat-4`) dan nilai terlapor per kuartal standalone
 * (titik `accent` di akhir kuartal) pada satu sumbu satuan. Kuartal yang belum diisi menjadi penanda kosong di dasar plot.
 */
export function DriverVsReported({ from, to, driverLabel, driverUnit, proxyLabel, driver, metricLabel, metricUnit, dots, sameUnit, plotHeight = 150 }: DriverVsReportedProps) {
  const [ref, width] = useWidth();
  const series = useMemo(() => inRange(driver, from, to), [driver, from, to]);
  const filled = dots.filter((d) => d.value !== null);
  const empty = dots.filter((d) => d.value === null);
  const dotValues = filled.map((d) => d.value as number);
  const left = niceAxis([...series.map((p) => p[1]), ...(sameUnit ? dotValues : [])]) ?? { min: 0, max: 1, step: 0.25, ticks: [0, 0.25, 0.5, 0.75, 1] };
  const right = sameUnit ? null : niceAxis(dotValues);
  const x0 = AXIS_W;
  const x1 = width - (right ? AXIS_W : 8);
  const xs = useMemo(() => timeScale(from, to, x0, x1), [from, to, x0, x1]);
  const yl = valueScale(left, 4, plotHeight);
  const yr = right ? valueScale(right, 4, plotHeight) : yl;
  const height = plotHeight + X_AXIS_H;
  const ticks = useMemo(() => timeTicks(xs, 5), [xs]);
  const driverName = `${driverLabel}${driverUnit ? ` (${driverUnit})` : ""}`;
  const summary = [
    `${driverName}${proxyLabel ? `, pengganti: ${proxyLabel}` : ""}: ${series.length} titik bulanan`,
    `${metricLabel} (${metricUnit}): ${filled.length} kuartal terisi${empty.length ? `, ${empty.length} belum diisi` : ""}`,
  ].join("; ");

  return (
    <figure className="m-0 min-w-0">
      <div ref={ref} className="relative w-full" style={{ height }}>
        <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`} role="img" aria-label={summary} className="absolute inset-0 block overflow-visible">
          {left.ticks.map((t) => (
            <g key={t}>
              <line x1={x0} x2={x1} y1={Math.round(yl(t)) + 0.5} y2={Math.round(yl(t)) + 0.5} stroke="var(--grid)" strokeWidth={1} strokeDasharray="4 4" />
              <text x={x0 - 6} y={yl(t)} textAnchor="end" dominantBaseline="middle" fill="var(--text-3)" fontSize={12} fontWeight={500} className="num" aria-hidden="true">
                {formatAxis(t, decimals(left.step))}
              </text>
            </g>
          ))}
          {right
            ? right.ticks.map((t) => (
                <text key={t} x={x1 + 6} y={yr(t)} dominantBaseline="middle" fill="var(--text-3)" fontSize={12} fontWeight={500} className="num" aria-hidden="true">
                  {formatAxis(t, decimals(right.step))}
                </text>
              ))
            : null}
          {ticks.map((t) => (
            <text key={t.day} x={t.x} y={plotHeight + 20} textAnchor="middle" fill="var(--text-3)" fontSize={12} fontWeight={500} className="num" aria-hidden="true">
              {t.label}
            </text>
          ))}
          {series.length > 1 ? <path data-series="driver" d={linePath(series, xs.x, yl)} fill="none" stroke="var(--cat-4)" strokeWidth={1.5} strokeLinejoin="round" /> : null}
          {filled.map((d) => (
            <circle key={d.label} data-dot={d.label} cx={xs.x(d.date)} cy={yr(d.value as number)} r={4.5} fill="var(--accent)" stroke="var(--bg)" strokeWidth={1.5} />
          ))}
          {empty.map((d) => (
            <circle key={d.label} data-dot-empty={d.label} cx={xs.x(d.date)} cy={plotHeight - 2} r={4} fill="var(--bg)" stroke="var(--accent)" strokeWidth={1.5} />
          ))}
        </svg>
      </div>
      <Legend
        items={[
          { key: "driver", swatch: <Swatch color="var(--cat-4)" />, text: `${driverName}${proxyLabel ? ` · pengganti: ${proxyLabel}` : ""}, rata-rata bulanan` },
          { key: "dots", swatch: <Swatch color="var(--accent)" dot />, text: `${metricLabel} (${metricUnit}) per kuartal${sameUnit ? "" : ", sumbu kanan"}` },
          ...(empty.length ? [{ key: "empty", swatch: <Swatch color="var(--accent)" dot hollow />, text: `belum diisi · ${empty.map((d) => periodLabel(d.label)).join(", ")}` }] : []),
        ]}
      />
      <div className="sr-only">
        <table className="sr-only-table">
          <caption>{`${metricLabel} per kuartal`}</caption>
          <thead>
            <tr>
              <th scope="col">Kuartal</th>
              <th scope="col">{`${metricLabel} (${metricUnit})`}</th>
            </tr>
          </thead>
          <tbody>
            {dots.map((d) => (
              <tr key={d.label}>
                <th scope="row">{periodLabel(d.label)}</th>
                <td>{d.value !== null ? formatExact(d.value) : d.state === "unknown" ? "tidak diketahui" : "belum diisi"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </figure>
  );
}

export interface PriceVsDriverProps {
  ticker: string;
  from: string;
  to: string;
  price: readonly DatedPoint[];
  driverLabel: string | null;
  proxyLabel: string | null;
  driver: readonly DatedPoint[];
  plotHeight?: number;
}

/**
 * Grafik 2 (ui-slicing.md §1.1): penutupan saham (`accent`) dan driver (`cat-4`) keduanya dirasionalkan ke 100 di awal
 * rentang, sehingga gerak bersamanya terlihat tanpa sumbu kedua. Tanpa seri driver, hanya harga yang digambar.
 */
export function PriceVsDriver({ ticker, from, to, price, driverLabel, proxyLabel, driver, plotHeight = 150 }: PriceVsDriverProps) {
  const [ref, width] = useWidth();
  const priceIdx = useMemo(() => rebaseTo100(inRange(price, from, to)), [price, from, to]);
  const driverIdx = useMemo(() => rebaseTo100(inRange(driver, from, to)), [driver, from, to]);
  const axis = niceAxis([...priceIdx.map((p) => p[1]), ...driverIdx.map((p) => p[1])]) ?? { min: 0, max: 200, step: 50, ticks: [0, 50, 100, 150, 200] };
  const x0 = AXIS_W;
  const x1 = width - 8;
  const xs = useMemo(() => timeScale(from, to, x0, x1), [from, to, x0, x1]);
  const y = valueScale(axis, 4, plotHeight);
  const ticks = useMemo(() => timeTicks(xs, 5), [xs]);
  const height = plotHeight + X_AXIS_H;
  const lastPrice = priceIdx[priceIdx.length - 1];
  const lastDriver = driverIdx[driverIdx.length - 1];
  const summary = `Harga ${ticker} (100 = awal rentang) terakhir ${lastPrice ? formatValue(lastPrice[1], 1) : "belum ada"} per ${lastPrice ? formatDate(lastPrice[0]) : "-"}${driverLabel && lastDriver ? `; ${driverLabel} ${formatValue(lastDriver[1], 1)}` : "; tanpa seri pendorong"}`;

  return (
    <figure className="m-0 min-w-0">
      <div ref={ref} className="relative w-full" style={{ height }}>
        <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`} role="img" aria-label={summary} className="absolute inset-0 block overflow-visible">
          {axis.ticks.map((t) => (
            <g key={t}>
              <line x1={x0} x2={x1} y1={Math.round(y(t)) + 0.5} y2={Math.round(y(t)) + 0.5} stroke="var(--grid)" strokeWidth={1} strokeDasharray="4 4" />
              <text x={x0 - 6} y={y(t)} textAnchor="end" dominantBaseline="middle" fill="var(--text-3)" fontSize={12} fontWeight={500} className="num" aria-hidden="true">
                {formatValue(t, decimals(axis.step))}
              </text>
            </g>
          ))}
          {ticks.map((t) => (
            <text key={t.day} x={t.x} y={plotHeight + 20} textAnchor="middle" fill="var(--text-3)" fontSize={12} fontWeight={500} className="num" aria-hidden="true">
              {t.label}
            </text>
          ))}
          {driverIdx.length > 1 ? <path data-series="driver" d={linePath(driverIdx, xs.x, y)} fill="none" stroke="var(--cat-4)" strokeWidth={1.5} strokeLinejoin="round" /> : null}
          {priceIdx.length > 1 ? <path data-series="price" d={linePath(priceIdx, xs.x, y)} fill="none" stroke="var(--accent)" strokeWidth={1.5} strokeLinejoin="round" strokeLinecap="round" /> : null}
        </svg>
      </div>
      <Legend
        items={[
          { key: "price", swatch: <Swatch color="var(--accent)" />, text: `Harga ${ticker} (100 = awal rentang)` },
          ...(driverLabel && driverIdx.length > 1 ? [{ key: "driver", swatch: <Swatch color="var(--cat-4)" />, text: `${driverLabel}${proxyLabel ? ` · pengganti: ${proxyLabel}` : ""} (100 = awal rentang)` }] : []),
        ]}
      />
    </figure>
  );
}
