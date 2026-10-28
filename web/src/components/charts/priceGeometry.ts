/** Geometri `PriceChart`: skala, polyline, dan penempatan label garis acuan tanpa tumpang tindih. */

export type Point = readonly [string, number];

export interface PriceChartBox {
  width: number;
  height: number;
  left: number;
  right: number;
  top: number;
  bottom: number;
}

export const BOX: PriceChartBox = { width: 357, height: 232, left: 44, right: 8, top: 10, bottom: 26 };

export interface ChartInput {
  closes: Point[];
  ma200: Point[];
  band: { low: number; high: number } | null;
  refs: Array<{ id: string; value: number }>;
}

export interface Scale {
  min: number;
  max: number;
  x: (i: number, n: number) => number;
  y: (v: number) => number;
  ticks: number[];
}

export function niceTicks(min: number, max: number, count = 4): number[] {
  const step = (max - min) / (count - 1);
  return Array.from({ length: count }, (_, i) => min + step * i);
}

export function buildScale(input: ChartInput, box: PriceChartBox = BOX): Scale | null {
  const values = [
    ...input.closes.map((p) => p[1]),
    ...input.ma200.map((p) => p[1]),
    ...input.refs.map((r) => r.value),
    ...(input.band ? [input.band.low, input.band.high] : []),
  ].filter((v) => Number.isFinite(v));
  if (input.closes.length === 0 || values.length === 0) return null;
  const lo = Math.min(...values);
  const hi = Math.max(...values);
  const pad = (hi - lo) * 0.06 || hi * 0.05 || 1;
  const min = Math.max(0, lo - pad);
  const max = hi + pad;
  const plotW = box.width - box.left - box.right;
  const plotH = box.height - box.top - box.bottom;
  return {
    min,
    max,
    x: (i, n) => box.left + (n <= 1 ? plotW : (i / (n - 1)) * plotW),
    y: (v) => box.top + (1 - (v - min) / (max - min)) * plotH,
    ticks: niceTicks(min, max),
  };
}

/** Polyline untuk seri bertanggal; sumbu x memakai rentang tanggal `closes` (hari kalender). */
export function polyline(series: Point[], dates: string[], scale: Scale, box: PriceChartBox = BOX): string {
  if (dates.length === 0) return "";
  const t0 = Date.parse(dates[0]);
  const t1 = Date.parse(dates[dates.length - 1]);
  const span = t1 - t0 || 1;
  const plotW = box.width - box.left - box.right;
  return series
    .filter((p) => Date.parse(p[0]) >= t0)
    .map((p) => {
      const x = box.left + ((Date.parse(p[0]) - t0) / span) * plotW;
      return `${x.toFixed(1)},${scale.y(p[1]).toFixed(1)}`;
    })
    .join(" ");
}

/**
 * De-collision label garis acuan (plan F6, Acceptance M4-3).
 * Nilai y yang tumpang tindih dipisahkan minimal `gap` px (>= tinggi label)
 * dan seluruh label tetap berada di dalam batas plot [lo, hi].
 *
 * Bila tidak muat di plot (kasus degenerasi), label dengan prioritas lebih rendah
 * dibuang (null) agar jarak antar-label tidak pernah dipaksa lebih kecil dari tinggi label.
 */
export function decollideLabels(
  ys: number[],
  gap: number,
  lo: number,
  hi: number,
): Array<number | null> {
  if (ys.length === 0) return [];
  if (hi < lo) return ys.map(() => null);

  const out = new Array<number | null>(ys.length).fill(null);
  const maxFit = Math.floor((hi - lo) / gap) + 1;

  // Pertahankan label berprioritas tinggi hingga batas `maxFit`
  const keptIndices = Array.from({ length: Math.min(ys.length, maxFit) }, (_, i) => i);
  if (keptIndices.length === 0) return out;
  if (keptIndices.length === 1) {
    const idx = keptIndices[0];
    out[idx] = Math.min(Math.max(ys[idx], lo), hi);
    return out;
  }

  const order = keptIndices
    .map((i) => ({ y: ys[i], i }))
    .sort((a, b) => a.y - b.y);

  // 1. Dorong maju agar jarak antar label berturut-turut minimal `gap`
  for (let k = 0; k < order.length; k++) {
    const floor = k === 0 ? lo : order[k - 1].y + gap;
    order[k].y = Math.max(order[k].y, floor);
  }

  // 2. Dorong mundur bila ada yang melebihi batas atas `hi`
  for (let k = order.length - 1; k >= 0; k--) {
    const ceil = k === order.length - 1 ? hi : order[k + 1].y - gap;
    order[k].y = Math.min(order[k].y, ceil);
  }

  // 3. Forward pass kedua untuk menjamin tidak ada yang jatuh di bawah `lo`
  for (let k = 0; k < order.length; k++) {
    const floor = k === 0 ? lo : order[k - 1].y + gap;
    order[k].y = Math.max(order[k].y, floor);
  }

  for (const o of order) {
    out[o.i] = o.y;
  }

  return out;
}
