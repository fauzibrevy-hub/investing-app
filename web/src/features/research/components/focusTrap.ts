import { useEffect, useRef, type RefObject } from "react";
import { pushModal } from "../../../lib/modalStack";

export interface FocusTrapOptions {
  /** Pemicu yang menerima fokus kembali; tanpa itu elemen yang aktif saat jebakan dipasang. */
  returnFocusTo?: HTMLElement | null;
  /**
   * Jebakan dipasang pada dialog terdekat dari elemen `panel` (mis. `[role=dialog]`), bukan pada `panel` sendiri.
   * Dipakai di dalam `Sheet`, yang sudah mendaftarkan dialognya ke tumpukan modal (Tab, Esc, `inert`); jebakan ini
   * hanya memindahkan fokus masuk dan mengembalikannya ke pemicu.
   */
  closest?: string;
  /** Esc ditangani jebakan ini. Matikan bila dialog induk sudah menanganinya (`Sheet`). Tidak berlaku dengan `closest`. */
  handleEscape?: boolean;
  /** Ganti nilainya untuk memindahkan fokus kembali ke panel tanpa memasang ulang jebakan (mis. fakta lain dibuka). */
  refocusKey?: string;
}

/**
 * Jebakan fokus untuk panel dialog (design.md §8): fokus masuk ke panel saat aktif, Tab dan Shift+Tab berputar
 * di dalamnya, Esc memanggil `onEscape`, dan fokus kembali ke pemicu saat dilepas (bila pemicu masih ada di dokumen).
 * Panel ikut tumpukan modal (`lib/modalStack`): hanya dialog teratas yang menerima Tab dan Esc, jadi sheet yang dibuka
 * di atas panel ini tidak ikut tertarik fokusnya. Latar tidak dibuat `inert` (panel samping tanpa lapisan gelap).
 */
export function useFocusTrap(panel: RefObject<HTMLElement | null>, active: boolean, onEscape: () => void, options: FocusTrapOptions = {}): void {
  const { closest, handleEscape = true, refocusKey } = options;
  const escape = useRef(onEscape);
  escape.current = onEscape;
  const returnTo = useRef(options.returnFocusTo);
  returnTo.current = options.returnFocusTo;
  const container = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const el = (closest ? panel.current?.closest<HTMLElement>(closest) : panel.current) ?? null;
    container.current = el;
    if (!active || !el) return;
    const previous = document.activeElement as HTMLElement | null;
    const release = closest
      ? null
      : pushModal({ panel: el, scope: el, onEscape: handleEscape ? () => escape.current() : null, inertBackground: false, lockScroll: false });
    el.focus();

    return () => {
      release?.();
      const target = returnTo.current?.isConnected ? returnTo.current : previous?.isConnected ? previous : null;
      // Sheet memulihkan fokusnya sendiri saat dilepas; pemicu kita harus menang, jadi dipindahkan sesudahnya.
      if (closest) setTimeout(() => target?.focus?.(), 0);
      else target?.focus?.();
    };
  }, [panel, active, closest, handleEscape]);

  // Isi dialog berganti (operand membuka fakta lain): elemen yang fokusnya hilang tidak boleh meninggalkan fokus di <body>.
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    if (active) container.current?.focus();
  }, [refocusKey, active]);
}
