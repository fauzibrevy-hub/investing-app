import { cn } from "../lib/cn";

export interface SegmentedOption<T extends string> {
  value: T;
  label: string;
  /** Jika ada, segmen menjadi tautan (mis. "#/thesis"). */
  href?: string;
}

interface SegmentedProps<T extends string> {
  options: SegmentedOption<T>[];
  value: T;
  onChange?: (value: T) => void;
  /** Nama grup untuk pembaca layar. */
  label: string;
  /** `scroll`: segmen selebar isi, geser horizontal (Tipe di sheet "Catat transaksi"). */
  layout?: "fill" | "scroll";
  className?: string;
}

/**
 * Segmented control (design.md §3.3–§3.6): tiap segmen 44 px (target sentuh §9), track 48 px. Segmen terpilih: `bg` putih + teks `accent` 700
 * di atas track `surface-sunken`; tiap segmen adalah target sentuh 44 px penuh.
 */
export function Segmented<T extends string>({ options, value, onChange, label, layout = "fill", className }: SegmentedProps<T>) {
  return (
    <div
      role={options.some((o) => o.href) ? "navigation" : "radiogroup"}
      aria-label={label}
      className={cn(
        "flex h-12 rounded-sm bg-surface-sunken p-0.5",
        layout === "scroll" ? "hscroll" : "w-full",
        className,
      )}
    >
      {options.map((o) => {
        const active = o.value === value;
        const cls = cn(
          "inline-flex h-11 min-w-11 items-center justify-center whitespace-nowrap rounded-[6px] px-4 text-label",
          layout === "fill" && "flex-1",
          active ? "border border-border-hairline bg-bg font-bold text-accent" : "font-semibold text-text-2",
        );
        return o.href ? (
          <a key={o.value} href={o.href} aria-current={active ? "page" : undefined} className={cls}>
            {o.label}
          </a>
        ) : (
          <button key={o.value} type="button" role="radio" aria-checked={active} className={cls} onClick={() => onChange?.(o.value)}>
            {o.label}
          </button>
        );
      })}
    </div>
  );
}
