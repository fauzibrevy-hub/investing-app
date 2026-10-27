import type { ReactNode } from "react";
import { cn } from "../lib/cn";
import type { LoadState } from "./Card";

export interface SectionHeaderAction {
  label: string;
  onClick?: () => void;
  href?: string;
}

export interface SectionHeaderProps {
  title: string;
  /**
   * `section`: judul section ("Terakhir dicatat"), tinggi 44 px.
   * `screen`: judul layar ("Kategori"); 24/30 di desktop (design.md §4).
   * `app`: wordmark "Finance" di header Beranda, tinggi 52 px.
   */
  variant?: "section" | "screen" | "app";
  level?: 1 | 2 | 3;
  /** Link teks kanan ("Semua"). */
  action?: SectionHeaderAction;
  /** Elemen kanan kustom (tombol ikon, chip bulan). Menggantikan `action`. */
  trailing?: ReactNode;
  /** Selain `ready`, aksi disembunyikan (design.md §8). */
  status?: LoadState;
  className?: string;
}

/** Tanda app: lingkaran accent dengan dompet putih tersenyum (setara logo moni, bukan salinannya). */
function AppMark() {
  return (
    <svg width={32} height={32} viewBox="0 0 32 32" aria-hidden="true" className="shrink-0">
      <circle cx={16} cy={16} r={16} fill="var(--accent)" />
      <rect x={7.5} y={10} width={17} height={12.5} rx={4} fill="#FFFFFF" />
      <path d="M12 15.5c1.2 1.7 2.5 2.5 4 2.5s2.8-.8 4-2.5" fill="none" stroke="var(--accent)" strokeWidth={2.2} strokeLinecap="round" />
    </svg>
  );
}

const actionClass =
  "-mr-2 inline-flex min-h-11 min-w-11 items-center justify-end px-2 text-label font-semibold text-text-2 active:opacity-[0.85]";

export function SectionHeader({ title, variant = "section", level, action, trailing, status = "ready", className }: SectionHeaderProps) {
  const Heading = `h${level ?? (variant === "section" ? 2 : 1)}` as "h1" | "h2" | "h3";
  const showAction = status === "ready";

  let right: ReactNode = null;
  if (showAction && trailing) right = trailing;
  else if (showAction && action) {
    right = action.href ? (
      <a href={action.href} className={actionClass}>
        {action.label}
      </a>
    ) : (
      <button type="button" onClick={action.onClick} className={actionClass}>
        {action.label}
      </button>
    );
  }

  return (
    <div className={cn("flex items-center justify-between gap-3", variant === "app" ? "min-h-[52px]" : "min-h-11", className)}>
      {variant === "app" ? (
        // Logo + wordmark gaya moni: lingkaran accent 32 px, wordmark huruf kecil 28/32 800.
        <Heading className="flex min-w-0 items-center gap-2 text-[28px] font-extrabold leading-8 tracking-[-0.8px] text-text">
          <AppMark />
          <span className="truncate lowercase">{title}</span>
        </Heading>
      ) : (
        <Heading className={cn("min-w-0 truncate text-text", variant === "screen" ? "text-title lg:text-title-desktop" : "text-title")}>{title}</Heading>
      )}
      {right}
    </div>
  );
}
