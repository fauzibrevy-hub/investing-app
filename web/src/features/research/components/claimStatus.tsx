import { cn } from "../../../lib/cn";
import type { ClaimCounts, ClaimStatus } from "../types";

/** Urutan klaim (design.md §4.2): Patah, Belum teruji, Berlaku, Kedaluwarsa. */
export const STATUS_ORDER: readonly ClaimStatus[] = ["broken", "not_testable", "holding", "expired"];

/** Kata status (design.md §3.4). Selalu tampil di samping titiknya: tidak pernah hanya warna. */
export const STATUS_WORDS: Record<ClaimStatus, string> = {
  holding: "Berlaku",
  broken: "Patah",
  not_testable: "Belum teruji",
  expired: "Kedaluwarsa",
};

const DOTS: Record<ClaimStatus, string> = {
  holding: "bg-status-safe-fill",
  broken: "bg-status-over-fill",
  // `#8A94A6` (design.md §3.4) = `cat-8`.
  not_testable: "bg-cat-8",
  expired: "bg-status-warn-fill",
};

const TEXT: Record<ClaimStatus, string> = {
  holding: "text-status-safe",
  broken: "text-status-over",
  not_testable: "text-status-neutral",
  expired: "text-status-warn",
};

export interface ClaimStatusMarkProps {
  status: ClaimStatus;
  /** Teks di samping titik; bawaan kata status. */
  children?: string;
  className?: string;
}

/** Titik + kata status (`status-*`, design.md §3.4). */
export function ClaimStatusMark({ status, children, className }: ClaimStatusMarkProps) {
  return (
    <span data-status={status} className={cn("inline-flex items-center gap-1.5 whitespace-nowrap text-caption font-bold", TEXT[status], className)}>
      <span aria-hidden="true" className={cn("h-2 w-2 shrink-0 rounded-pill", DOTS[status])} />
      {children ?? STATUS_WORDS[status]}
    </span>
  );
}

export interface ClaimCountsLineProps {
  counts: ClaimCounts;
  /** Sembunyikan status yang nol (strip sempit). */
  hideZero?: boolean;
  className?: string;
}

/** Hitungan klaim per status dengan titik dan kata, urutan Patah, Belum teruji, Berlaku, Kedaluwarsa. */
export function ClaimCountsLine({ counts, hideZero, className }: ClaimCountsLineProps) {
  const shown = STATUS_ORDER.filter((s) => !hideZero || counts[s] > 0);
  if (shown.length === 0) return <span className={cn("text-caption text-text-2", className)}>Belum ada klaim</span>;
  return (
    <span className={cn("inline-flex flex-wrap items-center gap-x-3 gap-y-1", className)}>
      {shown.map((s) => (
        <ClaimStatusMark key={s} status={s}>{`${counts[s]} ${STATUS_WORDS[s]}`}</ClaimStatusMark>
      ))}
    </span>
  );
}
