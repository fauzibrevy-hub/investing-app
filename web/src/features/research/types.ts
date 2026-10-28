/**
 * Tipe payload Riset canon v1 (docs/research/plan.md §5). Diturunkan dari serializer Rust di
 * `src/research/` (api.rs, api/r4.rs, packs/payload.rs, prices/payload.rs, valuation.rs,
 * calibration.rs, calendar.rs, events.rs, documents.rs, claims/, journal.rs, universe.rs), bukan
 * dari tabel di plan. Dua endpoint R5 (`GET /{ticker}`, `GET /compare`) mengikuti kontrak lane A1
 * di plan §5; field yang tidak bisa dipastikan dari kode Rust ditandai `// TBD`.
 * Angka persen adalah nilai persen (9,0 = 9 %), bukan pecahan.
 */

// ---- umum ----

export type Role = "held" | "manual" | "peer";
/** `YYYY-MM-DD`. */
export type DateText = string;
/** RFC 3339. */
export type Timestamp = string;
/** `[tanggal, nilai]`, seri harga dan driver. */
export type SeriesPoint = [DateText, number];

/** Label kuartal `2026Q2`; label tahunan `2026`. */
export interface QuarterRef {
  fy: number;
  q: number;
}

export interface LabelledQuarter extends QuarterRef {
  label: string;
}

/** Galat 422 per field dilepas `lib/api` sebagai `ApiError.fields`; tidak ada tipe tersendiri. */

// ---- fakta dan keuangan (R1) ----

export type CtxKind = "duration" | "instant";

/** Penanda fakta di filing: target `FactChip` dan kunci `GET /facts/{filing_id}/{element}`. */
export interface FactTarget {
  filing_id: string;
  element: string;
  ctx_kind: CtxKind;
  start: DateText | null;
  end: DateText;
  /** `axis=member;axis=member` atau kosong. */
  dims: string;
  /** `YYYY-MM-DD` (instan) atau `YYYY-MM-DD_YYYY-MM-DD` (durasi): nilai query `ctx`. */
  ctx: string;
}

/** Bendera sel turunan (`derive::Flag`). `gap` juga dipakai backend untuk sel tanpa baris. */
export type CellFlag =
  | "derived_fy"
  | "from_comparative"
  | "restated"
  | "gap"
  | "instant"
  | "reconcile_failed"
  | "basis_break"
  | "sign_conflict"
  | "per_share_approx";

export type FinancialsView = "standalone" | "ttm" | "annual";

/** Asal laporan: instans XBRL IDX, atau PDF asli yang dibaca di luar aplikasi dan diperiksa pemilik. */
export type SourceKind = "xbrl" | "pdf";

/** Halaman PDF dan label sebagaimana tercetak untuk fakta yang dibaca dari PDF; `null` untuk fakta XBRL. */
export interface FactCite {
  kind: "pdf";
  /** Halaman PDF, 1 dst (bukan nomor halaman cetak). */
  page: number;
  label: string;
}

export interface FinancialsPeriod extends QuarterRef {
  /** `2026Q2`, atau `2026` untuk tampilan tahunan. */
  label: string;
  end: DateText | null;
  filing_id: string | null;
  /** Asal laporan periode ini; `pdf` = laporan asli dari PDF (tanpa XBRL di IDX). */
  source_kind: SourceKind | null;
  audited: boolean | null;
  /** Kolom Q4 kuartalan = FY − 9 bulan; header memakai badge `turunan`. */
  derived_fy: boolean;
}

export interface FinancialsCell {
  value: number | null;
  flags: CellFlag[];
  /** Fakta pertama dari masukan; `null` untuk sel kosong. */
  fact: FactTarget | null;
  /** Jumlah fakta masukan. */
  inputs: number;
}

export interface FinancialsRow {
  id: string;
  label: string;
  element: string;
  /** Satuan sebagaimana dibawa fakta: `IDR`, `USD`, `USD/shares`, `pure`; `null` bila tak ada. */
  unit: string | null;
  cells: FinancialsCell[];
}

export interface FinancialsGroup {
  /** `laba_rugi`, `arus_kas`, `neraca` (bank: lihat plan §5 R1.5). */
  id: string;
  label: string;
  rows: FinancialsRow[];
}

export type ShareSource =
  | { kind: "filing"; filing_id: string; element: string; period_end: DateText }
  | { kind: "latest_instant"; filing_id: string; element: string; as_of: DateText }
  | { kind: "manual"; as_of: DateText; source_note: string; after_date: boolean }
  | { kind: "derived"; filing_id: string; period_end: DateText; numerator_element: string; eps_element: string };

export interface ShareCount {
  shares: number;
  source_label: string;
  source: ShareSource;
}

