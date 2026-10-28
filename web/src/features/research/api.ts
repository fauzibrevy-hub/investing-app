import { newIdempotencyKey, request } from "../../lib/api";
import { invalidate, useResource, type Resource } from "../../lib/useResource";
import type {
  CalendarCreateBody,
  CalendarResponse,
  CalendarRow,
  CalibrationKind,
  CalibrationResponse,
  Claim,
  ClaimCreateBody,
  ClaimEvalsResponse,
  ClaimEvidenceResponse,
  ClaimsResponse,
  CompareResponse,
  DeskResponse,
  DocumentCreateBody,
  DocumentsResponse,
  DossierResponse,
  EvidenceBody,
  EvidenceLink,
  FactDetail,
  FactTarget,
  FilingsResponse,
  FinancialsResponse,
  FinancialsView,
  JournalAnnotation,
  JournalCreateBody,
  JournalEntry,
  JournalResponse,
  KpiWriteBody,
  KpiWriteResult,
  PackResponse,
  PacksReloadResponse,
  PacksResponse,
  PriceRange,
  PricesResponse,
  ProposalAcceptResponse,
  ProposalsResponse,
  UploadResponse,
  RefreshResult,
  ResearchDocument,
  ResearchEvent,
  RulesResponse,
  ScenariosBody,
  ScenariosPutResponse,
  SharesBody,
  SharesWriteResult,
  StructureDraftResult,
  StructureNodeCreate,
  StructureNodePatch,
  StructureNodeResult,
  StructureResponse,
  LookupResponse,
  UniverseCreateBody,
  UniverseItem,
  UniverseResponse,
  UniverseUpdateBody,
  ValuationResponse,
} from "./types";

export const BASE = "/api/research";

/** Hasil baca: `loading | ready | error` + `reload`. Gagal tetap gagal: tidak ada data pengganti. */
export type ResearchResource<T> = Resource<T>;

const enc = (segment: string) => encodeURIComponent(segment);
const encTicker = (ticker: string) => enc(ticker.toUpperCase());
/** `null` menunda pemuatan (menunggu ticker atau target dipilih). */
const at = (ticker: string | null, rest: string): string | null => (ticker ? `${BASE}/${encTicker(ticker)}${rest}` : null);

// ---- baca (satu entri cache per URL, dibagi antar layar; `useResource` di lib) ----

export const useDesk = (ticker?: string): ResearchResource<DeskResponse> =>
  useResource<DeskResponse>(`${BASE}/desk`, { ticker: ticker?.toUpperCase() });

export const useUniverse = (): ResearchResource<UniverseResponse> => useResource<UniverseResponse>(`${BASE}/universe`);

/** Wizard tambah: cek kode dan cari pembanding. `null` (tanpa kata kunci) menunda pemuatan. */
export const useLookup = (q: string | null, limit?: number) => useResource<LookupResponse>(q ? `${BASE}/lookup` : null, { q: q ?? undefined, limit });

export const useDossier = (ticker: string | null) => useResource<DossierResponse>(at(ticker, ""));

export const usePack = (ticker: string | null) => useResource<PackResponse>(at(ticker, "/pack"));

/** `elements`: `all` atau daftar elemen dipisah koma; tanpa itu, baris inti design.md §3.1. */
export const useFinancials = (ticker: string | null, view: FinancialsView = "standalone", periods = 12, elements?: string) =>
  useResource<FinancialsResponse>(at(ticker, "/financials"), { view, periods, elements });

/** Pemuat `FactDrawer`: `null` = drawer tertutup, tidak ada request. */
export const useFactDetail = (target: FactTarget | null) =>
  useResource<FactDetail>(target ? `${BASE}/facts/${enc(target.filing_id)}/${enc(target.element)}` : null, {
    ctx: target?.ctx,
    dims: target?.dims,
  });

export const useFilings = (ticker?: string) => useResource<FilingsResponse>(`${BASE}/filings`, { ticker: ticker?.toUpperCase() });

export const useValuation = (ticker: string | null, fcf: "ttm" | "avg5y" = "ttm") =>
  useResource<ValuationResponse>(at(ticker, "/valuation"), { fcf });

