import { useMemo, useRef } from "react";
import { newIdempotencyKey } from "../../../lib/api";

export interface DraftKey {
  /** Kunci untuk isi ini: sama selama isinya sama (percobaan ulang), baru begitu draf diubah. */
  forBody: (body: unknown) => string;
  /** Setelah tersimpan atau saat editor dibuka lagi: isi yang sama berikutnya adalah tulisan baru. */
  reset: () => void;
}

/**
 * `Idempotency-Key` yang mengikuti isi draf. Backend menolak kunci yang sama untuk isi lain (409), jadi draf yang
 * diubah setelah percobaan gagal harus mendapat kunci baru, sedangkan percobaan ulang isi yang sama tidak boleh
 * membuat tulisan kedua.
 */
export function useDraftKey(): DraftKey {
  const current = useRef<{ body: string; key: string } | null>(null);
  return useMemo(
    () => ({
      forBody(body: unknown) {
        const text = JSON.stringify(body);
        if (current.current?.body !== text) current.current = { body: text, key: newIdempotencyKey() };
        return current.current.key;
      },
      reset() {
        current.current = null;
      },
    }),
    [],
  );
}
