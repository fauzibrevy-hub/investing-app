/**
 * Geometri DonutChart (design.md §5.1), dipisah dari komponen agar bisa diuji murni.
 * Sudut memakai "derajat jam": 0° = jam 12, bertambah searah jarum jam.
 */
import { catForRank, CAT_OTHER, MAX_RANKED, type CatIndex } from "../../lib/categories";

export interface DonutDatum {
  label: string;
  value: number;
}

export interface DonutSegment {
  label: string;
  value: number;
  /** Porsi 0..1 dari total. */
  share: number;
  cat: CatIndex;
  /** Label asli yang digabung ke "Lainnya". */
  merged: string[];
}

export const OTHER_LABEL = "Lainnya";
/** Segmen < 1.5% total digabung ke "Lainnya". */
export const MIN_SHARE = 0.015;

export const DONUT_SIZES = {
  mobile: { diameter: 164, thickness: 22 },
  desktop: { diameter: 220, thickness: 28 },
} as const;
export const GAP_PX = 2;
/** Hit area diperlebar 8 px ke luar ring. */
export const HIT_EXTRA = 8;

/**
 * Urutkan terbesar → terkecil, gabung segmen < 1.5% dan peringkat > 7 ke "Lainnya" (`cat-8`).
 * Nilai ≤ 0 dibuang.
 */
export function prepareSegments(data: DonutDatum[]): DonutSegment[] {
  const positive = data.filter((d) => Number.isFinite(d.value) && d.value > 0);
  const total = positive.reduce((s, d) => s + d.value, 0);
  if (total <= 0) return [];

  const sorted = [...positive].sort((a, b) => b.value - a.value);
  const kept: DonutDatum[] = [];
  const other: DonutDatum[] = [];
  for (const d of sorted) {
    if (d.label === OTHER_LABEL || d.value / total < MIN_SHARE || kept.length >= MAX_RANKED) other.push(d);
    else kept.push(d);
  }

  const segments: DonutSegment[] = kept.map((d, rank) => ({
    label: d.label,
    value: d.value,
    share: d.value / total,
    cat: catForRank(rank),
    merged: [],
  }));
  if (other.length > 0) {
    const value = other.reduce((s, d) => s + d.value, 0);
    segments.push({ label: OTHER_LABEL, value, share: value / total, cat: CAT_OTHER, merged: other.map((d) => d.label) });
  }
  return segments;
}

export interface DonutArc {
  /** Sudut segmen penuh (tanpa celah), derajat jam. */
  start: number;
  end: number;
  /** Sudut yang digambar setelah dikurangi celah setengah di tiap sisi. */
  drawStart: number;
  drawEnd: number;
}

export interface RingGeometry {
  diameter: number;
  thickness: number;
  /** Jari-jari tengah ring (posisi stroke). */
  rMid: number;
  /** Sudut celah dalam derajat = (2 px / r_tengah) rad. */
  gapDeg: number;
}

export function ringGeometry(diameter: number, thickness: number, gapPx = GAP_PX): RingGeometry {
  const rMid = diameter / 2 - thickness / 2;
  return { diameter, thickness, rMid, gapDeg: ((gapPx / rMid) * 180) / Math.PI };
}

export function computeArcs(segments: Pick<DonutSegment, "share">[], ring: RingGeometry): DonutArc[] {
  // Satu segmen = ring penuh tanpa celah.
  const gap = segments.length > 1 ? ring.gapDeg : 0;
  let cursor = 0;
  return segments.map((s) => {
    const start = cursor;
    const end = cursor + s.share * 360;
    cursor = end;
    const half = Math.min(gap / 2, (end - start) / 2);
    return { start, end, drawStart: start + half, drawEnd: end - half };
  });
}

/** Titik pada lingkaran untuk derajat jam (0 = atas, searah jarum jam). */
export function polar(cx: number, cy: number, r: number, deg: number): [number, number] {
  const rad = (deg * Math.PI) / 180;
  return [cx + r * Math.sin(rad), cy - r * Math.cos(rad)];
}

/** Path busur (untuk di-stroke) dari `startDeg` ke `endDeg` searah jarum jam. */
export function arcPath(cx: number, cy: number, r: number, startDeg: number, endDeg: number): string {
  const sweep = endDeg - startDeg;
  if (sweep <= 0) return "";
  if (sweep >= 359.999) {
    // Lingkaran penuh: dua setengah busur.
    const [x0, y0] = polar(cx, cy, r, 0);
    const [x1, y1] = polar(cx, cy, r, 180);
    return `M ${x0} ${y0} A ${r} ${r} 0 1 1 ${x1} ${y1} A ${r} ${r} 0 1 1 ${x0} ${y0}`;
  }
  const [x0, y0] = polar(cx, cy, r, startDeg);
  const [x1, y1] = polar(cx, cy, r, endDeg);
  const large = sweep > 180 ? 1 : 0;
  return `M ${round(x0)} ${round(y0)} A ${r} ${r} 0 ${large} 1 ${round(x1)} ${round(y1)}`;
}

const round = (n: number) => Math.round(n * 1000) / 1000;

/**
 * Label tengah 3 baris, center vertikal: tinggi baris 20 (`type-label`), 28 (`type-donut-total`),
 * 18 (`type-caption`), jarak 4 px antar baris → total 74 px.
 * Mengembalikan `top` tiap baris relatif ke pusat (negatif = di atas pusat).
 */
export const CENTER_LINES = [20, 28, 18] as const;
export const CENTER_LINE_GAP = 4;

export function centerLabelLayout(): { tops: [number, number, number]; height: number } {
  const height = CENTER_LINES.reduce((s, h) => s + h, 0) + CENTER_LINE_GAP * (CENTER_LINES.length - 1);
  let y = -height / 2;
  const tops = CENTER_LINES.map((h) => {
    const top = y;
    y += h + CENTER_LINE_GAP;
    return top;
  }) as [number, number, number];
  return { tops, height };
}

/** Lebar maks teks tengah = diameter dalam − 16 px (104 px di mobile). */
export function centerMaxWidth(diameter: number, thickness: number): number {
  return diameter - 2 * thickness - 16;
}
