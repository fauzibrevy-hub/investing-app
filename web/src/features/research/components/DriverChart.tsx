import { memo, useId, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { Button } from "../../../components/Button";
import { clampTooltipLeft, tooltipTop } from "../../../components/charts/trendGeometry";
import {
  DRIVER_LAYOUT,
  dayIndex,
  eventMarkers,
  indexOnPriceAxis,
  levelLines,
  linePath,
  nearestIndex,
  niceAxis,
  tableRows,
  timeScale,
  timeTicks,
  valueAsOf,
  valueScale,
  type ChartTableRow,
  type DatedPoint,
} from "../../../components/charts/driverGeometry";
import { cn } from "../../../lib/cn";
import { formatDate, formatExact, formatValue, unitHeader } from "../format";
import type { PricesDriver, PricesResponse } from "../types";

export const EMPTY_PRICES = "Belum cukup data harga";
export const ERROR_PRICES = "Gagal memuat";

/** Driver yang bisa digambar: seri sendiri, atau seri pengganti berlabel bila driver itu `TBD`. */
export interface PlottedDriver {
  /** Id driver paket (kunci pemilih), bukan id seri pengganti. */
  id: string;
  label: string;
  unit: string | null;
  series: DatedPoint[];
  /** Label seri pengganti bila yang digambar bukan driver itu sendiri. */
  proxyLabel: string | null;
  source: string | null;
}

/** Driver paket yang punya seri (sendiri atau pengganti), urutan payload. Yang pertama = driver utama. */
export function plottedDrivers(drivers: readonly PricesDriver[]): PlottedDriver[] {
  const out: PlottedDriver[] = [];
  for (const d of drivers) {
    if (d.series.length > 0) out.push({ id: d.id, label: d.label, unit: d.unit, series: d.series, proxyLabel: null, source: d.source });
    else if (d.proxy && d.proxy.series.length > 0)
      out.push({ id: d.id, label: d.label, unit: d.proxy.unit, series: d.proxy.series, proxyLabel: d.proxy.label, source: d.proxy.source });
  }
  return out;
}

export interface DriverChartProps {
  prices: PricesResponse | undefined;
  /** Status `useResource`; `ready` tanpa cukup harga menjadi keadaan kosong. */
  status: "loading" | "ready" | "error";
  onRetry?: () => void;
  /** Ringkasan: harga dan driver utama saja (tanpa MA200, level, indeks, penanda). */
  compact?: boolean;
  /** Driver yang digambar; tanpa itu driver utama (yang pertama bisa digambar). */
  driverId?: string | null;
  className?: string;
}

const fmtPrice = (v: number) => formatValue(v, 0);
const fmtIndex = (v: number) => formatValue(v, 1);
/** Nilai driver apa adanya sampai 2 desimal, tanpa nol di belakang. */
const fmtDriver = (v: number) => formatExact(Math.round(v * 100) / 100);
const axisDecimals = (step: number) => (step >= 1 ? 0 : step >= 0.1 ? 1 : 2);

/** Label sumbu: nilai besar dipadatkan (`1,5 T`, `250 M`, `12 jt`) supaya kolom sumbu tidak memakan plot; selebihnya angka penuh. */
export function formatAxis(value: number, decimals: number): string {
  const abs = Math.abs(value);
  const unit = abs >= 1e12 ? ([1e12, "T"] as const) : abs >= 1e9 ? ([1e9, "M"] as const) : abs >= 1e7 ? ([1e6, "jt"] as const) : null;
  return unit ? `${formatExact(Math.round((value / unit[0]) * 10) / 10)} ${unit[1]}` : formatValue(value, decimals);
}

/** Lebar kolom sumbu (px): bawaan 52 (label 12 px sampai 6 karakter), tumbuh bagi label yang lebih panjang. */
const AXIS_CHAR_PX = 7.2;
const axisColumn = (labels: readonly string[]) => Math.max(DRIVER_LAYOUT.axisWidth, Math.ceil(Math.max(0, ...labels.map((l) => l.length)) * AXIS_CHAR_PX) + 8);

const GRID_LINES = 5;
/** Satu array kosong bersama agar memo tidak dihitung ulang tiap render. */
const NONE: never[] = [];

/**
 * Grafik harga dan pendorong (design.md §6, §7 aturan 4, §8): harga `accent` 1,5 px, MA200 `text-3` 1 px, driver `cat-4`
 * 1,5 px di sumbu kanan, indeks relatif `cat-8` 1 px di balik toggle, level putus-putus `text-2`, dividen sebagai tick
 * 6 px dan split sebagai `×` di sumbu X, grid putus-putus `grid`. Legenda adalah daftar DI BAWAH grafik; tidak ada label
 * di plot. SVG ber-`role="img"` dengan ringkasan, plus tabel `sr-only` berisi seri yang digambar.
 */
export function DriverChart({ prices, status, onRetry, compact = false, driverId, className }: DriverChartProps) {
  const plotHeight = DRIVER_LAYOUT.plotHeight[compact ? "compact" : "full"];
  const svgHeight = plotHeight + DRIVER_LAYOUT.xAxisHeight;
  const hintId = useId();

  const wrapRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState<number>(DRIVER_LAYOUT.defaultWidth);
  useLayoutEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const update = () => {
      if (el.clientWidth > 0) setWidth(el.clientWidth);
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const [showIndex, setShowIndex] = useState(false);

  const price: readonly DatedPoint[] = prices?.price ?? NONE;
  const ready = status === "ready" && !!prices && !prices.empty && price.length >= 2;
  const state: "loading" | "error" | "empty" | "ready" = status === "loading" ? "loading" : status === "error" ? "error" : ready ? "ready" : "empty";

  const drivers = useMemo(() => plottedDrivers(prices?.drivers ?? NONE), [prices]);
  const driver = drivers.find((d) => d.id === driverId) ?? drivers[0] ?? null;
  const ma200 = compact ? NONE : (prices?.ma200 ?? NONE);
  const levels = compact ? NONE : (prices?.levels ?? NONE);
  const events = compact ? NONE : (prices?.events ?? NONE);
  const indexAvailable = !compact && (prices?.index_relative.length ?? 0) >= 2;
  const indexOn = indexAvailable && showIndex;
  const indexSeries = indexOn ? (prices?.index_relative ?? NONE) : NONE;

  const hasRight = !!driver;
  // Rentang dari payload; tanggal yang tidak terbaca jatuh ke ujung seri harga.
  const isDate = (d: string | null | undefined): d is string => !!d && Number.isFinite(dayIndex(d));
  const from = [prices?.from, price[0]?.[0]].find(isDate) ?? "1970-01-01";
  const to = [prices?.to, price[price.length - 1]?.[0]].find(isDate) ?? from;
  // Skala, sumbu, dan path dimemo: menggeser titik aktif (pointer, panah) hanya menggambar ulang tooltip dan titiknya.
  const indexMapped = useMemo(() => (indexSeries.length && price.length ? indexOnPriceAxis(indexSeries, price) : []), [indexSeries, price]);
  const leftAxis = useMemo(
    () =>
      niceAxis([...price.map((p) => p[1]), ...ma200.map((p) => p[1]), ...levels.map((l) => l.level), ...indexMapped.map((p) => p[1])]) ??
      { min: 0, max: 1, step: 0.25, ticks: [0, 0.25, 0.5, 0.75, 1] },
    [price, ma200, levels, indexMapped],
  );
  const rightAxis = useMemo(() => (driver ? niceAxis(driver.series.map((p) => p[1])) : null), [driver]);
  const ld = axisDecimals(leftAxis.step);
  const rd = rightAxis ? axisDecimals(rightAxis.step) : 0;
  // Kolom sumbu selebar label terpanjang (angka mentah triliunan dipadatkan oleh `formatAxis`, tapi tetap diukur).
  const leftWidth = axisColumn(leftAxis.ticks.map((t) => formatAxis(t, ld)));
  const rightWidth = rightAxis ? axisColumn(rightAxis.ticks.map((t) => formatAxis(t, rd))) : 0;
  const left = leftWidth;
  const right = width - (hasRight ? rightWidth : 8);
  const xs = useMemo(() => timeScale(from, to, left, right), [from, to, left, right]);
  const yLeft = useMemo(() => valueScale(leftAxis, 0, plotHeight), [leftAxis, plotHeight]);
  const yRight = useMemo(() => (rightAxis ? valueScale(rightAxis, 0, plotHeight) : null), [rightAxis, plotHeight]);
  const paths = useMemo(
    () => ({
      index: indexMapped.length > 1 ? linePath(indexMapped, xs.x, yLeft) : null,
      ma200: ma200.length > 1 ? linePath(ma200, xs.x, yLeft) : null,
      driver: driver && yRight && driver.series.length > 0 ? linePath(driver.series, xs.x, yRight) : null,
      price: linePath(price, xs.x, yLeft),
    }),
    [indexMapped, ma200, driver, price, xs, yLeft, yRight],
  );

  // Titik aktif default = penutupan terakhir; pilihan pengguna berlaku untuk seri tempat ia dipilih saja.
  const [picked, setPicked] = useState<{ series: readonly DatedPoint[]; index: number } | null>(null);
  const lastIndex = Math.max(0, price.length - 1);
  const activeIndex = picked && picked.series === price ? Math.min(picked.index, lastIndex) : lastIndex;
  const activePoint = price[activeIndex];
  const activeDriver = activePoint && driver ? valueAsOf(driver.series, dayIndex(activePoint[0])) : null;

  const move = (i: number) => setPicked({ series: price, index: Math.max(0, Math.min(lastIndex, i)) });
  const dragging = useRef(false);
  const snapTo = (e: PointerEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const ratio = rect.width > 0 ? (e.clientX - rect.left) / rect.width : 0;
    move(nearestIndex(price, xs.dayAt(left + ratio * (right - left))));
  };
  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    dragging.current = true;
    e.currentTarget.setPointerCapture?.(e.pointerId);
    snapTo(e);
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    if (dragging.current || e.pointerType === "mouse") snapTo(e);
  };
  const onPointerEnd = (e: PointerEvent<HTMLDivElement>) => {
    dragging.current = false;
    e.currentTarget.releasePointerCapture?.(e.pointerId);
  };
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = Math.max(1, Math.round(price.length / 20));
    const map: Record<string, number> = {
      ArrowLeft: activeIndex - 1,
      ArrowRight: activeIndex + 1,
      PageUp: activeIndex - step,
      PageDown: activeIndex + step,
      Home: 0,
      End: price.length - 1,
    };
    if (e.key in map) {
      e.preventDefault();
      move(map[e.key]);
    }
  };

  const tipRef = useRef<HTMLDivElement>(null);
  const [tipWidth, setTipWidth] = useState(180);
  const tipText = activePoint
    ? `${fmtPrice(activePoint[1])}${activeDriver && driver ? ` · ${driver.label} ${fmtDriver(activeDriver[1])}` : ""}`
    : "";
  // Tooltip terlihat memadatkan harga triliunan agar muat di kotak grafik; teks lengkap tetap di `aria-valuetext`.
  const tipShown = activePoint
    ? `${Math.abs(activePoint[1]) >= 1e12 ? formatAxis(activePoint[1], 0) : fmtPrice(activePoint[1])}${activeDriver && driver ? ` · ${driver.label} ${fmtDriver(activeDriver[1])}` : ""}`
    : "";
  useLayoutEffect(() => {
    if (tipRef.current && tipRef.current.offsetWidth > 0) setTipWidth(tipRef.current.offsetWidth);
  }, [tipShown, state]);

  const last = price[price.length - 1];
  const lastDriver = driver?.series[driver.series.length - 1] ?? null;
  const driverUnit = driver?.unit ? ` ${unitHeader(driver.unit)}` : "";
  const summary =
    state === "ready" && last
      ? [
          `Harga ${prices!.ticker} terakhir ${fmtPrice(last[1])} rupiah per ${formatDate(last[0])}`,
          driver && lastDriver
            ? `${driver.label}${driver.proxyLabel ? ` (pengganti: ${driver.proxyLabel})` : ""} ${fmtDriver(lastDriver[1])}${driverUnit} per ${formatDate(lastDriver[0])}`
            : "tanpa seri pendorong",
          compact ? null : `${levels.length} level`,
        ]
          .filter(Boolean)
          .join("; ")
      : state === "loading"
        ? "Memuat grafik harga"
        : state === "error"
          ? `Grafik harga: ${ERROR_PRICES}`
          : `Grafik harga: ${EMPTY_PRICES}`;

  const rows = useMemo(
    () => (state === "ready" ? tableRows({ price, ma200, driver: driver?.series ?? NONE, index: indexSeries }) : []),
    [state, price, ma200, driver, indexSeries],
  );

  const gridYs = state === "ready" ? leftAxis.ticks.map(yLeft) : Array.from({ length: GRID_LINES }, (_, i) => (i * plotHeight) / (GRID_LINES - 1));
  const ticks = useMemo(() => (state === "ready" ? timeTicks(xs) : []), [state, xs]);
  const markers = useMemo(() => (state === "ready" ? eventMarkers(events, xs) : []), [state, events, xs]);
  const lines = useMemo(() => (state === "ready" ? levelLines(levels, yLeft, left, right) : []), [state, levels, yLeft, left, right]);
  const px = activePoint ? xs.x(activePoint[0]) : 0;
  const py = activePoint ? yLeft(activePoint[1]) : 0;

  return (
    <figure className={cn("m-0 min-w-0", className)} aria-busy={state === "loading" || undefined}>
      <div ref={wrapRef} className="relative w-full" style={{ height: svgHeight }}>
        <svg width={width} height={svgHeight} viewBox={`0 0 ${width} ${svgHeight}`} role="img" aria-label={summary} className="absolute inset-0 block overflow-visible">
          {gridYs.map((y, i) => (
            <line key={i} data-grid="" x1={left} x2={right} y1={Math.round(y) + 0.5} y2={Math.round(y) + 0.5} stroke="var(--grid)" strokeWidth={1} strokeDasharray="4 4" />
          ))}

          {state === "loading" ? (
            <rect data-skeleton="" x={left} y={0} width={Math.max(0, right - left)} height={plotHeight} rx={8} fill="var(--surface-sunken)" className="animate-skeleton" />
          ) : null}

          {state === "ready" ? (
            <>
              <g aria-hidden="true" data-axis="left">
                {leftAxis.ticks.map((t) => (
                  <text key={t} x={left - 8} y={yLeft(t)} textAnchor="end" dominantBaseline="middle" fill="var(--text-3)" fontSize={12} fontWeight={500} className="num">
                    {formatAxis(t, ld)}
                  </text>
                ))}
              </g>
              {rightAxis && yRight ? (
                <g aria-hidden="true" data-axis="right">
                  {rightAxis.ticks.map((t) => (
                    <text key={t} x={right + 8} y={yRight(t)} dominantBaseline="middle" fill="var(--text-3)" fontSize={12} fontWeight={500} className="num">
                      {formatAxis(t, rd)}
                    </text>
                  ))}
                </g>
              ) : null}
              <g aria-hidden="true" data-axis="x">
                {ticks.map((t) => (
                  <text key={t.day} x={t.x} y={plotHeight + 28} textAnchor="middle" fill="var(--text-3)" fontSize={12} fontWeight={500} className="num">
                    {t.label}
                  </text>
                ))}
              </g>

              {/* Urutan gambar: indeks di belakang, lalu MA200, level, driver, harga di atas. */}
              {paths.index ? <path data-series="index" d={paths.index} fill="none" stroke="var(--cat-8)" strokeWidth={1} strokeLinejoin="round" /> : null}
              {paths.ma200 ? <path data-series="ma200" d={paths.ma200} fill="none" stroke="var(--text-3)" strokeWidth={1} strokeLinejoin="round" /> : null}
              {lines.map((l) => (
                <line key={l.id} data-level={l.id} x1={l.x1} x2={l.x2} y1={l.y} y2={l.y} stroke="var(--text-2)" strokeWidth={1} strokeDasharray="4 3" />
              ))}
              {paths.driver ? <path data-series="driver" d={paths.driver} fill="none" stroke="var(--cat-4)" strokeWidth={1.5} strokeLinejoin="round" /> : null}
              <path data-series="price" d={paths.price} fill="none" stroke="var(--accent)" strokeWidth={1.5} strokeLinejoin="round" strokeLinecap="round" />

              {markers.map((m) =>
                m.kind === "dividend" ? (
                  <line
                    key={`d-${m.date}`}
                    data-marker="dividend"
                    x1={m.x}
                    x2={m.x}
                    y1={plotHeight + DRIVER_LAYOUT.markerOffset - 3}
                    y2={plotHeight + DRIVER_LAYOUT.markerOffset - 3 + DRIVER_LAYOUT.dividendTick}
                    stroke="var(--text-2)"
                    strokeWidth={1}
                  />
                ) : (
                  <text key={`s-${m.date}`} data-marker="split" x={m.x} y={plotHeight + DRIVER_LAYOUT.markerOffset + 4} textAnchor="middle" fill="var(--text-2)" fontSize={11} fontWeight={700} aria-hidden="true">
                    ×
                  </text>
                ),
              )}

              {activePoint ? (
                <g data-active-point="" aria-hidden="true">
                  <circle cx={px} cy={py} r={5.5} fill="var(--bg)" />
                  <circle cx={px} cy={py} r={3.5} fill="var(--accent)" />
                </g>
              ) : null}
            </>
          ) : null}
        </svg>

        {state === "ready" && activePoint ? (
          <>
            <div
              ref={tipRef}
              data-tooltip=""
              aria-hidden="true"
              className="pointer-events-none absolute flex h-7 items-center whitespace-nowrap rounded-sm bg-surface-inverse px-3 text-label"
              style={{ left: tipWidth > right - left ? clampTooltipLeft(px, tipWidth, 0, width) : clampTooltipLeft(px, tipWidth, left, right), top: tooltipTop(py) }}
            >
              <span className="font-medium text-tooltip-date">{formatDate(activePoint[0])}</span>
              &nbsp;
              <span className="num font-extrabold text-white">{tipShown}</span>
            </div>
            <div
              role="slider"
              tabIndex={0}
              aria-label={`Titik harga ${prices!.ticker}`}
              aria-valuemin={0}
              aria-valuemax={price.length - 1}
              aria-valuenow={activeIndex}
              aria-valuetext={`${formatDate(activePoint[0])}, harga ${tipText}`}
              aria-describedby={hintId}
              className="absolute top-0 cursor-crosshair rounded-sm"
              style={{ left, width: Math.max(0, right - left), height: plotHeight, touchAction: "pan-y" }}
              onPointerDown={onPointerDown}
              onPointerMove={onPointerMove}
              onPointerUp={onPointerEnd}
              onPointerCancel={onPointerEnd}
              onKeyDown={onKeyDown}
            />
            <p id={hintId} className="sr-only">
              Panah kiri dan kanan memindah titik; tabel seri lengkap ada setelah legenda.
            </p>
          </>
        ) : null}

        {state === "empty" || state === "error" ? (
          <div className="absolute flex flex-col items-center justify-center gap-1 text-center" style={{ left, right: width - right, top: 0, height: plotHeight }}>
            <p role={state === "error" ? "alert" : undefined} className={cn("text-label", state === "error" ? "font-semibold text-status-over" : "text-text-2")}>
              {state === "error" ? ERROR_PRICES : EMPTY_PRICES}
            </p>
            {state === "error" && onRetry ? (
              <Button variant="ghost" onClick={onRetry} className="bg-bg">
                Coba lagi
              </Button>
            ) : null}
          </div>
        ) : null}
      </div>

      {state === "ready" ? (
        <>
          <ul aria-label="Legenda grafik" data-legend="" className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-label text-text-2">
            <LegendItem swatch="var(--accent)" width={1.5}>
              Harga (Rp)
            </LegendItem>
            {ma200.length > 1 ? (
              <LegendItem swatch="var(--text-3)" width={1}>
                MA200 (Rp)
              </LegendItem>
            ) : null}
            {driver ? (
              <LegendItem swatch="var(--cat-4)" width={1.5}>
                {`${driver.label}${driverUnit}${driver.proxyLabel ? ` · pengganti: ${driver.proxyLabel}` : ""} · sumbu kanan`}
              </LegendItem>
            ) : null}
            {indexAvailable ? (
              <li className="inline-flex items-center">
                <button
                  type="button"
                  aria-pressed={indexOn}
                  onClick={() => setShowIndex((v) => !v)}
                  className={cn("inline-flex min-h-11 items-center gap-2 rounded-sm px-1", indexOn ? "font-bold text-text" : "text-text-2")}
                >
                  <Swatch color="var(--cat-8)" width={1} />
                  {`Indeks relatif ${prices!.index_symbol} (100 = awal rentang)`}
                </button>
              </li>
            ) : null}
            {levels.map((l) => (
              <LegendItem key={l.id} swatch="var(--text-2)" width={1} dashed>
                {`Level ${fmtPrice(l.level)}${l.entered !== l.level ? ` (ditulis ${fmtPrice(l.entered)}, disesuaikan split)` : ""} · ${l.text} · ${formatDate(l.at)}`}
              </LegendItem>
            ))}
            {events.map((e) => (
              <li key={`${e.kind}-${e.date}`} className="inline-flex min-h-6 items-center gap-2">
                <span aria-hidden="true" className="inline-flex w-4 justify-center font-bold">
                  {e.kind === "dividend" ? "|" : "×"}
                </span>
                {e.kind === "dividend" ? `Dividen ${formatExact(e.amount)} · ${formatDate(e.date)}` : `Split ${e.ratio_num}:${e.ratio_den} · ${formatDate(e.date)}`}
              </li>
            ))}
          </ul>

          <SeriesTable
            ticker={prices!.ticker}
            rows={rows}
            compact={compact}
            driverHeader={driver ? `${driver.label}${driverUnit}` : null}
            indexOn={indexOn}
          />
        </>
      ) : null}
    </figure>
  );
}