export interface FilingRow {
  id: string;
  ticker: string;
  fy: number;
  period: "Q1" | "Q2" | "Q3" | "FY";
  taxonomy: string;
  industry_code: string | null;
  period_start: DateText;
  period_end: DateText;
  audited: boolean;
  rounding: string;
  currency: string;
  filed_at: DateText | null;
  available_from: DateText;
  available_source: "declared" | "estimated";
  source_path: string;
  file_hash: string;
  source_kind: SourceKind;
  ingested_at: Timestamp;
  fact_count: number;
}

export interface FinancialsResponse {
  ticker: string;
  view: FinancialsView;
  rules_version: string;
  currency: string | null;
  rounding: string | null;
  taxonomy: string | null;
  periods: FinancialsPeriod[];
  groups: FinancialsGroup[];
  /** Satu per periode; `null` bila tak ada hitungan. */
  shares: Array<ShareCount | null>;
  filings: FilingRow[];
}

export interface FilingsResponse {
  items: FilingRow[];
  unparsed: Array<{ source_path: string; error: string; at: Timestamp }>;
}

export interface FactDerivationInput {
  role: string;
  value: number;
  fact: FactTarget;
  /** Halaman PDF bila operand ini dibaca dari PDF. */
  cite: FactCite | null;
}

export interface FactDerived {
  fy: number;
  q: number;
  view: FinancialsView;
  value: number | null;
  flags: CellFlag[];
  formula: string;
  inputs: FactDerivationInput[];
  rules_version: string;
}

/** `GET /facts/{filing_id}/{element}?ctx=&dims=`. */
export interface FactDetail {
  fact: {
    filing_id: string;
    element: string;
    label: { id: string; en: string };
    ctx_kind: CtxKind;
    start: DateText | null;
    end: DateText;
    dims: string;
    value: number;
    unit: string;
    decimals: number | null;
    cite: FactCite | null;
  };
  filing: FilingRow;
  /** Periode yang sama di setiap filing; `original` menandai filing asal fakta ini. */
  history: Array<{
    filing_id: string;
    value: number;
    decimals: number | null;
    source_kind: SourceKind | null;
    cite: FactCite | null;
    available_from: DateText | null;
    original: boolean;
  }>;
  restatements: Array<{
    original: number;
    original_filing: string;
    restated: number;
    restated_filing: string;
    detected_at: Timestamp;
  }>;
  derived: FactDerived[];
}

// ---- semesta dan paket (R2) ----

export interface PackChoice {
  id: string | null;
  source: "override" | "filing" | "none";
  filing_code: string | null;
  filing_default: string | null;
  override: string | null;
}

export interface UniverseItem {
  ticker: string;
  role: Role;
  peer_of: string | null;
  pack: PackChoice;
  business_note: string | null;
  added_at: Timestamp;
  synced_at: Timestamp;
  peers: string[];
  avg_price: number | null;
}

export interface UniverseResponse {
  as_of: Timestamp;
  items: UniverseItem[];
}

export type MetricLayer = "xbrl" | "kpi" | "driver";
export type MetricDirection = "higher" | "lower" | "neutral";
export type DimensionKind = "volume" | "pricing" | "cost" | "quality" | "balance" | "core";

/** Alasan sel formula kosong (`packs::eval::Gap`) dengan teks Indonesia dari backend. */
export interface PackGap {
  kind: "missing" | "division_by_zero" | "unit_mismatch" | "cycle" | "unresolved";
  text: string;
  ident?: string;
  q?: QuarterRef;
  view?: string;
  units?: string[];
  metric?: string;
  reference?: string;
}

export interface PackCellInput {
  ident: string;
  source: string;
  q: string;
  view: string;
  value: number;
}

/** Fakta dari laporan PDF yang menopang sel paket: filing, elemen, halaman PDF, dan label tercetak. */
export interface PackCellSource {
  kind: "pdf";
  filing_id: string;
  element: string;
  page: number;
  label: string;
}

export interface XbrlPackCell extends LabelledQuarter {
  value: number | null;
  gap: PackGap | null;
  flags: Array<"out_of_range">;
  inputs: PackCellInput[];
  /** Hanya ada bila sel bertumpu pada fakta dari PDF. */
  sources?: PackCellSource[];
}

export interface KpiProvenance {
  kind: "manual";
  document: string;
  document_date: DateText;
  entered_at: Timestamp;
}

export type KpiCellState = "entered" | "unknown" | "not_entered" | "not_applicable";

export interface KpiPackCell extends LabelledQuarter {
  state: KpiCellState;
  value?: number | null;
  unknown?: boolean;
  unit?: string;
  provenance?: KpiProvenance;
  flags?: Array<"out_of_range">;
  /** `Isi` bila `not_entered`. */
  action?: "Isi";
}

export type PackCell = XbrlPackCell | KpiPackCell;

