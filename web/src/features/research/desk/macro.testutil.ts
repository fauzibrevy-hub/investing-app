import type { Observation, ObservationsResponse } from "./useMacro";

/** Observasi tiruan bentuk `/api/macro/observations` (nilai karangan; bukan data pemilik). Hanya untuk test dan e2e. */
const base: Omit<Observation, "metric_id" | "name" | "unit" | "value" | "history"> = {
  pillar: "Energi",
  observed_at: "2026-10-20",
  fetched_at: "2026-10-20T09:20:51Z",
  freshness: "fresh",
  upstream_ok: true,
};

export const OIL: Observation = {
  ...base,
  metric_id: "oil_front",
  name: "Minyak mentah kontrak terdekat",
  unit: "USD/barel",
  value: 80.5,
  history: [
    { date: "2026-10-16", value: 79.25 },
    { date: "2026-10-19", value: 78.65 },
    { date: "2026-10-20", value: 80.5 },
  ],
};
export const RATE: Observation = {
  ...base,
  metric_id: "policy_rate",
  name: "Suku bunga acuan",
  pillar: "Moneter",
  unit: "percent",
  value: 4.75,
  history: [{ date: "2026-09-18", value: 5 }, { date: "2026-10-20", value: 4.75 }],
};
export const FX: Observation = {
  ...base,
  metric_id: "fx_idr",
  name: "Kurs rupiah terhadap dolar AS",
  pillar: "Kurs",
  unit: "IDR/USD",
  value: 15250,
  history: [{ date: "2026-10-19", value: 15300 }, { date: "2026-10-20", value: 15250 }],
};
/** Nilai kosong datang dengan `observed_at` epoch: tak boleh tampil sebagai tanggal. */
export const EMPTY_OBS: Observation = { ...base, metric_id: "core_cpi", name: "Inflasi inti", pillar: "Harga", unit: "percent_yoy", value: null, observed_at: "1970-01-01", history: [] };

export const MACRO: ObservationsResponse = { as_of: "2026-10-20T09:21:52Z", observations: [OIL, RATE, FX, EMPTY_OBS] };

/** Satu observasi tambahan per indeks, untuk menguji lipatan "Tampilkan semua". */
export const manyObservations = (n: number): ObservationsResponse => ({
  as_of: MACRO.as_of,
  observations: Array.from({ length: n }, (_, i) => ({ ...OIL, metric_id: `m${i}`, name: `Indikator ${i + 1}`, value: i + 1, history: [] })),
});
