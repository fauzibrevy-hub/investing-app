import { useId, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from "react";
import { catColor } from "../../lib/categories";
import { cn } from "../../lib/cn";
import { formatPercent, formatRupiah } from "../../lib/money";
import { HIDDEN_ARIA_LABEL, useHideAmounts } from "../../lib/privacy";
import { useOverflow } from "../../lib/useOverflow";
import { Button } from "../Button";
import type { LoadState } from "../Card";
import { Skeleton } from "../Skeleton";
import {
  arcPath,
  centerLabelLayout,
  centerMaxWidth,
  computeArcs,
  DONUT_SIZES,
  HIT_EXTRA,
  prepareSegments,
  ringGeometry,
  type DonutDatum,
} from "./donutGeometry";

export interface DonutChartProps {
  data: DonutDatum[];
  /** Baris 1 label tengah, mis. "Pengeluaran" atau "Nilai pasar". */
  title: string;
  /** Baris 3 label tengah saat tidak ada yang dipilih. Default "`n` kategori". */
  countLabel?: (count: number) => string;
  status?: LoadState;
  onRetry?: () => void;
  /** Default 164 px (mobile, D13); 220 px hanya bila diminta. */
  size?: "mobile" | "desktop";
  /** Indeks segmen terpilih (terkontrol), mis. untuk menyorot baris di daftar. */
  selected?: number | null;
  onSelectedChange?: (index: number | null) => void;
  /** Ringkasan `aria-label`; default dibangun dari data. */
  ariaLabel?: string;
  className?: string;
}

const DIM_OPACITY = 0.35;

export function DonutChart({
  data,
  title,
  countLabel = (n) => `${n} kategori`,
  status = "ready",
  onRetry,
  size = "mobile",
  selected: selectedProp,
  onSelectedChange,
  ariaLabel,
  className,
}: DonutChartProps) {
  const { diameter, thickness } = DONUT_SIZES[size];
  const ring = useMemo(() => ringGeometry(diameter, thickness), [diameter, thickness]);
  const box = diameter + HIT_EXTRA * 2;
  const c = box / 2;
  const { hidden } = useHideAmounts();
  const maskId = useId();

  const segments = useMemo(() => (status === "ready" ? prepareSegments(data) : []), [data, status]);
  const arcs = useMemo(() => computeArcs(segments, ring), [segments, ring]);
  const total = segments.reduce((s, x) => s + x.value, 0);
  const effective: LoadState = status === "ready" && segments.length === 0 ? "empty" : status;

  const [selectedState, setSelectedState] = useState<number | null>(null);
  const selected = selectedProp !== undefined ? selectedProp : selectedState;
  const [focused, setFocused] = useState<number | null>(null);
  const hitRefs = useRef<Array<SVGPathElement | null>>([]);

  const select = (index: number | null) => {
    if (selectedProp === undefined) setSelectedState(index);
    onSelectedChange?.(index);
  };
  const toggle = (index: number) => select(selected === index ? null : index);

  const onSegmentKey = (e: KeyboardEvent<SVGPathElement>, index: number) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      toggle(index);
    } else if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      const n = segments.length;
      const next = (index + (e.key === "ArrowRight" ? 1 : -1) + n) % n;
      hitRefs.current[next]?.focus();
      select(next);
    }
  };

  const shortMoney = (v: number) => formatRupiah(v, { short: true, hidden });
  const summary =
    ariaLabel ??
    (segments.length
      ? `${title} ${shortMoney(total)}, terbesar ${segments[0].label} ${formatPercent(segments[0].share * 100)}`
      : title);

  // ---- label tengah ----
  const { tops } = centerLabelLayout();
  const maxW = centerMaxWidth(diameter, thickness);
  const sel = selected !== null && selected < segments.length ? segments[selected] : null;
  let lines: [ReactNode, ReactNode, ReactNode] | null = null;
  let centerNode: ReactNode = null;
  if (effective === "ready") {
    lines = sel
      ? [sel.label, shortMoney(sel.value), `${formatPercent(sel.share * 100)} dari total`]
      : [title, shortMoney(total), countLabel(segments.length)];
  } else if (effective === "loading") {
    centerNode = (
      <div className="flex flex-col items-center gap-2" aria-hidden="true">
        <Skeleton width={72} height={14} />
        <Skeleton width={96} height={22} />
        <Skeleton width={64} height={12} />
      </div>
    );
  } else {
    centerNode = (
      <p className="text-center text-label text-text-2" style={{ maxWidth: maxW }}>
        {effective === "empty" ? "Belum ada pengeluaran" : "Gagal memuat"}
      </p>
    );
  }

  // Nominal tengah tidak pernah dipotong (§5.1, §7): jika format singkat masih lebih lebar dari 104 px
  // (mis. "Rp11,67 jt"), turun ke `type-stat` alih-alih ellipsis.
  const totalRef = useRef<HTMLSpanElement>(null);
  const compactTotal = useOverflow(totalRef, lines ? lines[1] : null, lines !== null);

  const lineClass = [
    "text-label text-text-2",
    "num text-donut-total text-text",
    "text-caption font-bold text-text-2",
  ];

  const ringFill = effective === "loading" ? "var(--surface-sunken)" : "var(--track)";
  const circumference = 2 * Math.PI * ring.rMid;

  return (
    <figure className={cn("m-0 flex flex-col items-center", className)}>
      <div
        className="relative"
        style={{ width: box, height: box, margin: -HIT_EXTRA }}
        onClick={(e: MouseEvent) => {
          // Ketuk area kosong → kembali ke total.
          if (!(e.target as Element).closest("[data-segment]") && selected !== null) select(null);
        }}
      >
        <svg width={box} height={box} viewBox={`0 0 ${box} ${box}`} role="img" aria-label={summary} className="absolute inset-0">
          {effective === "ready" ? (
            <>
              <defs>
                <mask id={maskId}>
                  {/* Sweep masuk 400 ms sekali saat mount (mati bila prefers-reduced-motion). */}
                  <circle
                    cx={c}
                    cy={c}
                    r={ring.rMid}
                    fill="none"
                    stroke="#fff"
                    strokeWidth={thickness + 2}
                    strokeDasharray={circumference}
                    transform={`rotate(-90 ${c} ${c})`}
                    className="animate-donut-sweep"
                    style={{ ["--sweep-length" as string]: circumference }}
                  />
                </mask>
              </defs>
              <g mask={`url(#${maskId})`}>
                {segments.map((s, i) => (
                  <path
                    key={s.label}
                    data-arc={i}
                    d={arcPath(c, c, ring.rMid, arcs[i].drawStart, arcs[i].drawEnd)}
                    fill="none"
                    stroke={catColor(s.cat)}
                    strokeWidth={thickness}
                    strokeLinecap="butt"
                    opacity={selected !== null && selected !== i ? DIM_OPACITY : 1}
                  />
                ))}
              </g>
              {focused !== null && arcs[focused] ? (
                <path
                  d={arcPath(c, c, diameter / 2 + 3, arcs[focused].drawStart, arcs[focused].drawEnd)}
                  fill="none"
                  stroke="var(--accent)"
                  strokeWidth={2}
                  aria-hidden="true"
                />
              ) : null}
            </>
          ) : (
            <circle cx={c} cy={c} r={ring.rMid} fill="none" stroke={ringFill} strokeWidth={thickness} className={effective === "loading" ? "animate-skeleton" : undefined} />
          )}
        </svg>

        {effective === "ready" && segments.length > 0 ? (
          <svg width={box} height={box} viewBox={`0 0 ${box} ${box}`} className="absolute inset-0" role="group" aria-label={`Segmen ${title}`}>
            {segments.map((s, i) => (
              <path
                key={s.label}
                ref={(el) => {
                  hitRefs.current[i] = el;
                }}
                data-segment={i}
                d={arcPath(c, c, ring.rMid + HIT_EXTRA / 2, arcs[i].start, arcs[i].end)}
                fill="none"
                stroke="transparent"
                strokeWidth={thickness + HIT_EXTRA}
                pointerEvents="stroke"
                tabIndex={0}
                role="button"
                aria-pressed={selected === i}
                aria-label={`${s.label}, ${hidden ? HIDDEN_ARIA_LABEL : formatRupiah(s.value)}, ${formatPercent(s.share * 100)}`}
                className="cursor-pointer outline-none"
                onClick={(e) => {
                  e.stopPropagation();
                  toggle(i);
                }}
                onKeyDown={(e) => onSegmentKey(e, i)}
                onFocus={() => setFocused(i)}
                onBlur={() => setFocused((f) => (f === i ? null : f))}
              />
            ))}
          </svg>
        ) : null}

        <div className="pointer-events-none absolute inset-0" aria-live="polite">
          {lines
            ? lines.map((text, i) => (
                <span
                  key={i}
                  ref={i === 1 ? totalRef : undefined}
                  data-center-line={i}
                  className={cn("absolute left-1/2 block -translate-x-1/2 truncate whitespace-nowrap text-center", i === 1 && compactTotal ? "num text-stat text-text" : lineClass[i])}
                  style={{ top: c + tops[i] + (i === 1 && compactTotal ? 3 : 0), maxWidth: maxW }}
                  aria-label={i === 1 && hidden ? HIDDEN_ARIA_LABEL : undefined}
                >
                  {text}
                </span>
              ))
            : null}
          {centerNode ? <div className="absolute inset-0 flex items-center justify-center">{centerNode}</div> : null}
        </div>
      </div>

      {effective === "error" && onRetry ? (
        <Button variant="ghost" onClick={onRetry} className="mt-3">
          Coba lagi
        </Button>
      ) : null}

      {segments.length > 0 ? (
        <table className="sr-only-table">
          <caption>{title}</caption>
          <thead>
            <tr>
              <th scope="col">Kategori</th>
              <th scope="col">Nominal</th>
              <th scope="col">Porsi</th>
            </tr>
          </thead>
          <tbody>
            {segments.map((s) => (
              <tr key={s.label}>
                <th scope="row">{s.label}</th>
                <td>{hidden ? HIDDEN_ARIA_LABEL : formatRupiah(s.value)}</td>
                <td>{formatPercent(s.share * 100)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : null}
    </figure>
  );
}
