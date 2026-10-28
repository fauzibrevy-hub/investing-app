import { formatAge, formatDate } from "../format";
import type { DeskResponse } from "../types";

/** Rentang umur per sumber: `terbaru 1 hari, terlama 2 bulan`. */
function range(values: Array<number | null>): string {
  const known = values.filter((v): v is number => typeof v === "number");
  if (known.length === 0) return "belum ada";
  const [min, max] = [Math.min(...known), Math.max(...known)];
  return min === max ? formatAge(min) : `terbaru ${formatAge(min)}, terlama ${formatAge(max)}`;
}

/** Footer design.md §4.1 blok 5 dan §7.8: umur data per sumber, lalu "Bukan saran investasi". */
export function DeskFooter({ ages }: { ages?: DeskResponse["ages"] }) {
  const drivers = ages ? Object.entries(ages.drivers) : [];
  return (
    <footer className="flex flex-col gap-1 border-t border-border-hairline pt-3 text-caption text-text-2">
      {ages ? (
        <p>
          {`Umur data · Harga: ${range(Object.values(ages.prices))} · Laporan: ${range(Object.values(ages.filings))}`}
          {drivers.length ? ` · Pendorong: ${drivers.map(([id, d]) => `${ages?.driver_labels?.[id] ?? id} ${formatAge(d)}`).join(", ")}` : ""}
        </p>
      ) : null}
      <p>Bukan saran investasi</p>
    </footer>
  );
}

/** Footer layar Riset lain (design.md §7.8): waktu data, lalu "Bukan saran investasi". */
export function ScreenFooter({ asOf }: { asOf?: string }) {
  return (
    <footer className="flex flex-col gap-1 border-t border-border-hairline pt-3 text-caption text-text-2">
      {asOf ? <p>{`Data per ${formatDate(asOf)}`}</p> : null}
      <p>Bukan saran investasi</p>
    </footer>
  );
}