/** Tabel `sr-only` seri yang digambar; dimemo karena bisa ribuan baris dan tidak bergantung pada titik aktif. */
const SeriesTable = memo(function SeriesTable({
  ticker,
  rows,
  compact,
  driverHeader,
  indexOn,
}: {
  ticker: string;
  rows: readonly ChartTableRow[];
  compact: boolean;
  driverHeader: string | null;
  indexOn: boolean;
}) {
  // Pembungkus `sr-only` (div, overflow tersembunyi): tabel absolut yang lebih lebar dari 1 px tetap menyumbang lebar
  // gulir halaman karena `overflow` tidak berlaku pada elemen tabel; di zoom 200% itu menggulirkan Harga ke samping.
  return (
    <div className="sr-only">
      <table className="sr-only-table">
        <caption>{`Seri yang digambar, ${ticker}`}</caption>
        <thead>
          <tr>
            <th scope="col">Tanggal</th>
            <th scope="col">Harga (Rp)</th>
            {compact ? null : <th scope="col">MA200 (Rp)</th>}
            {driverHeader ? <th scope="col">{driverHeader}</th> : null}
            {indexOn ? <th scope="col">Indeks relatif</th> : null}
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.date}>
              <th scope="row">{formatDate(r.date)}</th>
              <td>{fmtPrice(r.price)}</td>
              {compact ? null : <td>{r.ma200 === null ? "tidak ada" : fmtPrice(r.ma200)}</td>}
              {driverHeader ? <td>{r.driver === null ? "tidak ada" : fmtDriver(r.driver)}</td> : null}
              {indexOn ? <td>{r.index === null ? "tidak ada" : fmtIndex(r.index)}</td> : null}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
});

function Swatch({ color, width, dashed = false }: { color: string; width: number; dashed?: boolean }) {
  return (
    <svg width={16} height={8} aria-hidden="true" className="shrink-0">
      <line x1={0} x2={16} y1={4} y2={4} stroke={color} strokeWidth={width + 0.5} strokeDasharray={dashed ? "4 3" : undefined} />
    </svg>
  );
}

function LegendItem({ swatch, width, dashed, children }: { swatch: string; width: number; dashed?: boolean; children: string }) {
  return (
    <li className="inline-flex min-h-6 items-center gap-2">
      <Swatch color={swatch} width={width} dashed={dashed} />
      {children}
    </li>
  );
}
