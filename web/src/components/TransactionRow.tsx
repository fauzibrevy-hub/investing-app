import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { catColor, catTint, type CatIndex } from "../lib/categories";
import { cn } from "../lib/cn";
import { formatPercent } from "../lib/money";
import { Button } from "./Button";
import { Money } from "./Money";
import { Skeleton } from "./Skeleton";

interface BaseProps {
  title: string;
  subtitle?: string;
  icon?: LucideIcon;
  /** Avatar kiri siap pakai (emoji kategori / avatar akun, design.md §8). Menggantikan `icon`. */
  leading?: ReactNode;
  /** Warna kategori untuk ikon (solid) dan lingkaran (tint). */
  cat?: CatIndex;
  /** Baris yang sesuai segmen donut terpilih (latar `surface-sunken`). */
  selected?: boolean;
  onClick?: () => void;
  className?: string;
}

export type TransactionRowProps =
  | (BaseProps & { variant?: "transaction"; amount: number; direction: "in" | "out" })
  | (BaseProps & { variant: "category"; amount: number; percent: number })
  | (BaseProps & { variant: "position"; value: number; plPercent: number });

/**
 * Baris daftar (design.md §8). Pitch 57 px (transaksi/posisi) atau 50 px (kategori).
 * Seluruh baris adalah satu tombol bila `onClick` ada.
 */
export function TransactionRow(props: TransactionRowProps) {
  const { title, subtitle, icon: Icon, leading, cat = 8, selected, onClick, className } = props;
  const variant = props.variant ?? "transaction";
  const iconSize = variant === "category" ? 40 : 36;

  let right;
  if (props.variant === "category") {
    right = (
      <>
        <Money value={props.amount} className="text-body-strong text-text" />
        <span className="num text-caption text-text-2">{formatPercent(props.percent)}</span>
      </>
    );
  } else if (props.variant === "position") {
    const gain = props.plPercent >= 0;
    right = (
      <>
        <Money value={props.value} className="text-body-strong text-text" />
        <span className={cn("num text-caption font-bold", gain ? "text-status-safe" : "text-status-over")}>
          {formatPercent(props.plPercent, { signed: true })}
        </span>
      </>
    );
  } else {
    right = <Money value={props.amount} className={cn("text-body-strong", props.direction === "in" ? "text-money-in" : "text-money-out")} />;
  }

  const content = (
    <>
      {leading ?? null}
      {!leading && Icon ? (
        <span
          className={cn("inline-flex shrink-0 items-center justify-center rounded-pill", variant === "category" && "border border-border-hairline")}
          style={{ width: iconSize, height: iconSize, background: catTint(cat), color: catColor(cat) }}
          aria-hidden="true"
        >
          <Icon size={20} strokeWidth={1.75} />
        </span>
      ) : null}
      <span className="flex min-w-0 flex-1 flex-col">
        <span className={cn("truncate text-text", variant === "transaction" ? "text-body" : "text-body-strong")}>
          {title}
        </span>
        {subtitle ? <span className="truncate text-label text-text-2">{subtitle}</span> : null}
      </span>
      <span className="flex shrink-0 flex-col items-end text-right">{right}</span>
    </>
  );

  const rowClass = cn(
    "-mx-2 flex w-[calc(100%+16px)] items-center gap-3 rounded-sm px-2 text-left",
    variant === "category" ? "min-h-[50px]" : "min-h-[57px]",
    selected && "bg-surface-sunken",
    onClick && "active:opacity-[0.85]",
    className,
  );

  return onClick ? (
    <button type="button" className={rowClass} onClick={onClick} aria-pressed={selected === undefined ? undefined : selected}>
      {content}
    </button>
  ) : (
    <div className={rowClass}>{content}</div>
  );
}

/** Skeleton baris: lingkaran 36 px + 2 bar (140 × 14, 90 × 12) + bar kanan 72 × 14 (design.md §8). */
export function TransactionRowSkeleton({ className }: { className?: string }) {
  return (
    <div className={cn("flex min-h-[57px] items-center gap-3", className)} aria-hidden="true">
      <Skeleton circle width={36} height={36} />
      <span className="flex flex-1 flex-col gap-1.5">
        <Skeleton width={140} height={14} />
        <Skeleton width={90} height={12} />
      </span>
      <Skeleton width={72} height={14} />
    </div>
  );
}

interface ListStateProps {
  status: "empty" | "error";
  /** Default empty: "Belum ada transaksi. Ketuk + untuk mencatat." */
  message?: string;
  onRetry?: () => void;
  className?: string;
}

/** State empty/error untuk daftar baris (ditangani parent list, design.md §8). */
export function ListState({ status, message, onRetry, className }: ListStateProps) {
  if (status === "empty") {
    return <p className={cn("py-6 text-center text-label text-text-2", className)}>{message ?? "Belum ada transaksi. Ketuk + untuk mencatat."}</p>;
  }
  return (
    <div role="alert" className={cn("flex flex-col items-center gap-1 py-4 text-center", className)}>
      <p className="text-label font-semibold text-status-over">{message ?? "Gagal memuat"}</p>
      {onRetry ? (
        <Button variant="ghost" onClick={onRetry}>
          Coba lagi
        </Button>
      ) : null}
    </div>
  );
}