/**
 * YoY menurut satu aturan (backend `yoy_json`, web `relativeChange`): `pct` = (sekarang − lalu) / |lalu| × 100,
 * `pp` = selisih poin untuk metrik `%`, `x` = selisih kali untuk rasio (kelipatan), `sign_change` = tanda melewati nol
 * sehingga tidak ada persentase.
 */
export type YoY = { kind: "pct" | "pp" | "x"; value: number } | { kind: "sign_change"; from: number; to: number };

export interface PackPercentile {
  value: number | null;
  n: number;
  years: number;
  /** `p41 · 10 th` atau `riwayat < 3 th`. */
  caption: string;
}

export interface PackPeerValue {
  ticker: string;
  fy: number | null;
  q: number | null;
  value: number | null;
  gap: PackGap | { kind: "not_entered"; text: string } | null;
}

export interface SeriesSummary {
  points: number;
  first: DateText;
  last: DateText;
  fetched_at: Timestamp;
}

export type DriverStatus = "TBD" | "belum diambil" | "tersedia";

export interface PackDriver {
  id: string;
  label: string;
  source: string;
  /** Id metrik paket yang dibandingkan dengan driver ini per kuartal; `null` bila tidak ada. */
  compare_metric: string | null;
  status: DriverStatus;
  unit: string | null;
  cadence: string | null;
  note: string | null;
  series: SeriesSummary | null;
  proxy: (Omit<PackDriver, "proxy" | "compare_metric"> & { proxy: true }) | null;
}

export interface PackMetric {
  id: string;
  label: string;
  layer: MetricLayer;
  unit: string;
  direction: MetricDirection;
  why: string;
  note: string | null;
  /** xbrl saja. */
  formula?: string;
  period?: "instant" | "standalone" | "ttm" | "average4q";
  bindings?: Record<string, string[]>;
  /** kpi saja. */
  cadence?: "quarterly" | "annual";
  /** driver saja. */
  driver?: PackDriver | null;
  range?: [number, number] | null;
  cells: PackCell[];
  latest: (LabelledQuarter & { value: number }) | null;
  yoy: YoY | null;
  /** Mengapa `yoy` kosong padahal kedua nilai ada, mis. `tidak bermakna: penyebut negatif`. */
  yoy_reason?: string | null;
  percentile: PackPercentile | null;
  peers: PackPeerValue[];
}

export interface PackDimension {
  /** `core` atau id paket industri. */
  pack: string;
  id: string;
  label: string;
  kind: DimensionKind;
  metrics: PackMetric[];
}

export type DataStateId = "lengkap" | "sebagian" | "basi";

export interface PackCoverage {
  has_pack: boolean;
  latest: QuarterRef;
  xbrl: { populated: number; total: number; pct: number | null };
  kpi: { entered: number; unknown: number; expected: number; missing: string[] };
  /** `basi` hanya datang dari Desk. */
  state: DataStateId;
}

/** `GET /{ticker}/pack`. */
export interface PackResponse {
  ticker: string;
  as_of: Timestamp;
  role: Role;
  business_note: string | null;
  pack: PackChoice & { version: string | null; label: string | null; action: "Tentukan paket" | null };
  core: { id: string; version: string };
  latest: LabelledQuarter & { from_filing: boolean };
  quarters: LabelledQuarter[];
  dimensions: PackDimension[];
  drivers: PackDriver[];
  missing_drivers: Array<{ id: string; label: string; proxy: string | null }>;
  peers: string[];
  coverage: PackCoverage;
}

/** Hasil `PUT /{ticker}/kpi/{metric}/{fy}/{q}`: sel dengan provenance. */
export interface KpiWriteResult {
  state: "entered" | "unknown";
  value: number | null;
  unknown: boolean;
  unit: string;
  fy: number;
  q: number;
  provenance: KpiProvenance;
  ticker: string;
  metric: string;
  pack: string;
}

export interface KpiWriteBody {
  value?: number;
  unknown?: boolean;
  document: string;
  document_date: DateText;
}

export interface PackDefinitionMetric {
  id: string;
  label: string;
  layer: MetricLayer;
  unit: string;
  direction: MetricDirection;
  why: string;
  note: string | null;
  formula: string | null;
  bindings: Record<string, string[]> | null;
  period: string | null;
  cadence: string | null;
  driver: string | null;
  range: [number, number] | null;
}

export interface PackDefinition {
  id: string;
  version: string;
  label: string;
  industry_codes: string[];
  taxonomies: string[];
  members: string[];
  dimensions: Array<{ id: string; label: string; kind: DimensionKind; metrics: PackDefinitionMetric[] }>;
  drivers: PackDriver[];
}

export interface PacksStatus {
  source: string;
  dir: string;
  files: number;
  loaded: number;
  invalid: Array<{ file: string; error: string }>;
  embedded_ids: string[];
  loaded_at: Timestamp;
  ok: boolean;
}

