import { catColor, type CatIndex } from "../lib/categories";
import { cn } from "../lib/cn";
import { formatPercent, formatRupiah } from "../lib/money";
import { HIDDEN_ARIA_LABEL, useHideAmounts } from "../lib/privacy";
import type { LoadState } from "./Card";
import { Money } from "./Money";
import { Skeleton } from "./Skeleton";

export type BudgetStatus = "safe" | "warn" | "over";

/** ASUMSI ambang 85% (design.md §5.3). */
export const WARN_THRESHOLD = 0.85;

/** Status dari `terpakai / limit`: < 85% aman, 85–100% warning, > 100% lewat. */
export function budgetStatus(spent: number, limit: number): BudgetStatus {
  if (limit <= 0) return spent > 0 ? "over" : "safe";
  const ratio = spent / limit;
  if (ratio > 1) return "over";
  if (ratio >= WARN_THRESHOLD) return "warn";
  return "safe";
}

/** Lebar isi bar 0..1; lewat = penuh. */
export function fillRatio(spent: number, limit: number): number {
  if (limit <= 0) return spent > 0 ? 1 : 0;
  return Math.min(1, Math.max(0, spent / limit));
}

const statusText: Record<BudgetStatus, string> = {
  safe: "text-status-neutral",
  warn: "text-status-warn",
  over: "text-status-over",
};

function fillClass(status: BudgetStatus, variant: "total" | "row"): string {
  if (status === "over") return "bg-status-over-fill";
  if (status === "warn") return "bg-status-warn-fill";
  return variant === "total" ? "bg-accent" : "bg-status-safe-fill";
}

/** Teks status kanan: "sisa Rp540 rb" / "92% terpakai" / "lewat Rp35 rb". */
export function budgetStatusLabel(spent: number, limit: number, hidden = false): string {
  const status = budgetStatus(spent, limit);
  if (status === "warn") return `${formatPercent(Math.round((spent / limit) * 100))} terpakai`;
  if (status === "over") return `lewat ${formatRupiah(spent - limit, { short: true, hidden })}`;
  return `sisa ${formatRupiah(limit - spent, { short: true, hidden })}`;
}

interface CommonProps {
  spent?: number;
  limit?: number;
  status?: LoadState;
  className?: string;
}

export type BudgetBarProps =
  | (CommonProps & {
      variant: "total";
      /** Hari ini (1-based) dan jumlah hari bulan: penanda hari + "sisa …/hari". */
      day: number;
      daysInMonth: number;
    })
  | (CommonProps & {
      variant?: "row";
      name: string;
      cat: CatIndex;
      /** Bar 4 px di bawah baris. Beranda tidak menampilkannya (design.md §5.3). */
      showBar?: boolean;
      /** Penanda opsional 0..1, mis. bobot target di "Alokasi vs target". */
      marker?: number;
      /** Ganti teks kanan, mis. drift "+2,1 pp". */
      rightText?: string;
    });

function Track({
  ratio,
  fill,
  height,
  marker,
  markerHeight,
  grey,
  label,
}: {
  ratio: number;
  fill: string;
  height: number;
  marker?: number;
  markerHeight?: number;
  grey?: boolean;
  label?: string;
}) {
  return (
    <div
      className={cn("relative w-full rounded-pill", grey ? "bg-border-hairline" : "bg-track")}
      style={{ height }}
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
    >
      {ratio > 0 ? <div className={cn("h-full rounded-pill", fill)} style={{ width: `${ratio * 100}%` }} data-fill="" /> : null}
      {marker !== undefined && markerHeight ? (
        <div
          data-marker=""
          className="absolute w-0.5 -translate-x-1/2 rounded-pill bg-text"
          style={{ left: `${Math.min(1, Math.max(0, marker)) * 100}%`, height: markerHeight, top: (height - markerHeight) / 2 }}
        />
      ) : null}
    </div>
  );
}

/** Bar anggaran (design.md §5.3). */
export function BudgetBar(props: BudgetBarProps) {
  const { spent = 0, limit = 0, status = "ready", className } = props;
  const { hidden } = useHideAmounts();
  const variant = props.variant ?? "row";
  const barHeight = variant === "total" ? 6 : 4;

  if (status === "empty") {
    return <p className={cn("py-2 text-label text-text-2", className)}>Anggaran belum diatur di Sheets</p>;
  }
  if (status === "error") {
    return (
      <div className={cn("flex flex-col gap-3", className)} role="alert">
        <p className="text-label font-semibold text-status-over">Gagal memuat anggaran</p>
        <Track ratio={0} fill="" height={barHeight} grey />
      </div>
    );
  }

  const loading = status === "loading";
  const bStatus = budgetStatus(spent, limit);
  const ratio = loading ? 0 : fillRatio(spent, limit);
  const statusLabel = budgetStatusLabel(spent, limit, hidden);

  if (props.variant === "total") {
    const { day, daysInMonth } = props;
    const daysLeft = Math.max(1, daysInMonth - day + 1);
    const right =
      bStatus === "over" ? (
        <span className="num text-label font-semibold text-status-over">{statusLabel}</span>
      ) : (
        <span className="num text-label text-text-2" aria-label={hidden ? HIDDEN_ARIA_LABEL : undefined}>
          sisa {formatRupiah((limit - spent) / daysLeft, { short: true, hidden })}/hari
        </span>
      );
    return (
      <div className={cn("flex flex-col gap-3", className)}>
        <div className="flex items-baseline justify-between gap-3 whitespace-nowrap">
          {loading ? (
            <>
              <Skeleton width={150} height={18} />
              <Skeleton width={110} height={16} />
            </>
          ) : (
            <>
              <span className="min-w-0 truncate">
                <Money value={spent} short className="text-body-strong text-text" />{" "}
                <span className="text-body text-text-2">
                  dari <Money value={limit} short />
                </span>
              </span>
              {right}
            </>
          )}
        </div>
        <Track
          ratio={ratio}
          fill={fillClass(bStatus, "total")}
          height={6}
          marker={day / daysInMonth}
          markerHeight={16}
          label={loading ? undefined : `${formatPercent(Math.round((spent / (limit || 1)) * 100))} anggaran terpakai, hari ke-${day} dari ${daysInMonth}`}
        />
      </div>
    );
  }

  const { name, cat, showBar = true, marker, rightText } = props;
  return (
    <div className={cn("flex min-h-12 flex-col justify-center gap-2", className)}>
      <div className="flex items-center justify-between gap-3">
        <span className="flex min-w-0 items-center gap-2">
          <span className="h-2.5 w-2.5 shrink-0 rounded-pill" style={{ background: catColor(cat) }} aria-hidden="true" />
          <span className="truncate text-body text-text">{name}</span>
        </span>
        {loading ? (
          <Skeleton width={84} height={16} />
        ) : (
          <span className={cn("num shrink-0 whitespace-nowrap text-label font-semibold", rightText ? "text-text-2" : statusText[bStatus])}>
            {rightText ?? statusLabel}
          </span>
        )}
      </div>
      {showBar ? <Track ratio={ratio} fill={fillClass(bStatus, "row")} height={4} marker={marker} markerHeight={marker !== undefined ? 12 : undefined} /> : null}
    </div>
  );
}
