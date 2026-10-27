import { formatRupiah } from "../../lib/money";
import {
  BOX,
  buildScale,
  decollideLabels,
  polyline,
  type ChartInput,
  type Point,
} from "./priceGeometry";

export interface PriceChartProps {
  ticker: string;
  closes: Point[];
  ma200: Point[];
  /** Pita nilai wajar datar (low–high); `null` bila belum terhitung. */
  band: { low: number; high: number } | null;
  avgPrice?: number | null;
  entryPrice?: number | null;
  /** Modal hanya ditampilkan bila emiten dipegang (`isHeld: true`). Default false (fail closed). */
  isHeld?: boolean;
}

const PILL_H = 18;
const GAP = PILL_H + 4;

function pillWidth(text: string, size: number): number {
  return Math.ceil(text.length * size * 0.58) + 16;
}

/**
 * Grafik harga Riset (plan §5.3, F6): close, MA200, pita nilai wajar datar tanpa gradient (token accent-soft),
 * garis acuan Modal (hanya bila dipegang) dan Incaran dengan label yang tidak bertabrakan (Acceptance M4-3). SVG murni.
 */
export function PriceChart({
  ticker,
  closes,
  ma200,
  band,
  avgPrice,
  entryPrice,
  isHeld = false,
}: PriceChartProps) {
  const showModal = isHeld && Boolean(avgPrice);
  const showIncaran = Boolean(entryPrice);

  const refs: ChartInput["refs"] = [];
  if (showModal && avgPrice) refs.push({ id: "modal", value: avgPrice });
  if (showIncaran && entryPrice) refs.push({ id: "incaran", value: entryPrice });
  const scale = buildScale({ closes, ma200, band, refs });
  if (!scale) return null;

  const dates = closes.map((p) => p[0]);
  const { left, width, right, top, height, bottom } = BOX;
  const plotR = width - right;
  const plotB = height - bottom;
  const lastClose = closes[closes.length - 1][1];
  const bandText = band ? `Nilai wajar ${formatRupiah(band.low, { bare: true })}–${formatRupiah(band.high, { bare: true })}` : "";

  // Label: pita di kiri atas pita, acuan rata kanan; semuanya dide-kolisi agar tidak bertabrakan di 320 px.
  const labels: Array<{ id: string; text: string; value: number; y: number; side: "left" | "right" }> = [];
  if (band) {
    labels.push({ id: "band", text: bandText, value: band.high, y: scale.y(band.high) + 8 + PILL_H / 2, side: "left" });
  }
  if (showModal && avgPrice) {
    labels.push({ id: "modal", text: `Modal ${formatRupiah(avgPrice, { bare: true })}`, value: avgPrice, y: scale.y(avgPrice) - PILL_H, side: "right" });
  }
  if (showIncaran && entryPrice) {
    labels.push({ id: "incaran", text: `Incaran ${formatRupiah(entryPrice, { bare: true })}`, value: entryPrice, y: scale.y(entryPrice) + PILL_H, side: "right" });
  }

  const ys = decollideLabels(labels.map((l) => l.y), GAP, top + PILL_H / 2, plotB - PILL_H / 2);
  const visibleLabels = labels
    .map((l, i) => ({ ...l, y: ys[i] }))
    .filter((l): l is typeof l & { y: number } => l.y !== null);

  const lastX = plotR;
  const lastY = scale.y(lastClose);
  const label = `Harga ${ticker}: close ${formatRupiah(lastClose, { bare: true })}${band ? `, ${bandText.toLowerCase()}` : ""}`;

  return (
    <svg viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="xMidYMid meet" role="img" aria-label={label} className="w-full h-auto block">
      {band ? (
        <rect
          x={left}
          y={scale.y(band.high)}
          width={plotR - left}
          height={Math.max(0, scale.y(band.low) - scale.y(band.high))}
          fill="var(--accent-soft)"
        />
      ) : null}

      {scale.ticks.map((t) => (
        <g key={t}>
          <line x1={left} x2={plotR} y1={scale.y(t)} y2={scale.y(t)} stroke="#E3E7EE" strokeDasharray="3 4" />
          <text x={left - 6} y={scale.y(t) + 4} textAnchor="end" fontSize="11" fill="#5A6472">
            {formatRupiah(t, { bare: true })}
          </text>
        </g>
      ))}

      <text x={left} y={height - 8} fontSize="11" fill="#5A6472" textAnchor="start">
        {dates[0]}
      </text>
      <text x={plotR} y={height - 8} fontSize="11" fill="#5A6472" textAnchor="end">
        {dates[dates.length - 1]}
      </text>

      {ma200.length > 1 ? (
        <polyline points={polyline(ma200, dates, scale)} fill="none" stroke="#8A94A6" strokeWidth="1.5" />
      ) : null}
      <polyline points={polyline(closes, dates, scale)} fill="none" stroke="#1961C4" strokeWidth="2.25" strokeLinejoin="round" strokeLinecap="round" />

      {showModal && avgPrice ? (
        <line x1={left} x2={plotR} y1={scale.y(avgPrice)} y2={scale.y(avgPrice)} stroke="#5A6472" strokeWidth="1.5" strokeDasharray="5 4" />
      ) : null}
      {showIncaran && entryPrice ? (
        <line x1={left} x2={plotR} y1={scale.y(entryPrice)} y2={scale.y(entryPrice)} stroke="#EE6535" strokeWidth="1.5" strokeDasharray="5 4" />
      ) : null}

      {visibleLabels.map((l) => {
        const w = pillWidth(l.text, 12);
        const x = l.side === "left" ? left + 6 : plotR - w;
        const stroke = l.id === "incaran" ? "#EE6535" : l.id === "modal" ? "#5A6472" : "none";
        const fill = l.id === "band" ? "#1961C4" : l.id === "modal" ? "#5A6472" : "#0A1A33";
        return (
          <g key={l.id}>
            <rect x={x} y={l.y - PILL_H / 2} width={w} height={PILL_H} rx={9} fill="#ffffff" stroke={stroke} />
            <text x={x + w / 2} y={l.y + 4.5} textAnchor="middle" fontSize="12" fontWeight="700" fill={fill}>
              {l.text}
            </text>
          </g>
        );
      })}

      <circle cx={lastX} cy={lastY} r="4" fill="#1961C4" stroke="#ffffff" strokeWidth="2" />
    </svg>
  );
}
