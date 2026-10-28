import { useId, type ReactNode } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { Button } from "../../components/Button";
import { Skeleton } from "../../components/Skeleton";
import { cn } from "../../lib/cn";

export interface BlockProps {
  /** Jangkar `?section=`. */
  id: string;
  title: string;
  /** Hanya blok yang bisa dilipat memberi `onToggle`; judulnya menjadi tombol (`aria-expanded`). */
  open?: boolean;
  onToggle?: () => void;
  /** Aksi di sisi kanan judul (tombol 44 px). */
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}

/**
 * Satu blok halaman riset: judul `h2` lalu isi. Blok lipat memuat isinya hanya saat terbuka, jadi blok yang tertutup
 * tidak meminta data apa pun.
 */
export function Block({ id, title, open = true, onToggle, action, children, className }: BlockProps) {
  const bodyId = useId();
  const Chevron = open ? ChevronDown : ChevronRight;
  return (
    <section id={id} aria-labelledby={`${id}-title`} data-block={id} className={cn("flex min-w-0 scroll-mt-4 flex-col gap-3", className)}>
      <div className="flex min-h-11 items-center justify-between gap-2">
        <h2 id={`${id}-title`} className="min-w-0 text-title text-text">
          {onToggle ? (
            // Panah sesudah judul: teks judul lipat sejajar dengan judul blok lain di tepi kiri kolom.
            <button type="button" aria-expanded={open} aria-controls={bodyId} onClick={onToggle} className="inline-flex min-h-11 items-center gap-1 rounded-sm pr-2 text-left">
              {title}
              <Chevron aria-hidden="true" size={18} strokeWidth={1.75} className="shrink-0 text-text-2" />
            </button>
          ) : (
            title
          )}
        </h2>
        {action}
      </div>
      {open ? (
        <div id={bodyId} className="flex min-w-0 flex-col gap-3">
          {children}
        </div>
      ) : null}
    </section>
  );
}

/** Kerangka memuat: `role="status"` dengan teks untuk pembaca layar. */
export function Loading({ label, rows = 2, height = 44 }: { label: string; rows?: number; height?: number }) {
  return (
    <div role="status" aria-busy="true" className="flex flex-col gap-2">
      <span className="sr-only">{label}</span>
      {Array.from({ length: rows }, (_, i) => (
        <Skeleton key={i} height={height} />
      ))}
    </div>
  );
}

/** Galat blok: pesan dan **Coba lagi**; tidak ada data pengganti. */
export function LoadError({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <div role="alert" className="flex flex-col items-start gap-1">
      <p className="text-label font-semibold text-status-over">{message}</p>
      <Button variant="ghost" onClick={onRetry} className="-ml-3">
        Coba lagi
      </Button>
    </div>
  );
}

/** Keadaan kosong yang mengajak: satu kalimat dan aksinya. */
export function Empty({ children, action }: { children: ReactNode; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-start gap-1 rounded-md border border-dashed border-border-hairline p-4">
      <p className="text-label text-text-2">{children}</p>
      {action}
    </div>
  );
}

/**
 * Satu kolom selebar kolom cangkang (maks. 480 px, design.md §1, D13); gutter 18 px datang dari cangkang. `overflow-x-clip` (bukan
 * `hidden`, jadi `sticky` tetap bekerja) menjaga halaman tidak lebih lebar dari layar: elemen yang meluap terpotong di
 * tepi kolom, bukan memperlebar viewport tata letak dan membuat halaman bergeser ke samping. `overflow-clip-margin`
 * 18 px membiarkan baris gulir yang meluas ke gutter (`-mx-[var(--gutter-left)]`) tetap tergambar sampai tepi layar.
 */
export const PAGE = "flex w-full min-w-0 flex-col gap-8 overflow-x-clip [overflow-clip-margin:18px]";

/** Kelas bersama untuk input dan select 48 px. */
export const FIELD = "h-12 w-full rounded-sm border border-transparent bg-surface-sunken px-3 text-body text-text";
