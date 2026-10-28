import { useRef } from "react";
import { useOverflow } from "../lib/useOverflow";
import { cn } from "../lib/cn";
import type { LoadState } from "./Card";
import { Skeleton } from "./Skeleton";

export interface StatRowProps {
  label: string;
  /** Nilai yang sudah diformat (pakai `formatRupiah`/`formatPercent`). */
  value?: string;
  status?: LoadState;
  /** Vertikal (kartu ringkasan mobile) atau horizontal (label kiri, nilai kanan; desktop/fundamental). */
  orientation?: "vertical" | "horizontal";
  /** Penyebab error, tampil sebagai tooltip `title` di em dash. */
  errorReason?: string;
  /**
   * Di atas permukaan tint (mis. `surface-summary`). `text-3` hanya lolos kontras di `bg` putih (§1.2),
   * jadi em dash empty/error memakai `text-2` di sini.
   */
  onTint?: boolean;
  /** Warna nilai, mis. `text-status-safe` untuk delta. */
  valueClassName?: string;
  /**
   * Label boleh dua baris (bukan dipotong titik-titik) dan selalu menyisakan tinggi dua baris dengan teks di dasar,
   * sehingga nilai di sel-sel sebaris sejajar. Hanya untuk orientasi vertikal.
   */
  wrapLabel?: boolean;
  className?: string;
}

const DASH = "—";

/**
 * Label `type-label` `text-2` + nilai `type-stat`, jarak 4 px (design.md §8).
 * Nilai tidak wrap: jika lebih lebar dari kolom, turun ke `type-label` 700 (design.md §3.1 blok 5); bila masih lebih
 * lebar (angka mentah triliunan), baru dibungkus per digit, karena angka yang terpotong terbaca sebagai angka lain.
 */
export function StatRow({
  label,
  value,
  status = "ready",
  orientation = "vertical",
  errorReason,
  onTint = false,
  valueClassName,
  wrapLabel = false,
  className,
}: StatRowProps) {
  const valueRef = useRef<HTMLSpanElement>(null);
  const compact = useOverflow(valueRef, value, status === "ready" && value !== undefined);

  const dashClass = onTint ? "text-text-2" : "text-text-3";
  let valueNode;
  if (status === "loading") {
    valueNode = <Skeleton width={64} height={18} className="my-0.5" />;
  } else if (status === "empty" || status === "error" || value === undefined) {
    valueNode = (
      <span className={cn("text-stat", dashClass)} title={status === "error" ? errorReason : undefined} aria-label={status === "error" ? `${label}: gagal memuat` : `${label}: belum ada data`}>
        {DASH}
      </span>
    );
  } else {
    valueNode = (
      <span
        ref={valueRef}
        className={cn("num block min-w-0 overflow-hidden", compact ? "" : "whitespace-nowrap", compact ? "whitespace-normal text-label font-bold [overflow-wrap:anywhere]" : "text-stat", valueClassName ?? "text-text")}
      >
        {value}
      </span>
    );
  }

  return (
    <div
      className={cn(
        "min-w-0",
        orientation === "vertical" ? "flex flex-col gap-1" : "flex items-baseline justify-between gap-3",
        className,
      )}
    >
      {wrapLabel ? (
        <span className="flex min-h-[2.6em] items-end break-words text-left text-label leading-snug text-text-2">{label}</span>
      ) : (
        <span className="truncate text-label text-text-2">{label}</span>
      )}
      {orientation === "horizontal" ? <span className="min-w-0 text-right">{valueNode}</span> : valueNode}
    </div>
  );
}