export interface DriverCatalogueEntry {
  id: string;
  label: string;
  unit: string;
  cadence: string;
  source: string | null;
  proxy_for: string | null;
  note: string;
  status: DriverStatus;
}

/** `GET /packs`. */
export interface PacksResponse {
  status: PacksStatus;
  packs: PackDefinition[];
  driver_catalogue: DriverCatalogueEntry[];
}

export interface PacksReloadResponse {
  status: PacksStatus;
  reloaded: number;
  rules: { version: string; source: string; error: string | null };
}

export interface RulesResponse {
  rules_version: string;
  derive_rules_version: string;
  discount_rates: number[];
  terminal_growth: number;
  horizon_years: number;
  cycle_windows: number[];
  min_cycle_years: number;
  percentile_window_years: number;
  percentile_min_years: number;
  pojk_offsets: { q1: number; q2: number; q3: number; fy: number };
  changelog: Array<{ version: string; date: DateText; note: string }>;
  source: string;
  file: string;
  error: string | null;
  loaded_at: Timestamp;
}

// ---- harga dan valuasi (R3) ----

export type PriceRange = "1y" | "3y" | "5y" | "10y";

export interface PricesDriver {
  id: string;
  label: string;
  source: string | null;
  /** Id metrik paket yang dibandingkan dengan driver ini per kuartal. */
  compare_metric: string | null;
  unit: string | null;
  cadence: string | null;
  note: string | null;
  status: DriverStatus;
  last: DateText | null;
  series: SeriesPoint[];
  /** Seri pengganti berlabel, bukan driver itu sendiri. */
  proxy: {
    id: string;
    label: string;
    source: string | null;
    unit: string | null;
    cadence: string | null;
    note: string | null;
    status: DriverStatus;
    last: DateText | null;
    series: SeriesPoint[];
    proxy: true;
    proxy_for: string | null;
  } | null;
}

export interface PriceLevel {
  id: string;
  /** Pada basis penutupan saat ini (disesuaikan split). */
  level: number;
  /** Angka yang ditulis pemilik. */
  entered: number;
  at: DateText;
  text: string;
  gap_pct: number | null;
}

export type PriceEvent =
  | { date: DateText; kind: "dividend"; amount: number }
  | { date: DateText; kind: "split"; ratio_num: number; ratio_den: number };

export interface PricesResponse {
  ticker: string;
  as_of: Timestamp;
  range: PriceRange;
  from: DateText;
  to: DateText | null;
  pack: string | null;
  price: SeriesPoint[];
  ma200: SeriesPoint[];
  drivers: PricesDriver[];
  drivers_plotted: number;
  index_relative: SeriesPoint[];
  index_symbol: string;
  levels: PriceLevel[];
  events: PriceEvent[];
  stats: {
    close: { value: number | null; date: DateText | null };
    high_52w: { value: number; date: DateText; drawdown_pct: number } | null;
    volatility_1y_pct: number | null;
    vs_index_6m_pct: number | null;
    dividend_yield_ttm_pct: number | null;
    avg_cost: { value: number; gap_pct: number | null } | null;
  };
  empty: boolean;
}

export interface MultipleBand {
  value: number | null;
  percentile: number | null;
  median: number | null;
  min: number | null;
  max: number | null;
  n: number;
  /** `riwayat < 3 th` bila riwayat kurang. */
  caption: string | null;
  cpi_adjusted: boolean | null;
}

/** `ttm` plus one band per `rules.cycle_windows` entry, keyed `cycle<N>y` (`cycle5y`, `cycle10y` under the canon rules). */
export type MultipleSet = { ttm: MultipleBand } & { [window: string]: MultipleBand };

export interface ReverseDcfRow {
  discount_rate: number;
  /** Persen per tahun; `null` bila tidak berlaku. */
  implied_growth: number | null;
}

export type ScenarioName = "bear" | "base" | "bull";

export interface ScenarioRow {
  name: ScenarioName;
  growth: number | null;
  margin: number | null;
  exit_multiple: number | null;
  years: number | null;
  value_per_share: number | null;
  gap_pct: number | null;
  updated_at: Timestamp | null;
  /** `Isi skenario` untuk baris kosong. */
  action: "Isi skenario" | null;
}

/**
 * Satu baris skenario untuk `PUT /{ticker}/scenarios`. Field `null` (atau absen) disimpan kosong: baris parsial
 * diterima dan nilai per saham muncul setelah keempatnya ada (api.rs `scenarios_put`).
 */
export interface ScenarioInput {
  growth: number | null;
  margin: number | null;
  exit_multiple: number | null;
  years: number | null;
}

export interface ScenariosBody {
  bear?: ScenarioInput | null;
  base?: ScenarioInput | null;
  bull?: ScenarioInput | null;
}

export interface ScenarioTable {
  discount_rate: number;
  rows: ScenarioRow[];
}

