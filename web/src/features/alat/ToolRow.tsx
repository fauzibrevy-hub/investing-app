import { ChevronRight } from "lucide-react";
import { Link } from "react-router-dom";

export interface ToolRowProps {
  /** Rute layar alat (design.md §2.2). */
  to: string;
  name: string;
  /** Satu kalimat: untuk apa alat ini. */
  purpose: string;
}

/** Baris Alat (design.md §3.5, §5): dua baris, nama lalu satu kalimat tujuan, tinggi minimum 64 px, panah di kanan. */
export function ToolRow({ to, name, purpose }: ToolRowProps) {
  return (
    <Link to={to} className="flex min-h-16 w-full items-center justify-between gap-3 py-2 active:bg-surface-sunken">
      <span className="flex min-w-0 flex-col gap-0.5">
        <span data-line="1" className="text-body font-semibold text-text">
          {name}
        </span>
        <span data-line="2" className="text-caption text-text-2">
          {purpose}
        </span>
      </span>
      <ChevronRight aria-hidden="true" size={18} strokeWidth={1.75} className="shrink-0 text-text-2" />
    </Link>
  );
}
