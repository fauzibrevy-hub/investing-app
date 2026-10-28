import { formatAge, formatDate } from "../../research/format";
import type { DossierResponse } from "../../research/types";

/** Blok 10: umur data per sumber dan penafian. */
export function Footer({ dossier }: { dossier: DossierResponse }) {
  const { latest_close: close, latest_filing: filing } = dossier;
  return (
    <footer id="footer" data-block="footer" className="flex flex-col gap-1 border-t border-border-hairline pt-3 text-caption text-text-2">
      <p>
        {`Harga ${close ? `${formatDate(close.date)} (${formatAge(close.age_days)})` : "belum ada"} · Laporan ${filing ? `tersedia sejak ${formatDate(filing.available_from)} (${formatAge(filing.age_days)})` : "belum ada"}`}
      </p>
      <p>{`Data dimuat ${formatDate(dossier.as_of)} · Bukan saran investasi`}</p>
    </footer>
  );
}
