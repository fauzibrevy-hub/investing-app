/**
 * Geometri DriverChart (docs/research/design.md §6 dan §8), murni agar bisa diuji: sumbu waktu dalam hari kalender,
 * sumbu kiri harga dan sumbu kanan driver dengan tick rapi, path seri, indeks relatif yang dirasionalkan ke 100,
 * garis level horizontal, penanda dividen dan split di sumbu X, pencarian titik terdekat untuk tooltip, dan baris
 * tabel `sr-only`. Nilai level datang dari payload apa adanya (sudah disesuaikan split oleh backend).
 */

/** `[tanggal YYYY-MM-DD, nilai]`, bentuk seri harga dan driver di payload. */
export type DatedPoint = readonly [string, number];

export const DRIVER_LAYOUT = {
  /** Lebar kolom label sumbu kiri (harga) dan kanan (driver). */
  axisWidth: 52,
  plotHeight: { compact: 180, full: 300 },
  /** Label tanggal di bawah plot, termasuk baris penanda dividen dan split. */
  xAxisHeight: 36,
  /** Jarak baris penanda dari dasar plot. */
  markerOffset: 6,
  /** Tinggi tick dividen (design.md §6: 6 px). */
  dividendTick: 6,
  defaultWidth: 720,
} as const;

const DAY_MS = 86_400_000;

/** Hari sejak 1970-01-01 dari tanggal `YYYY-MM-DD` (atau awal stempel waktu); `NaN` bila bukan tanggal yang ada. */
export function dayIndex(date: string): number {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(date);
  if (!m) return Number.NaN;
  const [y, mo, d] = [Number(m[1]), Number(m[2]) - 1, Number(m[3])];
  const ms = Date.UTC(y, mo, d);
  const back = new Date(ms);
  // `Date.UTC` menggulirkan 2026-13-45 ke tahun berikutnya; tanggal yang tidak ada ditolak.
  if (back.getUTCFullYear() !== y || back.getUTCMonth() !== mo || back.getUTCDate() !== d) return Number.NaN;
  return ms / DAY_MS;
}

/** Kebalikan `dayIndex`; kosong untuk hari yang bukan bilangan hingga. */
export function dateOfDay(day: number): string {
  if (!Number.isFinite(day)) return "";
  return new Date(Math.round(day) * DAY_MS).toISOString().slice(0, 10);
}

export interface TimeScale {
  from: number;
  to: number;
  left: number;
  right: number;
  x: (date: string) => number;
  xDay: (day: number) => number;
  /** Hari (pecahan) di bawah piksel X. */
  dayAt: (px: number) => number;
}

/** Skala X linear dalam hari kalender dari `from` ke `to` di antara `left` dan `right`. */
export function timeScale(from: string, to: string, left: number, right: number): TimeScale {
  const a = dayIndex(from);
  const b = dayIndex(to);
  const span = b - a || 1;
  const xDay = (day: number) => left + ((day - a) / span) * (right - left);
  return {
    from: a,
    to: b,
    left,
    right,
    xDay,
    x: (date) => xDay(dayIndex(date)),
    dayAt: (px) => a + ((px - left) / (right - left || 1)) * span,
  };
}

export interface NiceAxis {
  min: number;
  max: number;
  step: number;
  ticks: number[];
}

const NICE = [1, 2, 2.5, 5, 10];

function niceStep(raw: number): number {
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  return NICE.find((s) => s * magnitude >= raw - 1e-12)! * magnitude;
}

/** Membulatkan sisa pecahan floating point (0,1 + 0,2 di float = 0,3 lebih sedikit sisa → 0,3). */
const tidy = (n: number) => Number(n.toPrecision(12));

/**
 * Sumbu rapi yang memuat semua nilai hingga: langkah 1, 2, 2,5 atau 5 × 10^k dari rentang ÷ `count`, batas dibulatkan ke
 * kelipatan langkah. Seri datar diberi ruang ±5 % di sekitar nilainya. `null` bila tidak ada nilai hingga.
 */
