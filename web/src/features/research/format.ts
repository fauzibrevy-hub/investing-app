/**
 * Format angka Riset (design.md §4, §7). Satuan hanya di header dan judul, tidak pernah di sel:
 * sel tidak berisi `Rp`, `%`, atau `×`. Persen: koma desimal, 1 desimal. Kelipatan: 1 desimal.
 * Negatif memakai U+2212. Tidak ada formatter yang menerima `null`: pemanggil menampilkan gap
 * dengan aksinya, bukan strip.
 */

/** U+2212. */
export const MINUS = "−";

const formats = new Map<number, Intl.NumberFormat>();

function formatter(decimals: number): Intl.NumberFormat {
  let f = formats.get(decimals);
  if (!f) {
    f = new Intl.NumberFormat("id-ID", { minimumFractionDigits: decimals, maximumFractionDigits: decimals, useGrouping: true });
    formats.set(decimals, f);
  }
  return f;
}

/** Angka berpemisah ribuan titik dan koma desimal, negatif dengan U+2212, tanpa satuan. */
export function formatValue(value: number, decimals = 0): string {
  const body = formatter(decimals).format(Math.abs(value));
  // Nilai yang dibulatkan ke nol tidak diberi tanda.
  const zero = Number(body.replace(/\./g, "").replace(",", ".")) === 0;
  return `${value < 0 && !zero ? MINUS : ""}${body}`;
}

const exactFormat = new Intl.NumberFormat("id-ID", { minimumFractionDigits: 0, maximumFractionDigits: 6, useGrouping: true });

/** Nilai persis sebagaimana dilaporkan: sampai 6 desimal tanpa nol di belakang. Untuk asal angka (drawer, operand). */
export function formatExact(value: number): string {
  const body = exactFormat.format(Math.abs(value));
  return `${value < 0 && Number(body.replace(/\./g, "").replace(",", ".")) !== 0 ? MINUS : ""}${body}`;
}

/** Nilai persen (28,3 → `28,3`), 1 desimal. Tanda `%` ada di header. */
export function formatPct(value: number): string {
  return formatValue(value, 1);
}

/** Kelipatan (12,34 → `12,3`), 1 desimal. Tanda `×` ada di header. */
export function formatMultiple(value: number): string {
  return formatValue(value, 1);
}

/** Selisih bertanda, 1 desimal: `+2,1`, `−1,4`; nol tanpa tanda. Untuk YoY. */
export function formatDelta(value: number): string {
  const body = formatValue(value, 1);
  return value > 0 && body !== "0,0" ? `+${body}` : body;
}

// ---- skala per tabel ----

export interface Scale {
  /** Pembagi nilai mentah. */
  divisor: number;
  /** Akhiran header: `(RP MILIAR)`, `(USD JUTA)`. */
  header: string;
  /** Kata skala untuk judul tabel: `miliar`. Kosong untuk skala satuan. */
  word: string;
}

const STEPS: Array<{ divisor: number; word: string }> = [
  { divisor: 1, word: "" },
  { divisor: 1e3, word: "ribu" },
  { divisor: 1e6, word: "juta" },
  { divisor: 1e9, word: "miliar" },
  { divisor: 1e12, word: "triliun" },
];

/** Mata uang pelaporan → kata di header. */
export function currencyWord(currency: string): string {
  const upper = currency.toUpperCase();
  return upper === "IDR" ? "RP" : upper;
}

function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

/**
 * Skala terbesar yang membuat median |nilai| kolom tetap bertiga digit atau lebih (≥ 100), sehingga
 * median selalu 3 sampai 6 digit (langkah skala 10³). Tanpa data, atau median di bawah 100 pada skala
 * satuan, skala satuan. Mengembalikan akhiran header ala `(RP MILIAR)`.
 */
export function chooseScale(values: Array<number | null | undefined>, currency: string): Scale {
  const present = values.filter((v): v is number => typeof v === "number" && Number.isFinite(v)).map(Math.abs);
  const mid = present.length ? median(present) : 0;
  let chosen = STEPS[0];
  for (const step of STEPS) if (mid / step.divisor >= 100) chosen = step;
  const word = currencyWord(currency);
  return {
    divisor: chosen.divisor,
    word: chosen.word,
    header: `(${chosen.word ? `${word} ${chosen.word.toUpperCase()}` : word})`,
  };
}

