import { useLookup } from "../api";
import type { LookupItem } from "../types";
import { useDebounced } from "./useDebounced";

export const TICKER_LENGTH = 4;
export const LETTERS = /^[A-Z]*$/;

export interface TickerCheck {
  /** Empat huruf, sudah berhenti diketik, dan hasil backend belum datang. */
  pending: boolean;
  /** Pemeriksaan gagal (jaringan atau server), bukan "tidak ada di daftar". */
  failed: boolean;
  /** Daftar bursa terbaca; `null` selama belum ada jawaban. */
  listAvailable: boolean | null;
  /** Kecocokan persis; `null` bila tidak ada di daftar bursa atau belum dicek. */
  item: LookupItem | null;
  reload: () => void;
}

/** Cek kode ke `GET /lookup` setelah jeda ketik; hasil basi (kode sudah berubah) tidak dipakai. */
export function useTickerCheck(ticker: string): TickerCheck {
  const settled = useDebounced(ticker);
  const complete = settled.length === TICKER_LENGTH && LETTERS.test(settled);
  const lookup = useLookup(complete ? settled : null, 1);
  const current = complete && settled === ticker;
  const data = current ? lookup.data : undefined;
  return {
    pending: ticker.length === TICKER_LENGTH && LETTERS.test(ticker) && (!current || (lookup.status === "loading" && !data)),
    failed: current && lookup.status === "error" && !data,
    listAvailable: data ? data.list_available : null,
    item: data?.exact ?? null,
    reload: lookup.reload,
  };
}

/** Alasan kode belum bisa dilanjutkan, `null` bila bisa. Kode yang belum bisa diperiksa karena daftar bursa hilang tetap boleh. */
export function tickerBlocker(ticker: string, c: TickerCheck): string | null {
  if (ticker.length !== TICKER_LENGTH || !LETTERS.test(ticker)) return "Kode emiten terdiri dari empat huruf";
  if (c.pending) return "Kode masih diperiksa";
  if (c.failed) return "Gagal memeriksa kode, coba lagi";
  if (c.listAvailable === null) return "Kode masih diperiksa";
  if (c.listAvailable && !c.item) return `${ticker} tidak ada di daftar bursa`;
  if (c.item?.in_universe && c.item.role !== "peer") return `${ticker} sudah ada di Riset`;
  return null;
}
