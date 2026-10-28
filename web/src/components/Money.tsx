import { formatRupiah, type RupiahOptions } from "../lib/money";
import { HIDDEN_ARIA_LABEL, useHideAmounts } from "../lib/privacy";
import { cn } from "../lib/cn";

interface MoneyProps extends Omit<RupiahOptions, "hidden"> {
  value: number;
  className?: string;
}

/** Nominal rupiah lewat formatter pusat, mengikuti mode sembunyi nominal. */
export function Money({ value, className, ...options }: MoneyProps) {
  const { hidden } = useHideAmounts();
  return (
    <span className={cn("num", className)} aria-label={hidden ? HIDDEN_ARIA_LABEL : undefined}>
      {formatRupiah(value, { ...options, hidden })}
    </span>
  );
}
