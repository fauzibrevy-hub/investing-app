import type { CoverageRow } from "../../research/types";

/** Kuartal sebelum label `2026Q3` (`2026Q2`; `2027Q1` → `2026Q4`); label yang tidak dikenal → `null`. */
export function previousQuarter(label: string): string | null {
  const m = /^(\d{4})Q([1-4])$/.exec(label);
  if (!m) return null;
  const [fy, q] = [Number(m[1]), Number(m[2])];
  return q === 1 ? `${fy - 1}Q4` : `${fy}Q${q - 1}`;
}

/**
 * Kuartal laporan terbaru sebuah emiten. Desk tidak mengirimnya langsung, tetapi `next_report` adalah kuartal sesudah
 * laporan terakhir yang masuk (backend: `calendar::next_report`), jadi yang terbaru adalah kuartal sebelumnya.
 * `null` bila emiten belum punya laporan.
 */
export function latestReportQuarter(row: Pick<CoverageRow, "next_report">): string | null {
  return row.next_report ? previousQuarter(row.next_report.label) : null;
}

export interface CoverageGroups {
  held: CoverageRow[];
  manual: CoverageRow[];
  peers: CoverageRow[];
}

/** "Perusahaan saya": dipegang dulu, lalu manual; pembanding dilipat terpisah. Urutan backend dipertahankan. */
export function groupCoverage(rows: readonly CoverageRow[]): CoverageGroups {
  return {
    held: rows.filter((r) => r.role === "held"),
    manual: rows.filter((r) => r.role === "manual"),
    peers: rows.filter((r) => r.role === "peer"),
  };
}