export const usePrices = (ticker: string | null, range: PriceRange = "3y") =>
  useResource<PricesResponse>(at(ticker, "/prices"), { range });

export const useClaims = (ticker: string | null) => useResource<ClaimsResponse>(at(ticker, "/claims"));

export const useClaimEvals = (claimId: string | null) =>
  useResource<ClaimEvalsResponse>(claimId ? `${BASE}/claims/${enc(claimId)}/evals` : null);

export const useClaimEvidence = (claimId: string | null) =>
  useResource<ClaimEvidenceResponse>(claimId ? `${BASE}/claims/${enc(claimId)}/evidence` : null);

/** 20 per halaman, terbaru dulu. */
export const useJournal = (ticker: string | null, limit = 20, offset = 0) =>
  useResource<JournalResponse>(at(ticker, "/journal"), { limit, offset });

export const useDocuments = (ticker: string | null) => useResource<DocumentsResponse>(at(ticker, "/documents"));

/** Usulan KPI dari dokumen unggahan (semua dokumen, semua status); penyaringan per dokumen dilakukan layar. */
export const useProposals = (ticker: string | null) => useResource<ProposalsResponse>(at(ticker, "/proposals"));

export const useCalendar = (ticker: string | null, days = 90) => useResource<CalendarResponse>(at(ticker, "/calendar"), { days });

export const useCalibration = (ticker?: string, type?: CalibrationKind) =>
  useResource<CalibrationResponse>(`${BASE}/calibration`, { ticker: ticker?.toUpperCase(), type });

export interface CompareQuery {
  pack?: string;
  /** Paket milik ticker ini bila `pack` kosong. */
  ticker?: string;
  tickers?: string[];
}

/** `null` menunda (belum ada paket maupun ticker). */
export const useCompare = (q: CompareQuery | null) =>
  useResource<CompareResponse>(q && (q.pack || q.ticker) ? `${BASE}/compare` : null, {
    pack: q?.pack,
    ticker: q?.ticker?.toUpperCase(),
    tickers: q?.tickers?.length ? q.tickers.map((t) => t.toUpperCase()).join(",") : undefined,
  });

/** Peta struktur bisnis emiten (simpul usulan dan diterima, metrik yang boleh ditautkan, status berkas draf). */
export const useStructure = (ticker: string | null) => useResource<StructureResponse>(at(ticker, "/structure"));

export const usePacks = () => useResource<PacksResponse>(`${BASE}/packs`);

export const useRules = () => useResource<RulesResponse>(`${BASE}/rules`);

// ---- tulis: klien bersama (header klien, sesi, galat 401) + Idempotency-Key ----

/** Unggahan sampai 20 MB lewat koneksi biasa: batas waktu lebih longgar dari tulis JSON (15 detik). */
export const UPLOAD_TIMEOUT_MS = 120_000;

type WriteMethod = "POST" | "PUT" | "PATCH" | "DELETE";

export interface WriteOptions {
  /**
   * Kunci idempotensi. Tanpa ini setiap panggilan membuat kunci baru. Layar yang mengulang simpan setelah
   * timeout memakai kunci yang sama dengan percobaan pertama: backend menurunkan id baris (klaim, jurnal, dokumen,
   * anotasi) dari kunci, jadi percobaan ulang tidak membuat baris kedua.
   */
  idempotencyKey?: string;
}

/**
 * Setiap tulis membawa `Idempotency-Key`. Setelah berhasil, semua bacaan Riset dimuat ulang; `refetch: false`
 * untuk tulisan yang tidak boleh menggeser layar yang sedang dibaca.
 */
async function write<T = unknown>(method: WriteMethod, path: string, body?: unknown, opts: WriteOptions = {}, refetch: string | false = BASE): Promise<T> {
  const timeoutMs = typeof FormData !== "undefined" && body instanceof FormData ? UPLOAD_TIMEOUT_MS : undefined;
  const result = await request<T>(`${BASE}${path}`, { method, body, idempotencyKey: opts.idempotencyKey ?? newIdempotencyKey(), timeoutMs });
  if (refetch !== false) invalidate(refetch);
  return result;
}

