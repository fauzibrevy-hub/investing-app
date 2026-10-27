import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { cn } from "../../lib/cn";
import { formatRupiah } from "../../lib/money";
import { HIDDEN_ARIA_LABEL, useHideAmounts } from "../../lib/privacy";
import { useIsDesktop } from "../../lib/useMediaQuery";
import { Button } from "../Button";
import type { LoadState } from "../Card";
import {
  areaPath,
  clampTooltipLeft,
  defaultXTicks,
  gridValues,
  linePath,
  makeScales,
  nearestIndex,
  niceMax,
  tooltipTop,
  TREND_LAYOUT,
  type TrendPoint,
} from "./trendGeometry";

export interface TrendChartProps {
  /** Seri utama (periode ini). Berhenti di titik terakhir yang ada datanya. */
  current: TrendPoint[];
  /** Seri pembanding (periode lalu), penuh sampai akhir periode. */
  previous?: TrendPoint[];
  currentLabel: string;
  previousLabel?: string;
  /** Domain X; default dari data. Bulanan: [1, jumlah hari]. */
  xDomain?: [number, number];
  xTicks?: { x: number; label: string }[];
  /** Default: rupiah penuh (tooltip) dan singkat tanpa Rp (axis). */
  formatValue?: (v: number, hidden: boolean) => string;
  formatAxis?: (v: number, hidden: boolean) => string;
  status?: LoadState;
  onRetry?: () => void;
  size?: "mobile" | "desktop";
  ariaLabel?: string;
  className?: string;
  /** Dipanggil saat titik aktif berubah (indeks di `current`). */
  onActiveChange?: (index: number) => void;
}

const defaultFormatValue = (v: number, hidden: boolean) => formatRupiah(v, { hidden });
// Mode sembunyi juga menyamarkan axis: skala Y membocorkan nominal.
const defaultFormatAxis = (v: number, hidden: boolean) => formatRupiah(v, { short: true, bare: true, hidden });

