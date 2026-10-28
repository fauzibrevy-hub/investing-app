import { periodLabel } from "../format";

const MISSING = /^(\S+) tidak ada untuk (.+)$/;
const squash = (s: string) => s.replace(/[_\s]/g, "").toLowerCase();

/**
 * Alasan celah yang dibaca pemilik. Backend menulis `{ident} tidak ada untuk {periode} ({tampilan})` dengan `ident` dari
 * rumus (`GrossProfit`, `Production`); di UI nama itu diganti label metrik dari payload. Bila `ident` memang metrik itu
 * sendiri, labelnya dipakai langsung; bila bahan lain dari rumusnya, kalimatnya menyebut metrik itu sebagai pemilik masukan,
 * bukan nama internal. Alasan lain (pembagi nol, satuan berbeda) tidak berubah.
 */
export function gapReasonText(reason: string, metric: { id: string; label: string }): string {
  const found = MISSING.exec(reason);
  if (!found) return reason;
  const [, ident, raw] = found;
  const rest = raw.replace(/\d{4}Q[1-4]/g, periodLabel);
  return squash(ident) === squash(metric.id) ? `${metric.label} tidak ada untuk ${rest}` : `Masukan ${metric.label} tidak ada untuk ${rest}`;
}

/** Kurs tidak punya aksi pemilik: driver `usd_idr` diambil penjadwal tiap sepuluh menit (plan.md §5), bukan diisi tangan. */
export const FX_REASON = "Kurs USD/IDR belum tersedia; diambil otomatis pada pembaruan berikutnya";

/** Alasan dari backend ditulis huruf kecil di awal; di layar menjadi kalimat, dan alasan kurs menyebut langkah berikutnya. */
export function ownerReason(reason: string): string {
  const text = reason.charAt(0).toUpperCase() + reason.slice(1);
  return text.startsWith("Kurs USD/IDR belum tersedia") ? FX_REASON : text;
}

/** Status driver di layar: `TBD` adalah penanda internal sumber yang belum dipilih, bukan kata untuk pemilik. */
export const driverStatusText = (status: string) => (status === "TBD" ? "belum bersumber" : status);
