import { Financials } from "../../research/components/Financials";
import type { DossierResponse } from "../../research/types";
import { Block } from "../ui";

/**
 * Blok 9 "Keuangan" (lipat): tampilan Kuartal · TTM · Tahunan dan 8 · 12 · 20 periode, tabel per grup dengan kolom
 * pertama tetap dan gulir di dalam kotaknya. Memakai `Financials` dari Riset; dipasang hanya saat blok terbuka.
 */
export function Keuangan({ ticker, dossier, open, onToggle }: { ticker: string; dossier: DossierResponse; open: boolean; onToggle: () => void }) {
  return (
    <Block id="keuangan" title="Keuangan" open={open} onToggle={onToggle}>
      <Financials ticker={ticker} dossier={dossier} />
    </Block>
  );
}