/**
 * Nilai mentah pada skala tabel. Nilai bukan nol yang akan dibulatkan menjadi 0 diberi desimal (atau `<0,01`),
 * sehingga `0` selalu berarti nol sungguhan.
 */
export function formatScaled(value: number, scale: Scale, decimals = 0): string {
  const scaled = value / scale.divisor;
  const abs = Math.abs(scaled);
  if (decimals === 0 && abs > 0 && abs < 0.5) {
    if (abs < 0.005) return `${scaled < 0 ? MINUS : ""}<0,01`;
    return formatValue(scaled, abs < 0.05 ? 2 : 1);
  }
  return formatValue(scaled, decimals);
}

/** Judul tabel yang menyebut skala: `Laba rugi · dalam miliar` (design.md §4). */
export function scaleTitle(title: string, scale: Scale): string {
  return scale.word ? `${title} · dalam ${scale.word}` : title;
}

// ---- header satuan ----

const UNIT_HEADERS: Record<string, string> = {
  "%": "(%)",
  pp: "(PP)",
  x: "(×)",
  "×": "(×)",
};

/** Satuan metrik pack → akhiran header, huruf besar: `%` → `(%)`, `USD/t` → `(USD/T)`. `ccy` perlu `currency`. */
export function unitHeader(unit: string, currency = "IDR"): string {
  if (unit === "ccy") return `(${currencyWord(currency)})`;
  const known = UNIT_HEADERS[unit];
  if (known) return known;
  const parts = unit.split(" ");
  const head = parts[0].toUpperCase() === "IDR" ? "RP" : parts[0].toUpperCase();
  return `(${[head, ...parts.slice(1).map((p) => p.toUpperCase())].join(" ")})`;
}

// ---- umur, periode, tanggal ----

/** Umur dalam hari: `hari ini`, `3 hari`, `2 bulan`, `1 tahun`. */
export function formatAge(days: number): string {
  if (!Number.isFinite(days)) return "";
  const d = Math.max(0, Math.floor(days));
  if (d === 0) return "hari ini";
  if (d < 45) return `${d} hari`;
  if (d < 365) return `${Math.round(d / 30)} bulan`;
  return `${Math.floor(d / 365)} tahun`;
}

/** `2026Q2` → `Q2 26`; `2026` (tahunan) → `FY 26`. Label yang tidak dikenal dikembalikan apa adanya. */
export function periodLabel(label: string): string {
  const q = /^(\d{4})Q([1-4])$/.exec(label);
  if (q) return `Q${q[2]} ${q[1].slice(2)}`;
  const fy = /^(\d{4})$/.exec(label);
  if (fy) return `FY ${fy[1].slice(2)}`;
  return label;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des"];

const WIB_OFFSET_MS = 7 * 3600 * 1000;

/**
 * Hari kalender untuk teks tanggal. `YYYY-MM-DD` dipakai apa adanya; stempel waktu RFC 3339 dibaca sebagai hari WIB
 * (UTC+7), hari yang dipakai backend untuk jurnal dan kalibrasi, bukan hari UTC.
 */
function calendarDay(date: string): string {
  if (/^\d{4}-\d{2}-\d{2}$/.test(date)) return date;
  const t = Date.parse(date);
  return Number.isNaN(t) || !/^\d{4}-\d{2}-\d{2}T/.test(date) ? date : new Date(t + WIB_OFFSET_MS).toISOString().slice(0, 10);
}

/** `2026-10-24` (atau RFC 3339, hari WIB) → `24 Okt 2026`. Teks yang bukan tanggal dikembalikan apa adanya. */
export function formatDate(date: string): string {
  const day = calendarDay(date);
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(day);
  if (!m) return date;
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const valid = new Date(Date.UTC(y, mo - 1, d)).getUTCDate() === d && mo >= 1 && mo <= 12;
  return valid ? `${d} ${MONTHS[mo - 1]} ${m[1]}` : date;
}

/** `2026-10-24` → `24 Okt`. */
export function formatDay(date: string): string {
  const full = formatDate(date);
  return full === date ? date : full.slice(0, full.lastIndexOf(" "));
}