export function niceAxis(values: readonly number[], count = 4): NiceAxis | null {
  const finite = values.filter(Number.isFinite);
  if (finite.length === 0) return null;
  let lo = Math.min(...finite);
  let hi = Math.max(...finite);
  if (hi === lo) {
    const pad = Math.abs(lo) * 0.05 || 0.5;
    lo -= pad;
    hi += pad;
  }
  const step = niceStep((hi - lo) / count);
  const min = tidy(Math.floor(tidy(lo / step)) * step);
  const max = tidy(Math.ceil(tidy(hi / step)) * step);
  const n = Math.round((max - min) / step);
  return { min, max, step, ticks: Array.from({ length: n + 1 }, (_, i) => tidy(min + i * step)) };
}

/** Skala Y: minimum sumbu di `bottom`, maksimum di `top`. */
export function valueScale(axis: Pick<NiceAxis, "min" | "max">, top: number, bottom: number): (v: number) => number {
  const span = axis.max - axis.min || 1;
  return (v) => bottom - ((v - axis.min) / span) * (bottom - top);
}

const r2 = (n: number) => Math.round(n * 100) / 100;

/** Path SVG `M … L …`; nilai tidak hingga memutus garis (sub-path baru), tidak pernah disambung melewatinya. */
export function linePath(points: readonly DatedPoint[], x: (date: string) => number, y: (v: number) => number): string {
  const parts: string[] = [];
  let pen = false;
  for (const [date, value] of points) {
    if (!Number.isFinite(value)) {
      pen = false;
      continue;
    }
    parts.push(`${pen ? "L" : "M"} ${r2(x(date))} ${r2(y(value))}`);
    pen = true;
  }
  return parts.join(" ");
}

const finite = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);

/**
 * Seri dirasionalkan sehingga nilai hingga pertama = 100. Tanpa basis yang bisa dipakai (tidak ada, 0 atau negatif),
 * kosong. Nilai yang bukan angka (null dari JSON) tetap celah (`NaN`), tidak pernah menjadi 0.
 */
export function rebaseTo100(points: readonly DatedPoint[]): DatedPoint[] {
  const base = points.find((p) => finite(p[1]))?.[1];
  if (base === undefined || base <= 0) return [];
  return points.map(([d, v]) => [d, finite(v) ? (v / base) * 100 : Number.NaN] as const);
}

/**
 * Indeks relatif pada sumbu harga: 100 diletakkan di penutupan pada hari pertama indeks (backend merebase di hari pertama
 * yang punya harga dan indeks, bisa setelah awal seri harga), sehingga garis indeks berangkat dari titik yang sama dengan
 * garis harga. Tanpa penutupan pada hari itu, kosong. Tooltip dan tabel tetap menulis nilai indeks (100 = awal).
 */
export function indexOnPriceAxis(points: readonly DatedPoint[], price: readonly DatedPoint[]): DatedPoint[] {
  const first = points.find((p) => finite(p[1]));
  if (!first) return [];
  const anchor = valueAsOf(price, dayIndex(first[0]))?.[1];
  if (!finite(anchor)) return [];
  return rebaseTo100(points).map(([d, v]) => [d, (v * anchor) / 100] as const);
}

export interface LevelLine {
  id: string;
  y: number;
  x1: number;
  x2: number;
}

/** Garis level horizontal selebar plot; nilai `level` dari payload (basis penutupan saat ini). */
export function levelLines(levels: ReadonlyArray<{ id: string; level: number }>, y: (v: number) => number, left: number, right: number): LevelLine[] {
  return levels.filter((l) => Number.isFinite(l.level)).map((l) => ({ id: l.id, y: y(l.level), x1: left, x2: right }));
}

export type ChartEvent =
  | { date: string; kind: "dividend"; amount: number }
  | { date: string; kind: "split"; ratio_num: number; ratio_den: number };

export type EventMarker = ChartEvent & { x: number };