export function TrendChart({
  current,
  previous = [],
  currentLabel,
  previousLabel,
  xDomain,
  xTicks,
  formatValue = defaultFormatValue,
  formatAxis = defaultFormatAxis,
  status = "ready",
  onRetry,
  size,
  ariaLabel,
  className,
  onActiveChange,
}: TrendChartProps) {
  const isDesktop = useIsDesktop();
  const plotHeight = TREND_LAYOUT.plotHeight[size ?? (isDesktop ? "desktop" : "mobile")];
  const { hidden } = useHideAmounts();
  const fadeId = `trend-fade-${useId().replace(/:/g, "")}`;

  // Lebar mengikuti kolom konten.
  const wrapRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState<number>(TREND_LAYOUT.defaultWidth);
  useLayoutEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const update = () => {
      const w = el.clientWidth;
      if (w > 0) setWidth(w);
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const hasData = current.length >= 2 || previous.length >= 2;
  const effective: LoadState = status === "ready" && !hasData ? "empty" : status;
  const ready = effective === "ready";

  const all = useMemo(() => [...current, ...previous], [current, previous]);
  const [xMin, xMax] = xDomain ?? (all.length ? [Math.min(...all.map((p) => p.x)), Math.max(...all.map((p) => p.x))] : [1, 31]);
  const yMax = niceMax(Math.max(0, ...all.map((p) => p.value)));
  const s = makeScales({ width, plotHeight, xMin, xMax, yMax });
  const ticks = xTicks ?? defaultXTicks(xMin, xMax, current.length ? current : previous);

  // Titik aktif default = data terakhir seri utama.
  const [active, setActive] = useState(Math.max(0, current.length - 1));
  // Reset hanya saat jumlah titik berubah (array baru tiap render tidak boleh membatalkan drag).
  useEffect(() => setActive(Math.max(0, current.length - 1)), [current.length]);
  const activeIndex = Math.min(active, Math.max(0, current.length - 1));
  const activePoint = current[activeIndex];

  const move = (index: number) => {
    const next = Math.max(0, Math.min(current.length - 1, index));
    if (next !== activeIndex) {
      setActive(next);
      onActiveChange?.(next);
    }
  };

  // ---- interaksi drag dengan snap ----
  const dragging = useRef(false);
  const snapTo = (e: PointerEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const plotW = s.plotRight - s.plotLeft;
    const ratio = rect.width > 0 ? (e.clientX - rect.left) / rect.width : 0;
    move(nearestIndex(current, s, s.plotLeft + ratio * plotW));
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
    // Lepas jari → tetap di titik terakhir.
    dragging.current = false;
    e.currentTarget.releasePointerCapture?.(e.pointerId);
  };
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const map: Record<string, number> = { ArrowLeft: activeIndex - 1, ArrowRight: activeIndex + 1, Home: 0, End: current.length - 1 };
    if (e.key in map) {
      e.preventDefault();
      move(map[e.key]);
    }
  };

  // ---- tooltip ----
  const tipRef = useRef<HTMLDivElement>(null);
  const [tipWidth, setTipWidth] = useState(140);
  const tipText = activePoint ? formatValue(activePoint.value, hidden) : "";
  useLayoutEffect(() => {
    if (tipRef.current && tipRef.current.offsetWidth > 0) setTipWidth(tipRef.current.offsetWidth);
  }, [tipText, activePoint?.label, ready]);

  const px = activePoint ? s.x(activePoint.x) : 0;
  const py = activePoint ? s.y(activePoint.value) : 0;
  const svgHeight = plotHeight + TREND_LAYOUT.xAxisHeight;
  const grid = gridValues(yMax);

  const summary =
    ariaLabel ??
    (activePoint && current.length
      ? `${currentLabel}: ${hidden ? HIDDEN_ARIA_LABEL : formatValue(current[current.length - 1].value, false)} per ${current[current.length - 1].label}`
      : currentLabel);

  return (
    <figure className={cn("m-0", className)}>
      <div ref={wrapRef} className="relative w-full" style={{ height: svgHeight }}>
        <svg width={width} height={svgHeight} viewBox={`0 0 ${width} ${svgHeight}`} overflow="visible" role="img" aria-label={summary} className="absolute inset-0 block">
          {/* Area seri utama: fade vertikal accent 16% → 0 (moni-02). */}
          <defs>
            <linearGradient id={fadeId} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0" stopColor="var(--accent)" stopOpacity="0.16" />
              <stop offset="1" stopColor="var(--accent)" stopOpacity="0" />
            </linearGradient>
          </defs>
          {/* Grid horizontal: 0 solid border-strong, sisanya dashed 4 4. */}
          {grid.map((v, i) => {
            const y = Math.round(s.y(v)) + 0.5;
            return (
              <g key={i} data-grid={i}>
                <line
                  x1={s.plotLeft}
                  x2={s.plotRight}
                  y1={y}
                  y2={y}
                  stroke={i === 0 ? "var(--border-strong)" : "var(--grid)"}
                  strokeWidth={1}
                  strokeDasharray={i === 0 ? undefined : "4 4"}
                />
                {ready ? (
                  <text x={0} y={y} dominantBaseline="middle" fill="var(--text-3)" fontSize={12} fontWeight={500} className="num">
                    {formatAxis(v, hidden)}
                  </text>
                ) : null}
              </g>
            );
          })}

          {/* Label axis X */}
          {ticks.map((t) => (
            <text
              key={t.x}
              x={s.x(t.x)}
              y={plotHeight + 16}
              textAnchor="middle"
              fill="var(--text-3)"
              fontSize={12}
              fontWeight={500}
              className="num"
            >
              {t.label}
            </text>
          ))}

          {ready ? (
            <>
              {previous.length > 1 ? (
                <path data-series="previous" d={linePath(previous, s)} fill="none" stroke="var(--series-compare)" strokeWidth={1.5} strokeLinejoin="round" />
              ) : null}
              {current.length > 1 ? (
                <>
                  <path data-series="area" d={areaPath(current, s)} fill={`url(#${fadeId})`} stroke="none" />
                  <path data-series="current" d={linePath(current, s)} fill="none" stroke="var(--accent)" strokeWidth={2.5} strokeLinejoin="round" strokeLinecap="round" />
                </>
              ) : null}
              {activePoint ? (
                <g data-active-point="" aria-hidden="true">
                  <circle cx={px} cy={py} r={9} fill="var(--accent-soft)" />
                  <circle cx={px} cy={py} r={5.5} fill="#ffffff" />
                  <circle cx={px} cy={py} r={3.5} fill="var(--accent)" />
                </g>
              ) : null}
            </>
          ) : null}

          {effective === "loading" ? (
            <rect x={s.plotLeft} y={0} width={s.plotRight - s.plotLeft} height={plotHeight} rx={8} fill="var(--surface-sunken)" className="animate-skeleton" data-skeleton="" />
          ) : null}
        </svg>

        {ready && activePoint ? (
          <>
            <div
              ref={tipRef}
              data-tooltip=""
              className="pointer-events-none absolute flex h-7 items-center whitespace-nowrap rounded-sm bg-surface-inverse px-3 text-label"
              style={{ left: clampTooltipLeft(px, tipWidth, s.plotLeft, s.plotRight), top: tooltipTop(py) }}
              aria-hidden="true"
            >
              <span className="font-medium text-tooltip-date">{activePoint.label}</span>
              &nbsp;
              <span className="num font-extrabold text-white">{tipText}</span>
            </div>
            <div
              role="slider"
              tabIndex={0}
              aria-label={`Titik ${currentLabel}`}
              aria-valuemin={0}
              aria-valuemax={current.length - 1}
              aria-valuenow={activeIndex}
              aria-valuetext={`${activePoint.label}, ${hidden ? HIDDEN_ARIA_LABEL : tipText}`}
              className="absolute top-0 cursor-crosshair rounded-sm"
              style={{ left: s.plotLeft, width: s.plotRight - s.plotLeft, height: plotHeight, touchAction: "pan-y" }}
              onPointerDown={onPointerDown}
              onPointerMove={onPointerMove}
              onPointerUp={onPointerEnd}
              onPointerCancel={onPointerEnd}
              onKeyDown={onKeyDown}
            />
          </>
        ) : null}

        {effective === "empty" || effective === "error" ? (
          <div className="absolute flex flex-col items-center justify-center gap-1 text-center" style={{ left: s.plotLeft, right: 0, top: 0, height: plotHeight }}>
            <p className={cn("text-label", effective === "error" ? "font-semibold text-status-over" : "text-text-2")}>
              {effective === "empty" ? "Belum cukup data" : "Gagal memuat"}
            </p>
            {effective === "error" && onRetry ? (
              <Button variant="ghost" onClick={onRetry} className="bg-bg">
                Coba lagi
              </Button>
            ) : null}
          </div>
        ) : null}
      </div>

      {/* Legenda: titik 10 px + label, gap 16 px. */}
      <div className="flex h-7 items-center gap-4" data-legend="">
        <span className="inline-flex items-center gap-2 text-label font-bold text-text">
          <span className="h-2.5 w-2.5 rounded-pill bg-accent" aria-hidden="true" />
          {currentLabel}
        </span>
        {previousLabel ? (
          <span className="inline-flex items-center gap-2 text-label font-bold text-text-2">
            <span className="h-2.5 w-2.5 rounded-pill bg-series-compare" aria-hidden="true" />
            {previousLabel}
          </span>
        ) : null}
      </div>

      {ready ? (
        <table className="sr-only-table">
          <caption>{ariaLabel ?? currentLabel}</caption>
          <thead>
            <tr>
              <th scope="col">Titik</th>
              <th scope="col">{currentLabel}</th>
              {previousLabel ? <th scope="col">{previousLabel}</th> : null}
            </tr>
          </thead>
          <tbody>
            {(current.length >= previous.length ? current : previous).map((p, i) => (
              <tr key={p.x}>
                <th scope="row">{p.label}</th>
                <td>{current[i] ? (hidden ? HIDDEN_ARIA_LABEL : formatValue(current[i].value, false)) : "—"}</td>
                {previousLabel ? <td>{previous[i] ? (hidden ? HIDDEN_ARIA_LABEL : formatValue(previous[i].value, false)) : "—"}</td> : null}
              </tr>
            ))}
          </tbody>
        </table>
      ) : null}
    </figure>
  );
}
