import { chooseScale, currencyWord, type Scale } from "../research/format";
import type { CompareResponse } from "../research/types";
import { unitCaption } from "./lastQuarter";

/** Pelapor rupiah selalu miliar: satu skala di semua baris uang supaya `Rp M` dan `Rp T` tidak bercampur (N8). */
const RP_MILIAR: Scale = { divisor: 1e9, word: "miliar", header: "(RP MILIAR)" };

const present = (v: number | null): v is number => typeof v === "number" && Number.isFinite(v);

/**
 * Satu skala uang per kolom emiten: pelapor rupiah (atau mata uang belum diketahui, seperti sebelumnya) memakai miliar;
 * mata uang lain memilih skalanya sendiri dari seluruh nilai uang kolom itu, jadi `USD rb` tidak bercampur dengan
 * `USD jt` antar baris. Kolom tanpa nilai uang memakai skala dasarnya.
 */
export function columnScales(data: Pick<CompareResponse, "tickers" | "rows">): Map<string, { scale: Scale; currency: string }> {
  const out = new Map<string, { scale: Scale; currency: string }>();
  data.tickers.forEach((t, i) => {
    const currency = (t.currency ?? "IDR").toUpperCase();
    if (currencyWord(currency) === "RP") {
      out.set(t.ticker, { scale: RP_MILIAR, currency });
      return;
    }
    const values = data.rows.filter((r) => r.metric.unit === "ccy").map((r) => r.cells[i]?.value ?? null);
    out.set(t.ticker, { scale: chooseScale(values.filter(present), currency), currency });
  });
  return out;
}

/** Kepala baris uang bila kolom berbeda skala atau mata uang: satuannya ditulis di kepala kolom. */
export const MIXED_HEADER = "(SESUAI KOLOM)";

/** Kepala satuan baris uang: skala bersama bila semua kolom sama; bila tidak, `MIXED_HEADER`. */
export function moneyRowHeader(scales: ReadonlyMap<string, { scale: Scale; currency: string }>): string {
  const headers = new Set([...scales.values()].map((s) => `${s.currency}|${s.scale.header}`));
  const first = [...scales.values()][0];
  return headers.size <= 1 && first ? first.scale.header : MIXED_HEADER;
}

/** Satuan di bawah kode emiten: `Rp M`, `USD rb`. */
export const columnUnit = (s: { scale: Scale; currency: string }): string => unitCaption("ccy", s.currency, s.scale);
