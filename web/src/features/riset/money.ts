import { MINUS, formatValue } from "../research/format";

const PREFIX: Record<string, string> = { IDR: "Rp", USD: "US$" };

/** Skala singkat: triliun `T`, miliar `M`, juta `jt`; di bawah sejuta ditulis penuh. */
const STEPS: Array<[number, string]> = [
  [1e12, "T"],
  [1e9, "M"],
  [1e6, "jt"],
];

/**
 * Uang ringkas untuk angka yang tidak boleh terbungkus di layar 393 px (design.md §6): `Rp1,23 T`, `US$95,2 jt`.
 * Tiga angka penting: dua desimal di bawah 10, satu di bawah 100, selebihnya bulat. Mata uang lain ditulis kodenya.
 * Hanya untuk kartu dan kalimat; sel tabel memakai skala tabel dan satuan di judul baris.
 * Mata uang `null` (laporan belum termuat atau gagal) tidak ditebak Rp: angkanya tanpa awalan.
 */
export function formatCompactMoney(value: number, currency: string | null): string {
  const code = currency?.toUpperCase() ?? null;
  const prefix = code === null ? "" : (PREFIX[code] ?? `${code} `);
  const abs = Math.abs(value);
  const step = STEPS.find(([size]) => abs >= size);
  let body: string;
  if (step) {
    const scaled = abs / step[0];
    body = `${formatValue(scaled, scaled < 10 ? 2 : scaled < 100 ? 1 : 0)} ${step[1]}`;
  } else {
    body = formatValue(abs, 0);
  }
  const zero = body === "0";
  return `${value < 0 && !zero ? MINUS : ""}${prefix}${body}`;
}
