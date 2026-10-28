import { useState } from "react";
import { Link } from "react-router-dom";
import { cn } from "../../lib/cn";
import { CoverageMeter } from "../research/components/CoverageMeter";
import { ClaimCountsLine } from "../research/components/claimStatus";
import { formatDay, periodLabel } from "../research/format";
import type { CoverageRow, DeskResponse } from "../research/types";
import { groupCoverage, latestReportQuarter } from "../riset/screens/landingData";
import { Block, Empty } from "../riset/ui";

/** Satu emiten, dua baris (design.md §3.3 blok 2): ticker · paket · kuartal terakhir; klaim · KPI · laporan berikut. */
export function CompanyRow({ row }: { row: CoverageRow }) {
  const latest = latestReportQuarter(row);
  const kpiDone = row.kpi.entered + row.kpi.unknown;
  const next = row.next_report;
  return (
    <li data-company={row.ticker} className="border-b border-border-hairline last:border-b-0">
      <Link to={`/perusahaan/${row.ticker}`} className="flex min-h-16 flex-col justify-center gap-0.5 py-2 active:bg-surface-sunken">
        <span data-line="1" className="flex min-w-0 items-baseline gap-2 text-table">
          <span className="shrink-0 text-body-strong text-text">{row.ticker}</span>
          <span className="min-w-0 truncate text-text-2">{row.pack.label ?? "Tanpa paket"}</span>
          <span className="ml-auto shrink-0 whitespace-nowrap text-caption text-text-2">{latest ? periodLabel(latest) : "belum ada laporan"}</span>
        </span>
        <span data-line="2" className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-0.5 text-caption text-text-2">
          <ClaimCountsLine counts={row.claims} hideZero />
          <span className="inline-flex items-center gap-1.5">
            KPI
            <CoverageMeter populated={kpiDone} total={row.kpi.expected} label={`Cakupan KPI ${row.ticker}`} />
          </span>
          {next ? <span className={cn(next.overdue && "font-semibold text-status-warn")}>{`Laporan berikut: ${periodLabel(next.label)}, batas ${formatDay(next.deadline)}${next.overdue ? " (lewat)" : ""}`}</span> : null}
        </span>
      </Link>
    </li>
  );
}

function Rows({ label, rows }: { label: string; rows: CoverageRow[] }) {
  return (
    <ul aria-label={label}>
      {rows.map((r) => (
        <CompanyRow key={r.ticker} row={r} />
      ))}
    </ul>
  );
}

/**
 * Blok 2–4 layar Perusahaan (design.md §3.3; kontrak plan.md §3.3): Dipegang, Manual (bila ada), Pembanding (n)
 * (lipat, bila ada). Tidak memuat data sendiri; header, **Tambah**, muat, galat, "Tesis lain" dan footer milik layar.
 */
export function CoverageList({ desk }: { desk: DeskResponse }) {
  const [showPeers, setShowPeers] = useState(false);
  const { held, manual, peers } = groupCoverage(desk.coverage);
  let heldBody;
  if (held.length) heldBody = <Rows label="Dipegang" rows={held} />;
  else if (manual.length) heldBody = <Empty>Tidak ada emiten yang dipegang.</Empty>;
  else
    heldBody = (
      <Empty
        action={
          <Link to="/perusahaan/tambah" className="inline-flex min-h-11 items-center text-label font-bold text-accent">
            Tambah emiten
          </Link>
        }
      >
        Belum ada emiten di Riset. Tambah satu kode emiten untuk mulai mencatat strukturnya, klaim, dan jurnalnya.
      </Empty>
    );
  return (
    <>
      <Block id="dipegang" title="Dipegang">
        {heldBody}
      </Block>
      {manual.length ? (
        <Block id="manual" title="Manual">
          <Rows label="Manual" rows={manual} />
        </Block>
      ) : null}
      {peers.length ? (
        <Block id="pembanding" title={`Pembanding (${peers.length})`} open={showPeers} onToggle={() => setShowPeers((v) => !v)}>
          <Rows label="Pembanding" rows={peers} />
        </Block>
      ) : null}
    </>
  );
}
