import { Link } from "react-router-dom";
import { ChevronLeft } from "lucide-react";
import { PackOverride } from "../../research/components/PackOverride";
import { formatAge, formatValue } from "../../research/format";
import type { DossierResponse } from "../../research/types";

/** Lencana turun baris bila sempit (zoom teks 200% di 197 px): tinggi minimum 24 px, bukan tinggi tetap dengan satu baris yang terpotong. */
const BADGE = "inline-flex min-h-6 max-w-full items-center whitespace-normal break-words rounded-sm bg-surface-sunken px-2 py-0.5 text-table-head uppercase text-text-2";

/** Teks laporan terakhir: `Laporan Q3 2026 · 12 hari lalu`. */
export function filingText(filing: DossierResponse["latest_filing"]): string {
  if (!filing) return "Belum ada laporan";
  const age = formatAge(filing.age_days);
  return `Laporan ${filing.period} ${filing.fy} · ${age === "hari ini" ? age : `${age} lalu`}`;
}

/** Blok 0: kembali ke Riset, kode dan nama, paket (dengan **Ubah paket**), laporan terakhir, lencana dipegang. */
export function Header({ ticker, dossier }: { ticker: string; dossier: DossierResponse }) {
  const { pack } = dossier;
  return (
    <header className="flex flex-col gap-1" data-block="header">
      <Link to="/riset" className="-ml-1 inline-flex min-h-11 w-fit items-center gap-1 rounded-sm pl-1 pr-3 text-label font-bold text-accent">
        <ChevronLeft aria-hidden="true" size={18} strokeWidth={1.75} />
        Riset
      </Link>
      <h1 className="flex flex-wrap items-baseline gap-x-2 text-title text-text">
        <span className="text-stat">{ticker}</span>
        {dossier.name ? <span className="min-w-0 break-words text-label font-semibold text-text-2">{dossier.name}</span> : null}
      </h1>
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span className={BADGE} title={pack.id ? `${pack.id} · versi ${pack.version ?? ""}` : undefined}>
          {pack.label ?? "Tanpa paket"}
        </span>
        {dossier.role === "held" ? <span className={BADGE}>{dossier.avg_price !== null ? `Dipegang · rata-rata ${formatValue(dossier.avg_price)}` : "Dipegang"}</span> : null}
      </div>
      <p className="text-caption text-text-2">{filingText(dossier.latest_filing)}</p>
      {/* Baris sendiri: panel pilihan paket (320 px) melebar dari tepi kiri tombolnya. */}
      <div className="-ml-3">
        <PackOverride ticker={ticker} pack={pack} />
      </div>
    </header>
  );
}
