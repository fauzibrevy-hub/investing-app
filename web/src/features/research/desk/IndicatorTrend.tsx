import { TrendChart } from "../../../components/charts/TrendChart";
import { formatNumber } from "../../../lib/numbers";
import { MONTHS_ID_SHORT, formatDataTime } from "../../../lib/time";
import { freshnessWords, shortUnit, unitWords, type Observation } from "./useMacro";

/** "2026-10-20" → "20 Okt". */
export const dayLabel = (iso: string): string => {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso);
  return m ? `${Number(m[3])} ${MONTHS_ID_SHORT[Number(m[2]) - 1]}` : iso;
};

/** Isi sheet satu indikator: pilar, satuan, kesegaran, tren history, dan waktu ambil. Asal angka yang tampil di baris Desk. */
export function IndicatorTrend({ o }: { o: Observation }) {
  const pts = (o.history ?? []).map((p, i) => ({ x: i, value: p.value, label: dayLabel(p.date) }));
  return (
    <div className="flex flex-col gap-2">
      {/* Nama penuh: di baris Desk dan judul sheet nama dapat terpotong di layar sempit. */}
      <p className="break-words text-body-strong text-text">{o.name}</p>
      <p className="text-label text-text-2">{`${o.pillar} · ${unitWords(o.unit)} · ${freshnessWords(o.freshness)}`}</p>
      {o.value !== null ? <p className="text-label text-text-2">{`Diamati ${dayLabel(o.observed_at)}`}</p> : null}
      {pts.length > 0 ? (
        <TrendChart
          size="mobile"
          current={pts}
          currentLabel={o.name}
          formatValue={(v) => `${formatNumber(v)} ${shortUnit(o.unit) || unitWords(o.unit)}`}
          formatAxis={(v) => formatNumber(Math.round(v * 10) / 10)}
        />
      ) : (
        <p className="text-label text-text-2">Belum ada riwayat nilai</p>
      )}
      <p className="text-caption text-text-2">{`Diambil ${formatDataTime(new Date(o.fetched_at))}`}</p>
    </div>
  );
}
