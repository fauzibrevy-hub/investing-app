import { forwardRef, type ButtonHTMLAttributes, type MouseEvent, type ReactNode } from "react";
import { cn } from "../lib/cn";
import { Spinner } from "./Spinner";

export type ButtonVariant = "primary" | "sunken" | "ghost" | "icon" | "chip";

export interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "disabled"> {
  variant?: ButtonVariant;
  /** `danger` = teks `status-over` (mis. hapus draft, design.md §3.4). Hanya untuk `ghost`. */
  tone?: "default" | "danger";
  loading?: boolean;
  /** Memakai `aria-disabled` (tetap bisa difokus) dan menahan klik. */
  disabled?: boolean;
  fullWidth?: boolean;
  /** Chip terpilih (`accent-soft` + teks `accent`). */
  selected?: boolean;
  /** Ikon di depan label; untuk varian `icon` ini satu-satunya isi (wajib `aria-label`). */
  icon?: ReactNode;
  children?: ReactNode;
}

/*
 * Ukuran (design.md §8, §9): primary/sunken 52 px; ghost min 44 px;
 * icon visual 32 px dalam hit area 44 × 44; chip visual 36 px dalam hit area tinggi 44.
 * Elemen <button> sendiri adalah hit area, sehingga getBoundingClientRect() ≥ 44.
 */
const base =
  "relative inline-flex select-none items-center justify-center whitespace-nowrap transition-opacity active:opacity-[0.85] aria-disabled:cursor-not-allowed aria-disabled:opacity-40 aria-disabled:active:opacity-40";

const variants: Record<ButtonVariant, string> = {
  primary: "on-accent h-[52px] gap-2 rounded-pill bg-accent px-6 text-body-strong text-text-on-accent",
  sunken: "h-[52px] gap-2 rounded-lg bg-surface-sunken px-6 text-body-strong text-accent",
  ghost: "min-h-11 min-w-11 gap-1.5 rounded-sm px-3 text-label font-bold",
  icon: "h-11 w-11 shrink-0 rounded-pill",
  chip: "min-h-11 shrink-0",
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "primary", tone = "default", loading = false, disabled = false, fullWidth, selected, icon, children, className, onClick, type = "button", ...rest },
  ref,
) {
  const inert = disabled || loading;
  const handleClick = (e: MouseEvent<HTMLButtonElement>) => {
    if (inert) {
      e.preventDefault();
      return;
    }
    onClick?.(e);
  };

  const label = (
    <>
      {icon && variant !== "icon" ? <span className="inline-flex shrink-0 [&>svg]:h-5 [&>svg]:w-5">{icon}</span> : null}
      {children}
    </>
  );

  let content: ReactNode;
  if (variant === "icon") {
    content = (
      <span className="inline-flex h-8 w-8 items-center justify-center rounded-pill bg-surface-icon text-text [&>svg]:h-5 [&>svg]:w-5">
        {loading ? <Spinner size={18} /> : icon}
      </span>
    );
  } else if (variant === "chip") {
    content = (
      <span
        className={cn(
          "inline-flex h-9 items-center gap-1 rounded-sm px-3 text-label font-semibold [&>span>svg]:h-4 [&>span>svg]:w-4",
          selected ? "bg-accent-soft text-accent" : "bg-surface-sunken text-text",
        )}
      >
        <span className={cn("inline-flex items-center gap-1", loading && "invisible")}>{label}</span>
        {loading ? (
          <span className="absolute inset-0 flex items-center justify-center">
            <Spinner />
          </span>
        ) : null}
      </span>
    );
  } else {
    // Label tetap dirender (invisible) saat loading agar lebar tombol terkunci.
    content = (
      <>
        <span className={cn("inline-flex items-center gap-2", loading && "invisible")}>{label}</span>
        {loading ? (
          <span className="absolute inset-0 flex items-center justify-center">
            <Spinner />
          </span>
        ) : null}
      </>
    );
  }

  return (
    <button
      ref={ref}
      type={type}
      className={cn(
        base,
        variants[variant],
        variant === "ghost" && (tone === "danger" ? "text-status-over" : "text-accent"),
        fullWidth && "w-full",
        className,
      )}
      aria-disabled={disabled || undefined}
      aria-busy={loading || undefined}
      aria-pressed={variant === "chip" && selected !== undefined ? selected : undefined}
      onClick={handleClick}
      {...rest}
    >
      {content}
    </button>
  );
});
