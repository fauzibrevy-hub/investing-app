import { useId, useState, type ReactNode } from "react";
import { StatRow } from "../../../components/StatRow";
import { cn } from "../../../lib/cn";
import { formatDate } from "../format";

const TARGET = "block min-h-11 w-full min-w-0 rounded-sm text-left";

export interface FactStatProps {
  label: string;
  /** Nama untuk pembaca layar, mis. `Pendapatan Q2 26`; label tombol menjadi "Asal angka <nama>" (design.md §8). */
  name: string;
  value: string | null;
  /** Buka asal angka (drawer fakta bersama). Tanpa itu angka tidak ditampilkan sebagai fakta. */
  onOpen: ((trigger: HTMLElement) => void) | null;
  orientation?: "vertical" | "horizontal";
  /** Kelas nilai (rata kanan di sel dua kolom). */
  valueClassName?: string;
  className?: string;
}

/** `StatRow` yang seluruhnya satu tombol (hit ≥ 44 px) ke `FactDrawer`; gap tampil sebagai em dash tanpa tombol. */
export function FactStat({ label, name, value, onOpen, orientation = "horizontal", valueClassName, className }: FactStatProps) {
  if (value === null || !onOpen) return <StatRow label={label} status="empty" orientation={orientation} className={cn("min-h-11 items-center", className)} />;
  return (
    <button type="button" aria-haspopup="dialog" onClick={(e) => onOpen(e.currentTarget)} className={cn(TARGET, "relative flex items-center", className)}>
      <span className="sr-only">{`Asal angka ${name}: `}</span>
      <StatRow label={label} value={value} orientation={orientation} valueClassName={valueClassName} className="w-full" />
    </button>
  );
}

export interface SourcedStatProps {
  label: string;
  /** Nama angka untuk pembaca layar dan label tombol. */
  name: string;
  value: string | null;
  /** Asal angka dalam satu kalimat; tampil di bawah baris saat dibuka. */
  source: string;
  date?: string | null;
  detail?: readonly string[];
  /** Keterangan di bawah nilai yang selalu tampil (mis. jarak dari puncak). */
  sub?: ReactNode;
  gapReason: string;
  /**
   * Sel grid label-di-atas-nilai: label dua baris yang tidak dipotong, nilai rata kiri di bawahnya, jadi kolom 2 × N
   * sejajar. Tanpa ini nilai rata kanan dan label dipotong satu baris.
   */
  stacked?: boolean;
  /**
   * Nama tombol diawali label yang terlihat (WCAG 2.5.3: pengguna kontrol suara mengucapkan yang dilihatnya), lalu nilainya,
   * lalu "asal angka": `Harga (Rp) 1.290, asal angka Harga`. Bawaannya `Asal angka <nama>: <nilai>` yang dipakai dossier lama.
   */
  labelFirst?: boolean;
  className?: string;
}

/** `StatRow` sebagai tombol pembuka asal angka (disclosure, `aria-expanded`), seperti `SourcedValue` di desktop. */
export function SourcedStat({ label, name, value, source, date, detail = [], sub, gapReason, stacked = false, labelFirst = false, className }: SourcedStatProps) {
  const [open, setOpen] = useState(false);
  const panel = useId();
  if (value === null) {
    return (
      <div className={cn("flex min-w-0 flex-col", className)}>
        <StatRow label={label} status="empty" errorReason={gapReason} wrapLabel={stacked} className={stacked ? "justify-start" : "min-h-11 justify-center"} />
        <span className="text-caption text-text-2">{gapReason}</span>
      </div>
    );
  }
  return (
    <div className={cn("flex min-w-0 flex-col", className)}>
      <button type="button" aria-expanded={open} aria-controls={panel} aria-label={labelFirst ? `${label} ${value}, asal angka ${name}` : `Asal angka ${name}: ${value}`} onClick={() => setOpen((o) => !o)} className={cn(TARGET, stacked ? "block" : "flex items-center")}>
        <StatRow label={label} value={value} wrapLabel={stacked} valueClassName={stacked ? "text-left text-text" : "text-right text-text"} className="w-full" />
      </button>
      {sub}
      <span id={panel} hidden={!open} data-source="" className="text-caption text-text-2">
        {source}
        {date ? <span className="block">{`per ${formatDate(date)}`}</span> : null}
        {detail.map((line) => (
          <span key={line} className="block">
            {line}
          </span>
        ))}
      </span>
    </div>
  );
}