/** `GET /{ticker}/valuation?fcf=ttm|avg5y`. */
export interface ValuationResponse {
  ticker: string;
  as_of: Timestamp;
  rules_version: string;
  currency: string;
  /** Selalu ada (valuation.rs): kurs yang dipakai untuk emiten berpelaporan USD, `null` untuk IDR atau bila driver `usd_idr` belum ada. */
  fx_usd_idr: number | null;
  /** Selalu ada; `close` dan `date` null bila emiten belum punya harga. */
  price: { close: number | null; date: DateText | null };
  /** Selalu ada; `null` bila jumlah saham tidak bisa ditentukan (valuation.rs). */
  shares: { count: number; source: ShareSource; source_label: string } | null;
  market_cap: number | null;
  latest_quarter: (LabelledQuarter & { available_from: DateText }) | null;
  history: { from: DateText | null; to: DateText | null; days: number };
  reverse_dcf: {
    fcf_base: "ttm" | "avg5y";
    fcf: number | null;
    fcf_ttm: number | null;
    fcf_avg5y: number | null;
    horizon_years: number;
    terminal_growth: number;
    rows: ReverseDcfRow[];
    applicable: boolean;
    reason: string | null;
  };
  multiples: { pe: MultipleSet; pb: MultipleSet; ev_ebitda: MultipleSet; dividend_yield: MultipleSet };
  earnings_yield: {
    earnings_yield: number | null;
    bond_yield: number | null;
    /** `tersedia` bila driver obligasi punya nilai pada tanggal harga, selain itu `TBD` (valuation.rs). */
    bond_status: "tersedia" | "TBD";
    bond_driver: string | null;
    spread: number | null;
    spread_band: MultipleBand | null;
  };
  scenarios: ScenarioTable;
}

export interface ScenariosPutResponse {
  ticker: string;
  price: ValuationResponse["price"];
  shares: ValuationResponse["shares"];
  scenarios: ScenarioTable;
  rules_version: string;
}

// ---- klaim, jurnal, dokumen (R4) ----

export type ClaimStatus = "holding" | "broken" | "not_testable" | "expired";
export type Comparator = "<" | "<=" | ">" | ">=" | "between";

export interface ClaimLastEval {
  evaluated_at: Timestamp;
  status: ClaimStatus;
  status_label: string | null;
  value: number | null;
}

export interface Claim {
  id: string;
  ticker: string;
  statement: string;
  metric: string;
  metric_label: string;
  unit: string;
  comparator: Comparator;
  threshold: number;
  threshold_hi: number | null;
  /** `latest`, `at_due`, `every_quarter` atau `quarter:YYYYQn`. */
  window: string;
  due: DateText;
  status: ClaimStatus;
  /** Berlaku · Patah · Belum teruji · Kedaluwarsa. */
  status_label: string | null;
  created_at: Timestamp;
  closed_at: Timestamp | null;
  /** Kalimat "Akan dievaluasi saat …". */
  preview: string | null;
  last_eval: ClaimLastEval | null;
  evidence_count: number;
}

export interface ClaimMetricOption {
  /** `<paket>.<metrik>` atau `price.close`, `price.ma200`, `price.vs_ma200_pct`. */
  id: string;
  label: string;
  unit: string;
  layer: MetricLayer | "price";
  pack: string | null;
  quarterly: boolean;
}

export type ClaimCounts = Record<ClaimStatus, number>;

export interface ClaimsResponse {
  ticker: string;
  items: Claim[];
  counts: ClaimCounts;
  metrics: ClaimMetricOption[];
  windows: string[];
  comparators: Comparator[];
}

export interface ClaimCreateBody {
  statement: string;
  metric: string;
  comparator: Comparator;
  threshold: number;
  threshold_hi?: number | null;
  window: string;
  due: DateText;
}

export interface ClaimEval {
  status: ClaimStatus;
  label: string | null;
  evaluated_at: Timestamp;
  value: number | null;
  /** Jendela, tanggal acuan, observasi yang dipakai; bentuk bebas dari `claims::Evaluation`. */
  inputs: unknown; // TBD
}

export interface ClaimEvalsResponse {
  claim_id: string;
  ticker: string;
  statement: string;
  items: ClaimEval[];
}

export type JournalKind = "catatan" | "level" | "keputusan" | "sistem";

export interface JournalAnnotation {
  id: string;
  at: Timestamp;
  text: string;
}

export interface JournalEntry {
  id: string;
  ticker: string;
  at: Timestamp;
  kind: JournalKind;
  text: string;
  price_at: number | null;
  level: number | null;
  annotations: JournalAnnotation[];
}

export interface JournalResponse {
  ticker: string;
  items: JournalEntry[];
  total: number;
  limit: number;
  offset: number;
}