export const addToUniverse = (body: UniverseCreateBody, opts?: WriteOptions) =>
  write<UniverseItem>("POST", "/universe", { ...body, ticker: body.ticker.toUpperCase() }, opts);

export const updateUniverse = (ticker: string, body: UniverseUpdateBody, opts?: WriteOptions) =>
  write<UniverseItem>("PUT", `/universe/${encTicker(ticker)}`, body, opts);

/** Emiten yang dipegang ditolak (409). */
export const removeFromUniverse = (ticker: string, opts?: WriteOptions) => write<void>("DELETE", `/universe/${encTicker(ticker)}`, undefined, opts);

/** Satu sel KPI; `unknown: true` menyimpan "Tidak diketahui". */
export const putKpi = (ticker: string, metric: string, fy: number, q: number, body: KpiWriteBody, opts?: WriteOptions) =>
  write<KpiWriteResult>("PUT", `/${encTicker(ticker)}/kpi/${enc(metric)}/${fy}/${q}`, body, opts);

export const deleteKpi = (ticker: string, metric: string, fy: number, q: number, opts?: WriteOptions) =>
  write<void>("DELETE", `/${encTicker(ticker)}/kpi/${enc(metric)}/${fy}/${q}`, undefined, opts);

export const putShares = (ticker: string, body: SharesBody, opts?: WriteOptions) =>
  write<SharesWriteResult>("PUT", `/${encTicker(ticker)}/shares`, body, opts);

/** `asOf` menghapus satu hitungan bertanggal; tanpa itu, hitungan tunggal v4. */
export const deleteShares = (ticker: string, asOf?: string, opts?: WriteOptions) =>
  write<void>("DELETE", `/${encTicker(ticker)}/shares${asOf ? `?as_of=${enc(asOf)}` : ""}`, undefined, opts);

export const putScenarios = (ticker: string, body: ScenariosBody, opts?: WriteOptions) =>
  write<ScenariosPutResponse>("PUT", `/${encTicker(ticker)}/scenarios`, body, opts);

export const createClaim = (ticker: string, body: ClaimCreateBody, opts?: WriteOptions) => write<Claim>("POST", `/${encTicker(ticker)}/claims`, body, opts);

/**
 * **Tutup klaim**: klaim ditutup, tidak pernah diedit atau dibuka lagi. Balasan PATCH tidak membawa label metrik
 * dan satuan; pakai daftar yang dimuat ulang, bukan balasan ini.
 */
export const closeClaim = (claimId: string, opts?: WriteOptions) => write<Claim>("PATCH", `/claims/${enc(claimId)}`, { closed: true }, opts);

export const createJournalEntry = (ticker: string, body: JournalCreateBody, opts?: WriteOptions) =>
  write<JournalEntry>("POST", `/${encTicker(ticker)}/journal`, body, opts);

/** Entri jurnal tidak punya PUT/DELETE; anotasi ditambahkan di bawahnya. */
export const annotateJournal = (journalId: string, text: string, opts?: WriteOptions) =>
  write<JournalAnnotation & { journal_id: string }>("POST", `/journal/${enc(journalId)}/annotations`, { text }, opts);

export const createDocument = (ticker: string, body: DocumentCreateBody, opts?: WriteOptions) =>
  write<ResearchDocument>("POST", `/${encTicker(ticker)}/documents`, body, opts);

/**
 * **Unggah dokumen** (PDF atau PPTX): `multipart/form-data` dengan `file`, `kind`, `doc_date`, `fy`, `q`. Berkas yang sama untuk
 * emiten dan periode yang sama menjawab 200 `created: false`; percobaan ulang dengan kunci yang sama tidak membuat dokumen kedua.
 */
export const uploadDocument = (ticker: string, form: FormData, opts?: WriteOptions) => write<UploadResponse>("POST", `/${encTicker(ticker)}/documents/upload`, form, opts);

/** **Terima** usulan: menulis KPI dengan dokumen sebagai asal. 409 bila sel sudah berisi nilai dan `replace` tidak benar. */
export const acceptProposal = (proposalId: string, replace = false, opts?: WriteOptions) =>
  write<ProposalAcceptResponse>("POST", `/proposals/${enc(proposalId)}/accept`, replace ? { replace: true } : undefined, opts);

