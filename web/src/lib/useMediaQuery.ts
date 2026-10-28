import { useSyncExternalStore } from "react";

export function useMediaQuery(query: string): boolean {
  return useSyncExternalStore(
    (onChange) => {
      const mql = window.matchMedia(query);
      mql.addEventListener("change", onChange);
      return () => mql.removeEventListener("change", onChange);
    },
    () => window.matchMedia(query).matches,
    () => false,
  );
}

/** Kontrak breakpoint: ≥ 1024 px = layout desktop. */
export const DESKTOP_QUERY = "(min-width: 1024px)";
export const useIsDesktop = () => useMediaQuery(DESKTOP_QUERY);
