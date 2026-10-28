import { useId, useRef, useState, type KeyboardEvent } from "react";
import { cn } from "../../../lib/cn";
import { formatDate } from "../format";
import type { GapAction } from "./FactChip";

export interface SourcedValueProps {
  /** Angka yang sudah diformat, tanpa satuan; `null` = gap. */
  text: string | null;
  /** Nama angka, mis. `Close`, `P/E TTM`. */
  label: string;
  /** Asal angka dalam satu kalimat, mis. `Harga penutupan harian, GET /COAL/prices`. */
  source: string;
  /** Tanggal acuan `YYYY-MM-DD` (atau RFC 3339). */
  date?: string | null;
  /** Baris tambahan: rumus, basis, versi aturan. */
  detail?: readonly string[];
  /** Alasan gap (tooltip dan teks pembaca layar). */
  gapReason?: string;
  /** Konteks yang bisa diisi: gap menjadi tombol aksi. */
  gapAction?: GapAction;
  className?: string;
}

/**
 * Angka turunan dengan asalnya (design.md prinsip 4, "setiap angka bisa dibuka") untuk angka yang tidak punya fakta
 * XBRL tunggal: statistik harga dan angka valuasi. Klik atau Enter membuka keterangan asal di bawah angka
 * (disclosure, `aria-expanded`); Esc menutupnya. `FactChip` tetap dipakai untuk angka yang punya fakta filing.
 */
export function SourcedValue({ text, label, source, date, detail = [], gapReason = "Belum tersedia", gapAction, className }: SourcedValueProps) {
  const [open, setOpen] = useState(false);
  const panelId = useId();
  const button = useRef<HTMLButtonElement>(null);

  if (text === null) {
    if (gapAction) {
      return (
        <button
          type="button"
          onClick={gapAction.onClick}
          aria-label={`${gapAction.label} ${label}`}
          className={cn("inline-flex min-h-11 min-w-11 items-center justify-end rounded-sm px-2 text-caption font-bold text-accent", className)}
        >
          {gapAction.label}
        </button>
      );
    }
    return (
      <span title={gapReason} data-gap="" className={cn("inline-flex min-h-6 items-center justify-end text-text-3", className)}>
        <span aria-hidden="true" className="text-caption">{gapReason}</span>
        <span className="sr-only">{`${label}: ${gapReason}`}</span>
      </span>
    );
  }

  const onKeyDown = (e: KeyboardEvent<HTMLElement>) => {
    if (e.key === "Escape" && open) {
      e.stopPropagation();
      setOpen(false);
      button.current?.focus();
    }
  };

  return (
    <span className={cn("inline-flex min-w-0 max-w-full flex-col items-end", className)} onKeyDown={onKeyDown}>
      <button
        ref={button}
        type="button"
        aria-expanded={open}
        aria-controls={panelId}
        aria-label={`Asal angka ${label}: ${text}`}
        onClick={() => setOpen((v) => !v)}
        className="num min-h-11 min-w-11 max-w-full rounded-sm px-1 text-right [overflow-wrap:anywhere] decoration-dotted underline-offset-2 hover:underline focus-visible:underline"
      >
        {text}
      </button>
      <span id={panelId} hidden={!open} data-source="" className="max-w-[28ch] text-right text-caption text-text-2">
        {source}
        {date ? <span className="block">{`per ${formatDate(date)}`}</span> : null}
        {detail.map((line) => (
          <span key={line} className="block">
            {line}
          </span>
        ))}
      </span>
    </span>
  );
}
