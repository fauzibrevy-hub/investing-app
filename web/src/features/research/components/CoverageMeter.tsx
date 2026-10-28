import { CircleAlert } from "lucide-react";
import { Skeleton } from "../../../components/Skeleton";
import type { LoadState } from "../../../components/Card";
import { cn } from "../../../lib/cn";

export interface CoverageMeterProps {
  populated: number;
  total: number;
  /** Nama untuk pembaca layar, mis. `Cakupan XBRL`. */
  label: string;
  status?: LoadState;
  errorMessage?: string;
  className?: string;
}

/**
 * Batang 64 × 6, isi `accent`, keterangan `9/10` (design.md §6). Di bawah 50 % keterangan memakai teks
 * `status-warn`; warna batang tidak berubah. Tanpa metrik: `0/0` dengan "tanpa paket".
 */
export function CoverageMeter({ populated, total, label, status = "ready", errorMessage, className }: CoverageMeterProps) {
  if (status === "loading") {
    return (
      <span role="status" aria-busy="true" className={cn("inline-flex items-center gap-2", className)}>
        <span className="sr-only">{`Memuat ${label}…`}</span>
        <Skeleton width={64} height={6} />
        <Skeleton width={24} height={12} />
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
  if (status === "empty" || total <= 0) {
    return (
      <span className={cn("inline-flex items-center gap-2", className)}>
        <span className="h-1.5 w-16 rounded-pill bg-track" aria-hidden="true" />
        <span className="num text-caption text-text-2">0/0 tanpa paket</span>
      </span>
    );
  }
  // Nilai meter dijepit ke rentangnya: ARIA tidak boleh menyatakan 12 dari 10.
  const shown = Math.max(0, Math.min(total, Number.isFinite(populated) ? populated : 0));
  const pct = (shown / total) * 100;
  return (
    <span className={cn("inline-flex items-center gap-2", className)}>
      <span
        role="meter"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={total}
        aria-valuenow={shown}
        aria-valuetext={`${shown} dari ${total}`}
        className="relative block h-1.5 w-16 shrink-0 overflow-hidden rounded-pill bg-track"
      >
        <span className="absolute inset-y-0 left-0 bg-accent" style={{ width: `${pct}%` }} />
      </span>
      <span data-low={pct < 50 || undefined} className={cn("num text-caption", pct < 50 ? "text-status-warn" : "text-text-2")}>
        {shown}/{total}
      </span>
    </span>
  );
}
