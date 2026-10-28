import { useCallback, useEffect, useRef, useState } from "react";

/**
 * Sheet mobile yang mengembalikan fokus ke pemicunya. `Sheet` memulihkan `document.activeElement` saat dibuka, tetapi
 * Safari tidak memfokuskan tombol yang diketuk, jadi pemicu dicatat sendiri dan difokuskan setelah sheet tertutup.
 * `close` tetap stabil (identitasnya aman dipakai di dependensi efek).
 */
export function useSheetTrigger<T = true>(): { value: T | null; open: (value: T, trigger: HTMLElement | null) => void; close: () => void } {
  const [value, setValue] = useState<T | null>(null);
  const trigger = useRef<HTMLElement | null>(null);
  const refocus = useRef(false);

  const open = useCallback((next: T, from: HTMLElement | null) => {
    trigger.current = from;
    setValue(next);
  }, []);
  const close = useCallback(() => {
    refocus.current = true;
    setValue(null);
  }, []);

  useEffect(() => {
    if (value === null && refocus.current) {
      refocus.current = false;
      const el = trigger.current;
      if (el?.isConnected) el.focus();
    }
  }, [value]);

  return { value, open, close };
}
