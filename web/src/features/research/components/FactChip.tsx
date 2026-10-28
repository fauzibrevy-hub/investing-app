import { CircleAlert } from "lucide-react";
import { Skeleton } from "../../../components/Skeleton";
import type { LoadState } from "../../../components/Card";
import { cn } from "../../../lib/cn";
import { formatValue } from "../format";
import type { CellFlag, FactTarget } from "../types";
import { useFactDrawer } from "./factDrawerContext";
import { ProvenanceBadge, badgesForFlags, type ProvenanceKind } from "./ProvenanceBadge";

export const GAP_TOOLTIP = "Tidak ada di laporan";
export const UNTRACED_TOOLTIP = "Asal angka tidak tersedia";

export interface GapAction {
  /** `Isi`, `Tentukan paket`, `Tambah klaim`. */
  label: string;
  onClick: () => void;
}

export interface FactChipProps {
  value: number | null;
  /** Teks angka yang sudah diformat (skala tabel, persen); tanpa itu `formatValue(value)`. Tanpa satuan. */
  text?: string;
  /** Penanda fakta. Tanpa target angka tidak bisa dibuka dan tidak boleh ditampilkan sebagai fakta. */
  target: FactTarget | null;
  /** Nama baris, mis. `Laba bersih`. */
  label: string;
  /** Periode, mis. `Q2 26`. */
  period: string;
  flags?: readonly CellFlag[];
  /** Badge tambahan, mis. `manual`. */
  badges?: readonly ProvenanceKind[];
  /** Badge sebagai penanda pendek (tabel sempit); nama lengkapnya tetap terbaca. */
  compactBadges?: boolean;
  status?: LoadState;
  errorMessage?: string;
  /** Konteks yang bisa diisi: gap menjadi tombol aksi, bukan strip. */
  gapAction?: GapAction;
  /** Satu sel di tabel yang jadi perhentian Tab (roving tabindex); selebihnya -1. Bawaan: perhentian Tab biasa. */
  tabIndex?: number;
  /** Menggantikan drawer bersama dari `FactDrawerProvider`. */
  onOpen?: (target: FactTarget, trigger: HTMLElement) => void;
  /** Tombol membuka dialog (`FactDrawer`); false bila `onOpen` hanya memindahkan fokus, agar tidak diumumkan sebagai dialog. */
  popup?: boolean;
  className?: string;
}

/**
 * Angka inline, rata kanan, `tabular-nums`; badge design.md §3.2 setelah angka; klik atau Enter membuka `FactDrawer`
 * (design.md §6, §8). Gap = strip `text-3` dengan tooltip "Tidak ada di laporan", atau tombol aksi di konteks
 * yang bisa diisi. Angka tanpa target tidak pernah dirender sebagai angka (prinsip 4: setiap angka bisa dibuka).
 */
export function FactChip({ value, text, target, label, period, flags, badges = [], compactBadges = false, status = "ready", errorMessage, gapAction, tabIndex, onOpen, popup = true, className }: FactChipProps) {
  const drawer = useFactDrawer();

  if (status === "loading") return <Skeleton width={56} height={14} className={cn("ml-auto", className)} />;

  if (status === "error") {
    return (
      <span role="img" aria-label={`${label} ${period}: gagal memuat`} title={errorMessage} className={cn("inline-flex items-center text-text-3", className)}>
        <CircleAlert aria-hidden="true" size={14} strokeWidth={1.75} />
      </span>
    );
  }

  if (status === "empty" || value === null || !target) {
    if (gapAction) {
      return (
        <button
          type="button"
          onClick={gapAction.onClick}
          tabIndex={tabIndex}
          aria-label={`${gapAction.label} ${label} ${period}`}
          className={cn("inline-flex min-h-11 min-w-11 items-center justify-end rounded-sm px-2 text-caption font-bold text-accent", className)}
        >
          {gapAction.label}
        </button>
      );
    }
    // Angka tanpa asal tidak ditampilkan (design.md §1.4): gap dengan alasannya, bukan angka yang tidak bisa ditelusuri.
    const reason = value !== null && !target ? UNTRACED_TOOLTIP : GAP_TOOLTIP;
    return (
      <span title={reason} data-gap="" className={cn("inline-flex min-h-6 items-center text-text-3", className)}>
        <span aria-hidden="true" className="text-caption">{value !== null && !target ? "Asal belum ada" : "Belum ada"}</span>
        <span className="sr-only">{`${label} ${period}: ${reason}`}</span>
      </span>
    );
  }

  const shown = text ?? formatValue(value);
  const kinds = [...badgesForFlags(flags), ...badges.filter((b) => !badgesForFlags(flags).includes(b))];
  const open = (trigger: HTMLElement) => (onOpen ? onOpen(target, trigger) : drawer?.open(target, trigger));
  return (
    <span className={cn("inline-flex min-h-11 items-center justify-end gap-1", className)}>
      <button
        type="button"
        aria-haspopup={popup ? "dialog" : undefined}
        aria-label={`Asal angka ${label} ${period}: ${shown}`}
        onClick={(e) => open(e.currentTarget)}
        tabIndex={tabIndex}
        className="num min-h-11 min-w-11 rounded-sm px-1 text-right decoration-dotted underline-offset-2 hover:underline focus-visible:underline"
      >
        <span aria-hidden="true">{shown}</span>
      </button>
      {kinds.map((kind) => (
        <ProvenanceBadge key={kind} kind={kind} compact={compactBadges} />
      ))}
    </span>
  );
}
