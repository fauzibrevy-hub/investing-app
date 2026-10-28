import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";

/**
 * Mode sembunyi nominal (design.md §3.1 blok 2, §6.8). Preferensi tampilan ini disimpan di
 * localStorage (bukan data sensitif); token tidak pernah disimpan di sini.
 */
const STORAGE_KEY = "investing.hideAmounts";

interface HideAmountsValue {
  hidden: boolean;
  toggle: () => void;
}

const HideAmountsContext = createContext<HideAmountsValue>({ hidden: false, toggle: () => {} });

function readStored(): boolean {
  try {
    return window.localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

export function HideAmountsProvider({ children, initial }: { children: ReactNode; initial?: boolean }) {
  const [hidden, setHidden] = useState(() => initial ?? readStored());
  const toggle = useCallback(() => {
    setHidden((h) => {
      const next = !h;
      try {
        window.localStorage.setItem(STORAGE_KEY, next ? "1" : "0");
      } catch {
        /* storage diblokir: tetap berlaku untuk sesi ini */
      }
      return next;
    });
  }, []);
  const value = useMemo(() => ({ hidden, toggle }), [hidden, toggle]);
  return <HideAmountsContext.Provider value={value}>{children}</HideAmountsContext.Provider>;
}

export function useHideAmounts(): HideAmountsValue {
  return useContext(HideAmountsContext);
}

/** Label aksesibel untuk nominal yang disamarkan (design.md §9). */
export const HIDDEN_ARIA_LABEL = "nominal disembunyikan";