export interface JournalCreateBody {
  /** `sistem` ditolak API (422). */
  kind: Exclude<JournalKind, "sistem">;
  text: string;
  level?: number | null;
}

export type DocumentKind = "presentasi" | "update_operasional" | "laporan" | "artikel" | "catatan" | "lainnya";

export interface ResearchDocument {
  id: string;
  ticker: string;
  kind: DocumentKind;
  kind_label: string;
  url: string | null;
  text: string | null;
  doc_date: DateText | null;
  added_at: Timestamp;
  evidence: Array<{ claim_id: string | null; kpi_key: string | null }>;
}

export interface DocumentsResponse {
  ticker: string;
  items: ResearchDocument[];
  kinds: Array<{ id: DocumentKind; label: string }>;
}

export interface DocumentCreateBody {
  kind: DocumentKind;
  url?: string | null;
  text?: string | null;
  doc_date?: DateText | null;
  /** Paling banyak satu target. `kpi_key` = `<metrik>:<FY>Q<q>`. */
  claim_id?: string;
  kpi_key?: string;
}

// ---- R8: ekstraksi KPI dari dokumen (unggahan, usulan, terima atau tolak) ----

/** `superseded`: digantikan oleh usulan lain yang diterima (atau penggantian); tidak bisa diputuskan lagi. */
export type ProposalStatus = "pending" | "accepted" | "rejected" | "superseded";
export type UploadFormat = "pdf" | "pptx";
/** `page` untuk PDF, `slide` untuk PPTX. */
export type ProposalLocator = "page" | "slide";

export interface ProposalCounts {
  pending: number;
  accepted: number;
  rejected: number;
  superseded: number;
}

/** Satu nilai KPI yang dibaca dari dokumen; tidak tersimpan sebagai KPI sebelum pemilik menerimanya. */
export interface ExtractProposal {
  id: string;
  document_id: string;
  ticker: string;
  /** `<metrik>:<FY>Q<q>`. */
  kpi_key: string;
  metric: string;
  metric_label: string;
  fy: number;
  q: number;
  value: number;
  /** Satuan metrik paket (`Mt`, `USD/t`, `ccy`, `%` …). */
  unit: string;
  page: number;
  locator: ProposalLocator;
  /** Paling banyak 160 karakter. */
  snippet: string;
  rank: number;
  status: ProposalStatus;
  created_at: Timestamp;
  decided_at: Timestamp | null;
}

export interface ProposalDocument {
  id: string;
  kind: DocumentKind;
  kind_label: string;
  doc_date: DateText | null;
  added_at: Timestamp;
  format: UploadFormat | null;
  locator: ProposalLocator;
}

/** `GET /{ticker}/proposals`. */
export interface ProposalsResponse {
  ticker: string;
  document_id: string | null;
  items: ExtractProposal[];
  counts: ProposalCounts;
  documents: ProposalDocument[];
}

/** `POST /{ticker}/documents/upload`: 201 baru, 200 `created: false` untuk berkas yang sama pada periode yang sama. */
export interface UploadResponse {
  created: boolean;
  document: ResearchDocument;
  upload: { sha256: string; format: UploadFormat; bytes: number; pages: number; pages_with_text: number; locator: ProposalLocator };
  period: { fy: number; q: number; label: string };
  pack: { id: string; label: string } | null;
  proposals: ExtractProposal[];
  counts: ProposalCounts;
}

/** `POST /proposals/{id}/accept`. */
export interface ProposalAcceptResponse {
  proposal: ExtractProposal;
  kpi: KpiWriteResult;
  evidence: { document_id: string; kpi_key: string };
  replaced: boolean;
}

export interface EvidenceBody {
  document_id: string;
  claim_id?: string;
  kpi_key?: string;
}

export interface EvidenceLink {
  document_id: string;
  claim_id: string | null;
  kpi_key: string | null;
  created: boolean;
}

export interface ClaimEvidenceResponse {
  claim_id: string;
  ticker: string;
  items: ResearchDocument[]; // TBD: bentuk item `GET /claims/{id}/evidence` mengikuti `document_json`
}

// ---- Desk: event, cakupan, kalender (R4) ----

export type EventKind =
  | "filing_new"
  | "claim_status"
  | "kpi_due"
  | "level_crossed"
  | "restatement"
  | "stale_data"
  | "accounting_flag";

/** Chip filter Desk. */
export type EventGroup = "laporan" | "klaim" | "kpi" | "level" | "data" | "bendera";
export type EventFilter = "semua" | EventGroup;

/** Bagian dossier yang dibuka **Buka**. `claims` = panel kanan. */
export type EventSection = "financials" | "claims" | "pack" | "price" | "sources" | "overview" | "valuation";

