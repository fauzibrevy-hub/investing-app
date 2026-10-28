import { chooseScale, formatMultiple, formatPct, formatScaled, formatValue, unitHeader, type Scale } from "../format";

/**
 * Format nilai metrik paket (design.md §7): tanpa satuan di sel. `ccy` memakai skala baris (median 3 sampai 6 digit),
 * `%` dan `pp` 1 desimal, kelipatan `x` 1 desimal, satuan lain (Mt, USD/t, tahun) presisi menurut besarnya.
 */
export function formatMetric(value: number, unit: string, scale?: Scale | null): string {
  if (unit === "ccy") return scale ? formatScaled(value, scale) : formatValue(value);
  if (unit === "%" || unit === "pp") return formatPct(value);
  if (unit === "x" || unit === "×") return formatMultiple(value);
  const abs = Math.abs(value);
  return formatValue(value, Number.isInteger(value) || abs >= 100 ? 0 : abs >= 10 ? 1 : 2);
}

/** Skala baris untuk metrik `ccy` dari nilai yang tampil di baris itu; `null` untuk satuan lain. */
export function metricScale(unit: string, values: ReadonlyArray<number | null | undefined>, currency: string | null): Scale | null {
  return unit === "ccy" ? chooseScale([...values], currency ?? "IDR") : null;
}

/**
 * Akhiran satuan untuk judul atau header baris: `(USD JUTA)`, `(%)`, `(MT)`. Mata uang yang belum diketahui
 * ditulis "mata uang laporan", tidak pernah ditebak `RP`.
 */
export function metricUnitHeader(unit: string, currency: string | null, scale: Scale | null): string {
  if (unit !== "ccy") return unitHeader(unit);
  if (!currency) return scale?.word ? `(MATA UANG LAPORAN, ${scale.word.toUpperCase()})` : "(MATA UANG LAPORAN)";
  return scale ? scale.header : unitHeader("ccy", currency);
}
