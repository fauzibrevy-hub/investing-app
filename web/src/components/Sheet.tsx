import { useId, useRef, type ReactNode } from "react";
import { X } from "lucide-react";
import { cn } from "../lib/cn";
import { useModal } from "../lib/modalStack";
import { Button } from "./Button";

interface SheetProps {
  open: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
  /** Area sticky di bawah (mis. tombol "Simpan" 52 px), di atas safe-area. */
  footer?: ReactNode;
  className?: string;
  /** Judul panjang turun ke baris berikutnya (tanpa potong); bawaannya satu baris dengan elipsis. */
  wrapTitle?: boolean;
}

/**
 * Bottom sheet mobile (design.md §3.1): tinggi maks 90 vh (90 dvh di peramban yang punya, supaya judul tidak lolos di
 * atas layar saat bilah alamat Safari iOS tampil), `radius-md` atas, `shadow-popover`. Dialog modal sungguhan
 * (`lib/modalStack`): fokus masuk ke sheet, Tab berputar di dalamnya, latar `inert`, Esc menutup sheet teratas saja,
 * dan fokus kembali ke pemicu. Tutup dengan tombol, Esc, atau ketuk latar.
 */
export function Sheet({ open, title, onClose, children, footer, className, wrapTitle = false }: SheetProps) {
  const titleId = useId();
  const panelRef = useRef<HTMLDivElement>(null);
  // Pembungkus (lapisan gelap + panel) tetap interaktif; yang inert hanya latar di luarnya, supaya ketuk latar tetap menutup.
  const scopeRef = useRef<HTMLDivElement>(null);
  useModal(panelRef, open, onClose, { scope: scopeRef, lockScroll: true });

  if (!open) return null;
  return (
    <div ref={scopeRef} className="fixed inset-0 z-40 flex items-end justify-center">
      <div className="absolute inset-0" style={{ background: "rgba(10, 26, 51, 0.4)" }} aria-hidden="true" onClick={onClose} data-sheet-backdrop="" />
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        className={cn("relative flex max-h-[90vh] w-full max-w-mobile flex-col rounded-t-md bg-bg shadow-popover outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent supports-[height:100dvh]:max-h-[90dvh]", className)}
      >
        <div className={cn("flex shrink-0 items-center justify-between gap-2 pl-[var(--gutter-left)] pr-2", wrapTitle ? "min-h-14 py-2" : "h-14")}>
          <h2 id={titleId} className={cn("min-w-0 text-title text-text", wrapTitle ? "break-words" : "truncate")}>
            {title}
          </h2>
          <Button variant="icon" aria-label="Tutup" icon={<X />} onClick={onClose} />
        </div>
        <div className={cn("min-h-0 flex-1 overflow-y-auto pl-[var(--gutter-left)] pr-[var(--gutter-right)]", footer ? "pb-4" : "pb-[calc(16px+env(safe-area-inset-bottom))]")}>{children}</div>
        {footer ? (
          <div className="shrink-0 border-t border-border-hairline bg-bg pb-[calc(12px+env(safe-area-inset-bottom))] pl-[var(--gutter-left)] pr-[var(--gutter-right)] pt-3">
            {footer}
          </div>
        ) : null}
      </div>
    </div>
  );
}