/** **Tolak** usulan: tidak menyimpan apa pun selain keputusan itu sendiri. */
export const rejectProposal = (proposalId: string, opts?: WriteOptions) => write<{ proposal: ProposalsResponse["items"][number] }>("POST", `/proposals/${enc(proposalId)}/reject`, undefined, opts);

export const linkEvidence = (body: EvidenceBody, opts?: WriteOptions) => write<EvidenceLink>("POST", "/evidence", body, opts);

export const addCalendarDate = (ticker: string, body: CalendarCreateBody, opts?: WriteOptions) =>
  write<CalendarRow>("POST", `/${encTicker(ticker)}/calendar`, body, opts);

export const deleteCalendarDate = (ticker: string, date: string, kind: string, opts?: WriteOptions) =>
  write<void>("DELETE", `/${encTicker(ticker)}/calendar/${enc(date)}/${enc(kind)}`, undefined, opts);

/** **Abaikan**. 409 bila sudah diabaikan. */
export const dismissEvent = (eventId: string, opts?: WriteOptions) => write<ResearchEvent>("POST", `/events/${enc(eventId)}/dismiss`, undefined, opts);

/**
 * Menandai dossier sudah ditinjau. Hanya Desk yang dimuat ulang: memuat ulang dossier akan mengosongkan
 * "Apa yang berubah" yang justru dihitung dari penanda sebelumnya. Tulisan lain pada dossier yang sama memuatnya
 * ulang juga; layar dossier menyimpan `changes` yang sudah dibaca selama ia terbuka.
 */
export const markSeen = (ticker: string, opts?: WriteOptions) => write<void>("POST", `/${encTicker(ticker)}/seen`, undefined, opts, `${BASE}/desk`);

/** Jeda sebelum bacaan dimuat ulang setelah `refreshResearch`; backend belum punya sinyal selesai (TBD, plan.md R5). */
export const REFRESH_SETTLE_MS = 10_000;

/**
 * Ambil ulang dan evaluasi ulang. Backend menjawab 202 lalu bekerja di latar belakang, jadi bacaan tidak dimuat ulang
 * seketika (isinya masih lama); satu muat ulang dijadwalkan setelah `REFRESH_SETTLE_MS`. Dibatasi satu per menit.
 */
export async function refreshResearch(ticker?: string, opts?: WriteOptions): Promise<RefreshResult> {
  const result = await write<RefreshResult>("POST", `/refresh${ticker ? `?ticker=${encTicker(ticker)}` : ""}`, undefined, opts, false);
  setTimeout(() => invalidate(BASE), REFRESH_SETTLE_MS);
  return result;
}

/** **Muat draf**: memuat `<TICKER>.draft.json` ke peta sebagai usulan. 404 bila berkas belum ada; 422 per field bila ada yang salah (berkas ditolak utuh). */
export const importStructureDraft = (ticker: string, opts?: WriteOptions) =>
  write<StructureDraftResult>("POST", `/${encTicker(ticker)}/structure/draft`, undefined, opts);

/** **Terima** / **Tolak** / **Ubah** satu simpul. */
export const updateStructureNode = (ticker: string, nodeId: string, body: StructureNodePatch, opts?: WriteOptions) =>
  write<StructureResponse>("PUT", `/${encTicker(ticker)}/structure/nodes/${enc(nodeId)}`, body, opts);

/** **Tambah cabang**: simpul milik pemilik, langsung diterima. */
export const addStructureNode = (ticker: string, body: StructureNodeCreate, opts?: WriteOptions) =>
  write<StructureNodeResult>("POST", `/${encTicker(ticker)}/structure/nodes`, body, opts);

/** Hanya simpul milik pemilik tanpa cabang; simpul usulan ditolak, tidak dihapus (409). */
export const deleteStructureNode = (ticker: string, nodeId: string, opts?: WriteOptions) =>
  write<StructureResponse>("DELETE", `/${encTicker(ticker)}/structure/nodes/${enc(nodeId)}`, undefined, opts);

/** **Muat ulang** paket dan aturan dari berkas. */
export const reloadPacks = (opts?: WriteOptions) => write<PacksReloadResponse>("POST", "/packs/reload", undefined, opts);
