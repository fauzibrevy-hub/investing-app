import { formatDate } from "../research/format";
import type { CoverageRow } from "../research/types";
import type { DraftsResponse, ThesisSummary } from "../thesis/types";

export interface TesisLainRow {
  ticker: string;
  kind: "Dossier" | "Draf";
  /** Tanggal pembaruan, sudah berformat; `null` bila dossier belum pernah ditinjau. */
  updated: string | null;
}

/**
 * "Tesis lain" (design.md §3.3 blok 5, prd.md O1): ticker yang punya dossier atau draf tesis tetapi tidak ada di
 * semesta riset (`coverage`). Satu baris per ticker: "Dossier" bila ada dossier di vault (tanggal = tinjauan terakhir),
 * selain itu "Draf" (tanggal = pembaruan draf terbaru). Urutan: ticker.
 */
export function tesisLainRows(coverage: CoverageRow[], summary: ThesisSummary | undefined, drafts: DraftsResponse | undefined): TesisLainRow[] {
  const universe = new Set(coverage.map((c) => c.ticker.toUpperCase()));
  const rows = new Map<string, TesisLainRow>();
  for (const i of summary?.issuers ?? []) {
    const ticker = i.ticker.toUpperCase();
    if (!universe.has(ticker)) rows.set(ticker, { ticker, kind: "Dossier", updated: i.review.last_date ? formatDate(i.review.last_date) : null });
  }
  const newest = new Map<string, string>();
  for (const d of drafts?.drafts ?? []) {
    const ticker = d.ticker.toUpperCase();
    if (universe.has(ticker) || rows.has(ticker)) continue;
    if (!newest.has(ticker) || Date.parse(d.updated_at) > Date.parse(newest.get(ticker)!)) newest.set(ticker, d.updated_at);
  }
  for (const [ticker, at] of newest) rows.set(ticker, { ticker, kind: "Draf", updated: formatDate(at) });
  return [...rows.values()].sort((a, b) => a.ticker.localeCompare(b.ticker));
}