export interface ResearchEvent {
  id: string;
  at: Timestamp;
  ticker: string | null;
  kind: EventKind;
  group: EventGroup | null;
  section: EventSection | null;
  /** Kalimat fakta; dibangun backend saat baca. */
  text: string | null;
  age_days: number | null;
  dismissed_at: Timestamp | null;
  payload: unknown; // TBD: bentuk bebas per `kind`
}

export interface CalendarItem {
  date: DateText;
  ticker: string;
  /** `pojk`, `dividend` atau jenis manual (`rups`, `dividen`, `laporan`, `paparan`, `lainnya`). */
  kind: string;
  label: string;
  source: "pojk" | "corp_events" | "manual";
  window: { start: DateText; end: DateText } | null;
  projected: boolean;
  overdue: boolean;
  amount: number | null;
  note: string | null;
}

export interface CoverageRow {
  ticker: string;
  role: Role;
  peer_of: string | null;
  business_note: string | null;
  pack: { id: string | null; label: string | null; source: PackChoice["source"]; action: "Tentukan paket" | null };
  last_seen: Timestamp | null;
  claims: ClaimCounts;
  xbrl: PackCoverage["xbrl"];
  kpi: PackCoverage["kpi"];
  next_report: { label: string; deadline: DateText; overdue: boolean } | null;
  data: { state: DataStateId | null; label: string | null; reasons: string[]; has_pack: boolean };
}

export interface DeskResponse {
  as_of: Timestamp;
  inbox: ResearchEvent[];
  inbox_total: number;
  filters: Array<{ group: EventFilter; label: string; count: number | null }>;
  coverage: CoverageRow[];
  calendar: CalendarItem[];
  /** Hari sejak penutupan, laporan, dan tiap driver. */
  ages: {
    prices: Record<string, number | null>;
    filings: Record<string, number | null>;
    drivers: Record<string, number>;
    /** Label Indonesia tiap driver dari katalog (`bi_rate` → `BI rate`); id dipakai bila label tidak ada. */
    driver_labels?: Record<string, string>;
  };
}

export interface CalendarResponse {
  ticker: string;
  days: number;
  items: CalendarItem[];
}

export interface CalendarCreateBody {
  date: DateText;
  kind: string;
  note?: string | null;
}

export interface CalendarRow {
  ticker: string;
  date: DateText;
  kind: string;
  note: string | null;
}

// ---- kalibrasi (R4.6) ----

export type CalibrationKind = "klaim" | "level" | "keputusan";

export interface CalibrationRow {
  id: string;
  ticker: string;
  kind: CalibrationKind;
  kind_label: string;
  date: DateText;
  text: string;
  price_then: number | null;
  price_now: number | null;
  price_now_date: DateText | null;
  days: number;
  matured: boolean;
  matures_on: DateText;
  return_pct: number | null;
  level: number | null;
  reached: boolean | null;
  /** Status klaim, atau `Tercapai` / `Belum tercapai` untuk level. */
  status: string | null;
  reason: string | null;
}

export interface HitRate {
  ticker: string;
  holding: number;
  broken: number;
  undecided: number;
  rate_pct: number | null;
  text: string;
}

export interface CalibrationResponse {
  as_of: Timestamp;
  ticker: string | null;
  type: CalibrationKind | null;
  maturity_days: number;
  total: number;
  matured: number;
  pending: number;
  items: CalibrationRow[];
  hit_rates: HitRate[];
}

// ---- kontrak R5 (lane A1): dossier dan compare ----

export interface DossierChange {
  id: string;
  kind: EventKind;
  at: Timestamp;
  text: string;
  section: EventSection | null;
  dismissed: boolean;
}

/** `GET /{ticker}`: header dossier + ringkasan (plan §5, kontrak R5). */
export interface DossierResponse {
  ticker: string;
  name: string | null;
  as_of: Timestamp;
  role: Role;
  peer_of: string | null;
  avg_price: number | null;
  pack: { id: string | null; label: string | null; version: string | null; source: PackChoice["source"]; action: "Tentukan paket" | null };
  business_note: string | null;
  latest_filing: {
    id: string;
    fy: number;
    period: FilingRow["period"];
    available_from: DateText;
    age_days: number;
    audited: boolean;
    taxonomy: string;
  } | null;
  latest_close: { date: DateText; close: number; age_days: number } | null;
  seen_at: Timestamp | null;
  claims: ClaimCounts;
  coverage: PackCoverage;
  changes: DossierChange[];
}

export interface CompareCell {
  ticker: string;
  value: number | null;
  period: string | null;
  percentile: number | null;
  state: "value" | "gap" | "unknown";
  fact: FactTarget | null;
  /** Mengapa sel kosong: `not_defined`, `not_entered` atau teks evaluator. */
  reason: string | null;
  flags: string[];
}

export interface CompareRow {
  /** `core` atau id dimensi. */
  group: string;
  group_label: string;
  metric: { id: string; label: string; unit: string; layer: MetricLayer; direction: MetricDirection; period?: PackMetric["period"] | null };
  cells: CompareCell[];
}

