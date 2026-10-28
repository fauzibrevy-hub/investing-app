import { CircleAlert } from "lucide-react";
import { Skeleton } from "../../../components/Skeleton";
import type { LoadState } from "../../../components/Card";
import { cn } from "../../../lib/cn";

export const HISTORY_CAPTION = "riwayat < 3 th";

export interface PercentileRange {
  value: number;
  min: number;
  max: number;
  median: number;
}

export interface PercentileBandProps {
  /** Persentil 0 sampai 100; `null` bila riwayat kurang. */
  percentile: number | null;
  /** Keterangan dari backend, mis. `p41 · 10 th`. Tanpa itu dibuat dari `percentile`. */
  caption?: string | null;
  /** Bila ada, penanda diletakkan pada sumbu nilai (min sampai maks); tanpa itu pada sumbu persentil. */
  range?: PercentileRange | null;
  /** Nama untuk pembaca layar, mis. `Persentil P/E`. */
  label: string;
  width?: number;
  status?: LoadState;
  errorMessage?: string;
  className?: string;
}

const clamp = (n: number) => Math.max(0, Math.min(100, n));

/** Posisi (0 sampai 100) tiap bagian pita. Sumbu nilai memuat nilai kini walau di luar min–maks historis. */
export function bandPositions(percentile: number, range?: PercentileRange | null) {
  if (!range || ![range.value, range.min, range.max, range.median].every(Number.isFinite)) {
    return { fillStart: 0, fillEnd: 100, median: 50, marker: clamp(percentile) };
  }
  const lo = Math.min(range.min, range.value);
  const hi = Math.max(range.max, range.value);
  const span = hi - lo || 1;
  const at = (v: number) => clamp(((v - lo) / span) * 100);
  return { fillStart: at(range.min), fillEnd: at(range.max), median: at(range.median), marker: at(range.value) };
}

/**
 * Pita 120 × 12: lintasan 4 px `track`, rentang min–maks `surface-sunken`, tanda median 1 px `text-3`, penanda kini 8 px
 * `accent` (design.md §6). Riwayat kurang: hanya keterangan "riwayat < 3 th". Tanpa warna penilaian.
 */
export function PercentileBand({ percentile, caption, range, label, width = 120, status = "ready", errorMessage, className }: PercentileBandProps) {
  if (status === "loading") {
    return (
      <span role="status" aria-busy="true" className={cn("inline-block", className)}>
        <span className="sr-only">{`Memuat ${label}…`}</span>
        <Skeleton width={width} height={12} />
      </span>
    );
  }
  if (status === "error") {
    return (
      <span role="img" aria-label={`${label}: gagal memuat`} title={errorMessage} className={cn("inline-flex items-center text-text-3", className)}>
        <CircleAlert aria-hidden="true" size={14} strokeWidth={1.75} />
      </span>
    );
  }
  if (status === "empty" || percentile === null || !Number.isFinite(percentile)) {
    return (
      <span role="img" className={cn("inline-block whitespace-nowrap text-caption text-text-2", className)} aria-label={`${label}: ${caption ?? HISTORY_CAPTION}`}>
        {caption ?? HISTORY_CAPTION}
      </span>
    );
  }
  const text = caption ?? `p${Math.round(percentile)}`;
  const pos = bandPositions(percentile, range);
  return (
    <span className={cn("inline-flex items-center gap-2", className)}>
      <span
        role="meter"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(clamp(percentile))}
        aria-valuetext={text}
        className="relative block h-3 shrink-0"
        style={{ width }}
      >
        <span className="absolute inset-x-0 top-1 h-1 rounded-pill bg-track" />
        <span data-band-fill="" className="absolute top-1 h-1 bg-surface-sunken" style={{ left: `${pos.fillStart}%`, width: `${pos.fillEnd - pos.fillStart}%` }} />
        <span data-band-median="" className="absolute top-0.5 h-2 w-px bg-text-3" style={{ left: `${pos.median}%` }} />
        <span data-band-marker="" className="absolute top-0.5 h-2 w-2 -translate-x-1 rounded-pill bg-accent" style={{ left: `${pos.marker}%` }} />
      </span>
      <span className="num whitespace-nowrap text-caption text-text-2">{text}</span>
    </span>
  );
}
