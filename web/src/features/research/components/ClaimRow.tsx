import { cn } from "../../../lib/cn";
import { formatDate } from "../format";
import type { Claim } from "../types";
import { formatMetricValue, unitWord } from "./ClaimCard";
import { ClaimStatusMark, STATUS_WORDS } from "./claimStatus";

export interface ClaimRowProps {
  claim: Claim;
  /** Mata uang pelaporan untuk satuan `ccy`. */
  currency?: string | null;
  /** Ketuk: buka kartu lengkap (di mobile dalam `Sheet`). `trigger` menerima fokus kembali saat sheet ditutup. */
  onOpen: (claim: Claim, trigger: HTMLElement) => void;
  className?: string;
}

/**
 * Klaim ringkas untuk daftar mobile (design.md §5.2): titik + kata status, pernyataan (dua baris), jatuh tempo dan
 * nilai terakhir. Seluruh baris adalah satu tombol 44 px ke atas; kartu lengkap (evaluasi, bukti, **Tutup klaim**)
 * ada di `Sheet`. Klaim tertutup bergaris putus-putus seperti `ClaimCard`.
 */
export function ClaimRow({ claim, currency = null, onOpen, className }: ClaimRowProps) {
  const last = claim.last_eval;
  const lastText =
    last && last.value !== null
      ? `Terakhir ${formatMetricValue(last.value, claim.unit)}${claim.unit === "ccy" ? ` ${unitWord("ccy", currency)}` : ""}`
      : last
        ? "Belum ada nilai"
        : "Belum dievaluasi";
  const closed = claim.closed_at !== null;
  return (
    <button
      type="button"
      aria-haspopup="dialog"
      data-claim-status={claim.status}
      onClick={(e) => onOpen(claim, e.currentTarget)}
      className={cn("flex min-h-11 w-full min-w-0 flex-col items-stretch gap-1 rounded-md border border-border-hairline p-3 text-left", closed && "border-dashed", className)}
    >
      <ClaimStatusMark status={claim.status}>{claim.status_label ?? STATUS_WORDS[claim.status]}</ClaimStatusMark>
      <span className="line-clamp-2 break-words text-body text-text">{claim.statement}</span>
      <span className="text-caption text-text-2">{`jatuh tempo ${formatDate(claim.due)} · ${lastText}${closed ? " · ditutup" : ""}`}</span>
    </button>
  );
}