/** Dividen dan split di dalam rentang, dengan posisi X-nya. */
export function eventMarkers(events: readonly ChartEvent[], scale: TimeScale): EventMarker[] {
  return events
    .filter((e) => {
      const d = dayIndex(e.date);
      return d >= scale.from && d <= scale.to;
    })
    .map((e) => ({ ...e, x: scale.x(e.date) }));
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des"];
const MONTH_STEPS = [1, 2, 3, 6, 12, 24, 60];
/** Batas bulan yang dipindai (lebih dari 10T terpanjang); penjaga terhadap rentang yang rusak. */
const MAX_MONTHS = 2_000;

export interface TimeTick {
  day: number;
  x: number;
  label: string;
}

/**
 * Tick X pada awal bulan, paling banyak `max`: langkah 1, 2, 3, 6 bulan atau 1, 2, 5 tahun, selaras Januari.
 * Label `Jan 26` untuk langkah di bawah setahun, `2026` selebihnya.
 */
export function timeTicks(scale: TimeScale, max = 6): TimeTick[] {
  if (!Number.isFinite(scale.from) || !Number.isFinite(scale.to)) return [];
  const start = new Date(scale.from * DAY_MS);
  const months: Array<{ y: number; m: number; day: number }> = [];
  let y = start.getUTCFullYear();
  let m = start.getUTCMonth();
  // Awal bulan pertama di dalam rentang (bulan berjalan bila rentang mulai tanggal 1).
  if (start.getUTCDate() !== 1) m += 1;
  for (;;) {
    if (m > 11) {
      m -= 12;
      y += 1;
    }
    const day = Date.UTC(y, m, 1) / DAY_MS;
    if (day > scale.to || months.length > MAX_MONTHS) break;
    months.push({ y, m, day });
    m += 1;
  }
  for (const step of MONTH_STEPS) {
    const picked = months.filter((t) => (t.y * 12 + t.m) % step === 0);
    if (picked.length <= max || step === MONTH_STEPS[MONTH_STEPS.length - 1]) {
      return picked.map((t) => ({
        day: t.day,
        x: scale.xDay(t.day),
        label: step >= 12 ? String(t.y) : `${MONTHS[t.m]} ${String(t.y).slice(2)}`,
      }));
    }
  }
  return [];
}

/** Indeks titik dengan tanggal terdekat ke `day` (seri terurut naik); di luar seri ke ujung terdekat. −1 bila kosong. */
export function nearestIndex(points: readonly DatedPoint[], day: number): number {
  if (points.length === 0) return -1;
  let lo = 0;
  let hi = points.length - 1;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (dayIndex(points[mid][0]) < day) lo = mid + 1;
    else hi = mid;
  }
  if (lo > 0 && Math.abs(dayIndex(points[lo - 1][0]) - day) <= Math.abs(dayIndex(points[lo][0]) - day)) return lo - 1;
  return lo;
}

/** Observasi terbaru pada atau sebelum `day` (driver bulanan dibaca per hari harga); `null` bila belum ada. */
export function valueAsOf(points: readonly DatedPoint[], day: number): DatedPoint | null {
  let lo = 0;
  let hi = points.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (dayIndex(points[mid][0]) <= day) {
      found = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  return found >= 0 ? points[found] : null;
}

export interface ChartTableRow {
  date: string;
  price: number;
  ma200: number | null;
  driver: number | null;
  index: number | null;
}

/**
 * Baris tabel `sr-only`: satu per penutupan yang digambar. MA200 dan indeks relatif dibaca pada tanggal yang sama
 * (seri harian), driver per tanggal itu (observasi terbaru pada atau sebelumnya).
 */
export function tableRows(series: {
  price: readonly DatedPoint[];
  ma200: readonly DatedPoint[];
  driver: readonly DatedPoint[];
  index: readonly DatedPoint[];
}): ChartTableRow[] {
  const ma = new Map(series.ma200);
  const idx = new Map(series.index);
  return series.price.map(([date, price]) => ({
    date,
    price,
    ma200: ma.get(date) ?? null,
    driver: valueAsOf(series.driver, dayIndex(date))?.[1] ?? null,
    index: idx.get(date) ?? null,
  }));
}
