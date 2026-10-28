/** Geometri TrendChart (design.md §5.2), murni agar bisa diuji. */

export interface TrendPoint {
  /** Posisi X numerik, mis. tanggal 1..31 atau indeks kuartal. */
  x: number;
  value: number;
  /** Label tanggal di tooltip/tabel, mis. "24 Okt". */
  label: string;
}

export const TREND_LAYOUT = {
  /** Lebar kolom label axis Y. */
  yLabelWidth: 36,
  plotHeight: { mobile: 130, desktop: 240 },
  xAxisHeight: 24,
  legendHeight: 28,
  /** Lebar default = kolom konten mobile 393 − 2 × 18. */
  defaultWidth: 357,
  tooltipHeight: 28,
  tooltipOffset: 12,
} as const;

const NICE_STEPS = [1, 1.5, 2, 2.5, 3, 4, 5, 6, 7.5, 10];

/**
 * Maks "rapi": 4 interval grid dengan langkah 1/1,5/2/2,5/3/4/5/6/7,5 × 10^k. 9,1 jt → 10 jt (langkah 2,5 jt).
 * Langkah rapat agar garis mengisi plot seperti moni; 21,8 jt → 24 jt, bukan 40 jt.
 */
export function niceMax(max: number): number {
  if (!Number.isFinite(max) || max <= 0) return 1;
  const raw = max / 4;
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  const step = NICE_STEPS.find((s) => s * magnitude >= raw - 1e-9)! * magnitude;
  return step * 4;
}

/** 5 garis grid: 0, 25, 50, 75, 100% dari maks rapi. */
export function gridValues(max: number): number[] {
  return [0, 0.25, 0.5, 0.75, 1].map((f) => f * max);
}

export interface Scales {
  x: (x: number) => number;
  y: (v: number) => number;
  plotLeft: number;
  plotRight: number;
  plotTop: number;
  plotBottom: number;
}

export function makeScales(opts: { width: number; plotHeight: number; xMin: number; xMax: number; yMax: number }): Scales {
  const plotLeft = TREND_LAYOUT.yLabelWidth;
  const plotRight = opts.width;
  const plotTop = 0;
  const plotBottom = opts.plotHeight;
  const xSpan = opts.xMax - opts.xMin || 1;
  return {
    x: (x) => plotLeft + ((x - opts.xMin) / xSpan) * (plotRight - plotLeft),
    y: (v) => plotBottom - (Math.max(0, v) / opts.yMax) * (plotBottom - plotTop),
    plotLeft,
    plotRight,
    plotTop,
    plotBottom,
  };
}

const r2 = (n: number) => Math.round(n * 100) / 100;

export function linePath(points: TrendPoint[], s: Scales): string {
  return points.map((p, i) => `${i === 0 ? "M" : "L"} ${r2(s.x(p.x))} ${r2(s.y(p.value))}`).join(" ");
}

/** Area solid di bawah seri utama sampai garis 0. */
export function areaPath(points: TrendPoint[], s: Scales): string {
  if (points.length === 0) return "";
  const first = points[0];
  const last = points[points.length - 1];
  return `${linePath(points, s)} L ${r2(s.x(last.x))} ${r2(s.plotBottom)} L ${r2(s.x(first.x))} ${r2(s.plotBottom)} Z`;
}

/** Indeks titik dengan X piksel terdekat (snap). */
export function nearestIndex(points: TrendPoint[], s: Scales, px: number): number {
  let best = 0;
  let bestDist = Infinity;
  points.forEach((p, i) => {
    const d = Math.abs(s.x(p.x) - px);
    if (d < bestDist) {
      bestDist = d;
      best = i;
    }
  });
  return best;
}

/** Posisi kiri tooltip: center di titik, di-clamp ke [minX, maxX − lebar]. */
export function clampTooltipLeft(centerX: number, width: number, minX: number, maxX: number): number {
  const left = centerX - width / 2;
  return Math.max(minX, Math.min(left, maxX - width));
}

/**
 * Posisi atas tooltip: 12 px di atas titik. Jika itu keluar dari atas plot, tooltip dibalik ke
 * 12 px di bawah titik — clamp biasa akan menutupi titik aktif.
 */
export function tooltipTop(pointY: number, plotTop = 0): number {
  const above = pointY - TREND_LAYOUT.tooltipOffset - TREND_LAYOUT.tooltipHeight;
  return above >= plotTop ? above : pointY + TREND_LAYOUT.tooltipOffset;
}

/** Tick X default: bulanan 1, 5, 10, …, 30; selain itu label tiap titik seri yang dibagi rata (maks 7). */
export function defaultXTicks(xMin: number, xMax: number, points: TrendPoint[]): { x: number; label: string }[] {
  if (xMin === 1 && xMax >= 28 && xMax <= 31) {
    return [1, 5, 10, 15, 20, 25, 30].map((d) => ({ x: d, label: String(d) }));
  }
  if (points.length <= 7) return points.map((p) => ({ x: p.x, label: p.label }));
  const step = (points.length - 1) / 6;
  return Array.from({ length: 7 }, (_, i) => points[Math.round(i * step)]).map((p) => ({ x: p.x, label: p.label }));
}
