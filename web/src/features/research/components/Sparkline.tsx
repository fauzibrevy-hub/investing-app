import { CircleAlert } from "lucide-react";
import { Skeleton } from "../../../components/Skeleton";
import type { LoadState } from "../../../components/Card";
import { linePath, type Scales, type TrendPoint } from "../../../components/charts/trendGeometry";
import { cn } from "../../../lib/cn";

const PAD = 2;

export interface SparklineGeometry {
  /** Satu `M … L …` per rangkaian nilai yang tidak terputus; celah (null) memutus garis. */
  path: string;
  /** Titik terakhir yang punya nilai. */
  last: { x: number; y: number } | null;
  /** Garis nol hairline, hanya bila ada nilai negatif. */
  zeroY: number | null;
}

/** Geometri murni. Nilai negatif ada di bawah garis nol: sumbu memuat 0 saat ada yang negatif. */
export function sparklineGeometry(values: ReadonlyArray<number | null>, width: number, height: number): SparklineGeometry {
  const present = values.filter((v): v is number => v !== null && Number.isFinite(v));
  if (present.length === 0) return { path: "", last: null, zeroY: null };
  const hasNegative = present.some((v) => v < 0);
  let lo = Math.min(...present);
  let hi = Math.max(...present);
  if (hasNegative) {
    lo = Math.min(lo, 0);
    hi = Math.max(hi, 0);
  }
  const span = hi - lo || 1;
  const n = values.length;
  const scales: Scales = {
    x: (i) => PAD + (n > 1 ? (i / (n - 1)) * (width - 2 * PAD) : (width - 2 * PAD) / 2),
    // Nilai tinggi di atas. Rangkaian datar digambar di tengah.
    y: (v) => (hi === lo ? height / 2 : PAD + (1 - (v - lo) / span) * (height - 2 * PAD)),
    plotLeft: PAD,
    plotRight: width - PAD,
    plotTop: PAD,
    plotBottom: height - PAD,
  };

  const runs: TrendPoint[][] = [];
  let run: TrendPoint[] = [];
  let last: SparklineGeometry["last"] = null;
  values.forEach((v, i) => {
    if (v === null || !Number.isFinite(v)) {
      if (run.length) runs.push(run);
      run = [];
      return;
    }
    run.push({ x: i, value: v, label: String(i) });
    last = { x: scales.x(i), y: scales.y(v) };
  });
  if (run.length) runs.push(run);
  return {
    path: runs.map((r) => linePath(r, scales)).join(" "),
    last,
    zeroY: hasNegative ? scales.y(0) : null,
  };
}

export interface SparklineProps {
  /** 12 kuartal mandiri, terlama dulu; `null` = celah. */
  values: ReadonlyArray<number | null>;
  /** Nama untuk pembaca layar, mis. `Laba bersih, 12 kuartal`. */
  label: string;
  width?: number;
  height?: number;
  status?: LoadState;
  errorMessage?: string;
  className?: string;
}

/**
 * Garis 1,5 px `text-2`, titik terakhir 4 px `accent`, tanpa isi (design.md §6). Nilai negatif di bawah garis nol
 * hairline. Kurang dari dua nilai: keterangan "Data belum cukup". Ukuran bawaan 96 × 24 (baris `MetricRow`).
 */
export function Sparkline({ values, label, width = 96, height = 24, status = "ready", errorMessage, className }: SparklineProps) {
  if (status === "loading") {
    return (
      <span role="status" aria-busy="true" className={cn("inline-block", className)}>
        <span className="sr-only">{`Memuat ${label}…`}</span>
        <Skeleton width={width} height={height} />
      </span>
    );
  }
  if (status === "error") {
    return (
      <span role="img" aria-label={`${label}: gagal memuat`} title={errorMessage} className={cn("inline-flex items-center text-text-3", className)} style={{ width, height }}>
        <CircleAlert aria-hidden="true" size={14} strokeWidth={1.75} />
      </span>
    );
  }
  const count = values.filter((v) => v !== null && Number.isFinite(v)).length;
  if (status === "empty" || count < 2) {
    return (
      <span className={cn("inline-flex items-center text-caption text-text-3", className)} style={{ width, height }}>
        <span aria-hidden="true">Data belum cukup</span>
        <span className="sr-only">{`${label}: data belum cukup`}</span>
      </span>
    );
  }
  const geo = sparklineGeometry(values, width, height);
  return (
    <svg role="img" aria-label={label} width={width} height={height} viewBox={`0 0 ${width} ${height}`} className={cn("block shrink-0", className)}>
      {geo.zeroY !== null ? <line data-zero-line="" x1={PAD} x2={width - PAD} y1={geo.zeroY} y2={geo.zeroY} strokeWidth={1} className="stroke-border-hairline" /> : null}
      <path d={geo.path} fill="none" strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round" className="stroke-text-2" />
      {geo.last ? <circle data-last-point="" cx={geo.last.x} cy={geo.last.y} r={2} className="fill-accent" /> : null}
    </svg>
  );
}