/** `GET /compare?pack=&tickers=`. */
export interface CompareResponse {
  as_of: Timestamp;
  pack: { id: string; label: string; version: string } | null;
  /** `currency`: mata uang laporan terbaru emiten itu; `null` sampai ada laporan. */
  tickers: Array<{ ticker: string; name: string | null; role: Role; currency?: string | null }>;
  /** Daftar bawaan dipotong pada 12 emiten. */
  truncated: boolean;
  rows: CompareRow[];
}

// ---- tulis universe ----

export interface UniverseCreateBody {
  ticker: string;
  role?: "manual" | "peer";
  peer_of?: string;
  pack_override?: string | null;
  business_note?: string | null;
}

export interface UniverseUpdateBody {
  /** `null` menghapus. */
  pack_override?: string | null;
  business_note?: string | null;
  /** Mengganti daftar; paling banyak tiga. */
  peers?: string[];
}

export interface SharesBody {
  shares: number;
  as_of: DateText;
  source_note: string;
}

export interface SharesWriteResult {
  ticker: string;
  shares: number;
  as_of: DateText;
  source_note: string;
  entered_at: Timestamp;
}

export interface RefreshResult {
  status: "accepted";
}

// ---- cek emiten untuk wizard tambah (`GET /lookup`, design.md §4.5) ----

export interface LookupItem {
  ticker: string;
  name: string | null;
  /** `null`: daftar bursa tidak terbaca, kode belum bisa diperiksa. */
  listed: boolean | null;
  /** Kode industri dari laporan terakhir; `null` bila belum ada laporan. */
  industry_code: string | null;
  /** Paket yang mengklaim kode itu; `null` bila tidak ada. */
  pack: { id: string; label: string } | null;
  in_universe: boolean;
  role: Role | null;
}

export interface LookupResponse {
  as_of: Timestamp;
  query: string;
  list_available: boolean;
  /** Kecocokan persis untuk empat huruf; `null` bila tidak ada di daftar bursa. */
  exact: LookupItem | null;
  items: LookupItem[];
}

// ---- peta struktur bisnis (`/{ticker}/structure`, ui-slicing.md §7 G1) ----

export type StructureKind =
  | "segmen"
  | "produk"
  | "pelanggan"
  | "pendorong_harga"
  | "biaya"
  | "kepemilikan"
  | "lainnya";

/** `proposed` = usulan dari draf, belum diputuskan pemilik. */
export type StructureState = "proposed" | "accepted" | "rejected";

/** Asal simpul usulan: halaman (atau slide) dokumen dan kutipan persis, maks. 160 karakter. */
export interface StructureSource {
  document: string;
  page: number;
  snippet: string;
}

export interface StructureNode {
  id: string;
  /** `null` untuk anak langsung dari akar. */
  parent: string | null;
  label: string;
  kind: StructureKind;
  kind_label: string;
  /** Id metrik paket atau core yang nilai terakhirnya ditampilkan di simpul. */
  metric: string | null;
  metric_label: string | null;
  note: string | null;
  state: StructureState;
  state_label: "usulan" | "diterima" | "ditolak";
  /** Ditambahkan pemilik (langsung diterima, tanpa sumber). */
  owner: boolean;
  source: StructureSource | null;
}

export interface StructureMetricChoice {
  id: string;
  label: string;
  unit: string;
  /** `false` untuk metrik core. */
  pack: boolean;
}

export interface StructureResponse {
  ticker: string;
  pack: { id: string; label: string } | null;
  metrics: StructureMetricChoice[];
  kinds: Array<{ id: StructureKind; label: string }>;
  nodes: StructureNode[];
  counts: { proposed: number; accepted: number; rejected: number };
  updated_at: Timestamp | null;
  limits: { max_depth: number; max_nodes: number; max_label: number; max_note: number };
  draft: { file: string; available: boolean };
}

/** `POST /{ticker}/structure/nodes` menambahkan `node`. */
export interface StructureNodeResult extends StructureResponse {
  node: StructureNode;
}

/** `POST /{ticker}/structure/draft` menambahkan hitungan impor. */
export interface StructureDraftResult extends StructureResponse {
  imported: number;
  replaced: number;
}

/** Badan `PUT .../nodes/{id}`: kolom yang tidak dikirim tidak berubah; `null` menghapus metrik atau catatan. */
export interface StructureNodePatch {
  label?: string;
  kind?: StructureKind;
  metric?: string | null;
  note?: string | null;
  state?: "accepted" | "rejected";
}

/** Badan `POST .../nodes`: simpul milik pemilik. */
export interface StructureNodeCreate {
  label: string;
  kind?: StructureKind;
  parent?: string | null;
  metric?: string | null;
  note?: string | null;
}
