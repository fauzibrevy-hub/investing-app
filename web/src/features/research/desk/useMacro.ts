import { formatNumber } from "../../../lib/numbers";
import { useResource } from "../../../lib/useResource";

/** Bentuk `GET /api/macro/observations` (= modul macro_watch). Hanya field yang dipakai Desk. */
export interface Observation {
  metric_id: string;
  name: string;
  pillar: string;
  unit: string;
  value: number | null;
  observed_at: string;
  fetched_at: string;
  freshness: string;
  upstream_ok: boolean;
  history?: { date: string; value: number }[];
}

export interface ObservationsResponse {
  as_of: string;
  observations: Observation[];
}

/** Satu-satunya pembaca data makro di fitur Riset: endpoint dan cache bersama layar lain, tanpa fixture saat runtime. */
export const useMacroObservations = () => useResource<ObservationsResponse>("/api/macro/observations");

/** Nilai sebelumnya = titik history terakhir sebelum `observed_at` (history tidak dianggap terurut). */
export function previousValue(o: Observation): number | null {
  const day = o.observed_at.slice(0, 10);
  const before = (o.history ?? []).filter((p) => p.date < day).sort((x, y) => (x.date < y.date ? -1 : x.date > y.date ? 1 : 0));
  return before.length ? before[before.length - 1].value : null;
}

/**
 * Indikator yang naik ke kartu Desk lebih dulu (id dari registry macro): minyak, kurs, yield, dolar, inflasi Indonesia,
 * volatilitas. Sisanya mengikuti urutan backend di balik "Tampilkan semua"; id yang tak ada dilewati.
 */
export const KEY_METRICS = ["brent_front_month", "usd_idr", "ust_10y", "dxy", "id_core_cpi", "vix"] as const;

export function orderForDesk(list: Observation[]): Observation[] {
  const rank = (o: Observation) => {
    const i = (KEY_METRICS as readonly string[]).indexOf(o.metric_id);
    return i === -1 ? KEY_METRICS.length : i;
  };
  // Urutan stabil: Array.prototype.sort mempertahankan urutan backend untuk peringkat yang sama.
  return [...list].sort((a, b) => rank(a) - rank(b));
}

/** Satuan yang ikut angka di baris; kurs jelas dari nama indikator (satuannya dibacakan lewat `unitWords`). */
const SHORT_UNIT: Record<string, string> = { percent: "%", percent_yoy: "% yoy", "%": "%", bps: "bps", "USD/barel": "USD/barel", index_points: "poin" };

export const shortUnit = (unit: string): string => SHORT_UNIT[unit] ?? "";

/** Satuan dalam kata, untuk pembaca layar dan sheet; enum backend tidak sampai ke layar. */
const UNIT_WORDS: Record<string, string> = {
  percent: "persen",
  percent_yoy: "persen tahun ke tahun",
  "%": "persen",
  bps: "basis poin",
  index_points: "poin indeks",
  "IDR/USD": "rupiah per dolar AS",
  "USD/barel": "dolar AS per barel",
};
export const unitWords = (unit: string): string => UNIT_WORDS[unit] ?? unit;

const FRESHNESS_WORDS: Record<string, string> = { fresh: "baru", delayed: "tertunda", unavailable: "tidak tersedia" };
export const freshnessWords = (freshness: string): string => FRESHNESS_WORDS[freshness] ?? freshness;

const DAY_MS = 86_400_000;
/** Lebih lama dari ini terhadap `as_of`, baris menampilkan tanggal pengamatannya (deret bulanan tidak tampak seperti harian). */
export const STALE_DAYS = 7;

/** `YYYY-MM-DD` pengamatan bila sudah lebih dari `STALE_DAYS` hari di belakang `as_of`; null untuk nilai kosong (epoch). */
export function staleSince(o: Observation, asOf: string): string | null {
  if (o.value === null) return null;
  const obs = Date.parse(o.observed_at.slice(0, 10));
  const ref = Date.parse(asOf.slice(0, 10));
  return Number.isFinite(obs) && Number.isFinite(ref) && ref - obs > STALE_DAYS * DAY_MS ? o.observed_at.slice(0, 10) : null;
}

/** `111,41` untuk nilai (minus U+2212, bukan tanda hubung); `—` bila belum ada data. Satuan dipisah supaya sel tetap satu baris dan "Rp" tak berulang. */
export const valueText = (value: number | null): string => (value === null ? "—" : formatNumber(value).replace(/^-/, "−"));

/** Selisih dalam kata untuk pembaca layar: tanda +/− sering tak dibacakan, jadi "plus" dan "minus" (warna tetap netral). */
export const changeWords = (o: Observation): string => {
  const t = changeText(o);
  return t === "—" ? "tidak tersedia" : t.replace(/^\+/, "plus ").replace(/^−/, "minus ");
};

/** Selisih bertanda terhadap nilai sebelumnya, `—` bila salah satunya tak ada. Arah ditandai tanda +/−, bukan warna. */
export function changeText(o: Observation): string {
  const prev = previousValue(o);
  if (o.value === null || prev === null) return "—";
  const d = Math.round((o.value - prev) * 100) / 100;
  return `${d > 0 ? "+" : d < 0 ? "−" : ""}${formatNumber(Math.abs(d))}`;
}
