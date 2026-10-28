import type { CSSProperties, ReactNode } from "react";
import { CircleAlert } from "lucide-react";
import { cn } from "../lib/cn";
import { Button } from "./Button";
import { Skeleton } from "./Skeleton";

export type CardTone =
  | "plain"
  | "summary"
  | "sunken"
  | "tint-blue"
  | "tint-green"
  | "account-1"
  | "account-2"
  | "account-3";
export type LoadState = "ready" | "loading" | "empty" | "error";

const tones: Record<CardTone, string> = {
  // `bg` + outline hairline; kartu lain memakai tint tanpa border. Tidak pernah border + shadow (§7).
  plain: "bg-bg border border-border-hairline",
  summary: "bg-surface-summary",
  sunken: "bg-surface-sunken",
  "tint-blue": "bg-surface-tint-blue",
  "tint-green": "bg-surface-tint-green",
  "account-1": "bg-surface-account-1",
  "account-2": "bg-surface-account-2",
  "account-3": "bg-surface-account-3",
};

export interface CardProps {
  tone?: CardTone;
  status?: LoadState;
  children?: ReactNode;
  /** Default "Belum ada data bulan ini". */
  emptyMessage?: string;
  emptyAction?: ReactNode;
  /** Default "Gagal memuat". */
  errorMessage?: string;
  onRetry?: () => void;
  /** Tinggi minimum, dipertahankan di semua state agar tidak ada layout shift. */
  minHeight?: number;
  className?: string;
  as?: "div" | "section" | "article";
  "aria-label"?: string;
}

/** Kartu: padding 16, `radius-md`, tanpa shadow (design.md §1.7, §8). */
export function Card({
  tone = "plain",
  status = "ready",
  children,
  emptyMessage = "Belum ada data bulan ini",
  emptyAction,
  errorMessage = "Gagal memuat",
  onRetry,
  minHeight,
  className,
  as: Tag = "div",
  "aria-label": ariaLabel,
}: CardProps) {
  const style: CSSProperties | undefined = minHeight ? { minHeight } : undefined;
  let body: ReactNode;
  if (status === "loading") {
    // Isi tetap dirender tak terlihat agar tinggi sama persis; skeleton menutupinya.
    body = (
      <>
        <div className="invisible" aria-hidden="true">
          {children}
        </div>
        <div className="absolute inset-4 flex flex-col gap-2">
          <Skeleton className="h-full w-full" />
        </div>
      </>
    );
  } else if (status === "empty") {
    body = (
      <div className="flex h-full flex-col items-center justify-center gap-2 text-center" style={style}>
        <p className="text-label text-text-2">{emptyMessage}</p>
        {emptyAction}
      </div>
    );
  } else if (status === "error") {
    body = (
      <div role="alert" className="flex h-full flex-col items-center justify-center gap-1 text-center" style={style}>
        <p className="inline-flex items-center gap-1.5 text-label font-semibold text-status-over">
          <CircleAlert aria-hidden="true" size={16} strokeWidth={1.75} />
          {errorMessage}
        </p>
        {onRetry ? (
          <Button variant="ghost" onClick={onRetry}>
            Coba lagi
          </Button>
        ) : null}
      </div>
    );
  } else {
    body = children;
  }

  return (
    <Tag
      className={cn("relative rounded-md p-4", tones[tone], className)}
      style={style}
      aria-busy={status === "loading" || undefined}
      aria-label={ariaLabel}
    >
      {body}
    </Tag>
  );
}
