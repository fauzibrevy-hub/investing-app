/**
 * Satu-satunya formatter uang/persen di UI (design.md §6). Komponen tidak boleh memformat angka sendiri.
 */

export const HIDDEN_FULL = "Rp•••";
export const HIDDEN_BARE = "•••";
/** U+2212, dipakai untuk semua tanda minus (§6.6). */
export const MINUS = "−";

const intFormat = new Intl.NumberFormat("id-ID", { maximumFractionDigits: 0, useGrouping: true });
const dec2Format = new Intl.NumberFormat("id-ID", { maximumFractionDigits: 2, useGrouping: true });
const dec1Format = new Intl.NumberFormat("id-ID", { maximumFractionDigits: 1, useGrouping: true });

export interface RupiahOptions {
  /** Format singkat rb/jt/M (§6.2). Default: penuh (§6.1). */
  short?: boolean;
  /** Tanpa prefiks "Rp": axis chart (§6.2) dan sel tabel (§6.4). */
  bare?: boolean;
  /** Selalu tampilkan tanda (+ / −), mis. P/L (§6.6). Tanpa ini, nilai negatif tetap diberi "−". */
  signed?: boolean;
  /** Mode sembunyi nominal (§6.8). */
  hidden?: boolean;
}

function signOf(value: number, signed: boolean): string {
  if (value < 0) return MINUS;
  if (signed && value > 0) return "+";
  return "";
}

/** Bagian angka format singkat untuk |value|, tanpa "Rp". */
function shortMagnitude(abs: number): string {
  if (abs < 1_000) return intFormat.format(Math.round(abs));
  const thousands = Math.round(abs / 1_000);
  if (thousands < 1_000) return `${intFormat.format(thousands)} rb`;
  const millions = Math.round(abs / 10_000) / 100; // 2 desimal
  if (millions < 1_000) return `${dec2Format.format(millions)} jt`;
  const billions = Math.round(abs / 10_000_000) / 100;
  return `${dec2Format.format(billions)} M`;
}

/**
 * Format rupiah.
 * - penuh: `Rp6.480.000`
 * - singkat: `Rp190 rb`, `Rp6,48 jt`, `Rp8 jt`, `Rp1,2 M`
 * - bare: tanpa `Rp` (`6.480.000`, `2,5 jt`, `0`)
 */
export function formatRupiah(value: number, options: RupiahOptions = {}): string {
  const { short = false, bare = false, signed = false, hidden = false } = options;
  if (hidden) return short || bare ? HIDDEN_BARE : HIDDEN_FULL;
  if (!Number.isFinite(value)) return "—";

  const rounded = short ? value : Math.round(value);
  const abs = Math.abs(rounded);
  const magnitude = short ? shortMagnitude(abs) : intFormat.format(abs);
  // Nilai yang dibulatkan ke 0 tidak diberi tanda.
  const sign = magnitude === "0" ? "" : signOf(rounded, signed);
  return `${sign}${bare ? "" : "Rp"}${magnitude}`;
}

export interface PercentOptions {
  signed?: boolean;
}

/** Persen dari nilai persen (28.3 → `28,3%`). Koma desimal, maks 1 desimal, tanpa spasi (§6.7). */
export function formatPercent(percent: number, options: PercentOptions = {}): string {
  if (!Number.isFinite(percent)) return "—";
  const rounded = Math.round(percent * 10) / 10;
  const body = dec1Format.format(Math.abs(rounded));
  const sign = rounded === 0 ? "" : signOf(rounded, options.signed ?? false);
  return `${sign}${body}%`;
}

/** Selisih poin persen, selalu bertanda: `+2,1 pp`, `−1,4 pp` (design.md §3.3). */
export function formatPp(points: number): string {
  if (!Number.isFinite(points)) return "—";
  const rounded = Math.round(points * 10) / 10;
  const sign = rounded === 0 ? "" : signOf(rounded, true);
  return `${sign}${dec1Format.format(Math.abs(rounded))} pp`;
}
