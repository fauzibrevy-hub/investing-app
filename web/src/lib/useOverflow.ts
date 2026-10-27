import { useLayoutEffect, useState, type RefObject } from "react";

/**
 * true jika isi elemen lebih lebar dari kotaknya. Dipakai agar nominal tidak pernah wrap:
 * komponen beralih ke varian ringkas saat meluap (design.md §7).
 * Diukur ulang saat `key` berubah atau induk berubah ukuran.
 */
export function useOverflow(ref: RefObject<HTMLElement | null>, key: unknown, enabled = true): boolean {
  const [overflow, setOverflow] = useState(false);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !enabled) return;
    let frame = 0;
    const measure = () => {
      // Kembalikan ke ukuran penuh, ukur di frame berikutnya.
      setOverflow(false);
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => setOverflow(el.scrollWidth > el.clientWidth + 0.5));
    };
    measure();
    // Hanya lebar yang relevan; tinggi berubah saat beralih varian dan tidak boleh memicu ukur ulang (loop).
    let lastWidth = el.parentElement?.clientWidth ?? 0;
    const ro = new ResizeObserver(() => {
      const width = el.parentElement?.clientWidth ?? 0;
      if (width !== lastWidth) {
        lastWidth = width;
        measure();
      }
    });
    if (el.parentElement) ro.observe(el.parentElement);
    return () => {
      cancelAnimationFrame(frame);
      ro.disconnect();
    };
  }, [ref, key, enabled]);
  return overflow;
}
