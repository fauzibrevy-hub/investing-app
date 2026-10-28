import { createContext, useContext } from "react";
import type { FactTarget } from "../types";

export interface FactDrawerApi {
  /** Buka drawer untuk fakta ini; `trigger` menerima fokus kembali saat ditutup. */
  open: (target: FactTarget, trigger?: HTMLElement | null) => void;
  close: () => void;
  /** Ada drawer terbuka: layar mematikan pintasan keyboardnya selama itu (`useResearchShortcuts(h, !isOpen)`). */
  isOpen: boolean;
}

/** Disediakan `FactDrawerProvider`; `FactChip` di bawahnya membuka satu drawer bersama. */
export const FactDrawerContext = createContext<FactDrawerApi | null>(null);

export const useFactDrawer = (): FactDrawerApi | null => useContext(FactDrawerContext);
