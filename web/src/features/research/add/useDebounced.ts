import { useEffect, useState } from "react";

/** Nilai yang menyusul `value` setelah `ms` tanpa perubahan (kolom cari tidak memanggil backend per ketukan). */
export function useDebounced<T>(value: T, ms = 200): T {
  const [shown, setShown] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setShown(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return shown;
}
