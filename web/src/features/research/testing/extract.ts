import type { ExtractProposal, ProposalDocument, ProposalsResponse, UploadResponse } from "../types";

/**
 * Tiruan R8 untuk tes layar dan e2e (bentuk sama dengan `GET /{ticker}/proposals` dan `POST /{ticker}/documents/upload`,
 * lihat plan.md R8). Sintetis: golden canon tidak memuat usulan. Angka dan kutipan tidak berasal dari dokumen nyata.
 */
export const UPLOAD_DOC: ProposalDocument = {
  id: "d-u-0001",
  kind: "presentasi",
  kind_label: "Presentasi",
  doc_date: "2026-10-20",
  added_at: "2026-10-24T09:00:00+00:00",
  format: "pdf",
  locator: "page",
};

const base = { document_id: UPLOAD_DOC.id, ticker: "COAL", fy: 2026, q: 2, created_at: "2026-10-24T09:00:00+00:00", decided_at: null };

export function proposal(over: Partial<ExtractProposal> & Pick<ExtractProposal, "id" | "metric" | "metric_label" | "unit" | "value" | "page" | "snippet" | "rank">): ExtractProposal {
  return { ...base, kpi_key: `${over.metric}:2026Q2`, locator: "page", status: "pending", ...over };
}

export const PROPOSALS: ExtractProposal[] = [
  proposal({ id: "p-prod-1", metric: "production_mt", metric_label: "Produksi", unit: "Mt", value: 5.4, page: 3, snippet: "Produksi batubara kuartal ini 5,4 juta ton, naik tipis dari kuartal lalu", rank: 1 }),
  proposal({ id: "p-prod-2", metric: "production_mt", metric_label: "Produksi", unit: "Mt", value: 5.1, page: 7, snippet: "Produksi kuartal sebelumnya 5,1 juta ton | rencana tahunan", rank: 2 }),
  proposal({ id: "p-cash-1", metric: "cash_cost_usd_t", metric_label: "Biaya tunai", unit: "USD/t", value: 55.3, page: 5, snippet: "Biaya tunai USD 55,3 per ton", rank: 1 }),
  proposal({ id: "p-sr-1", metric: "stripping_ratio", metric_label: "Nisbah kupas", unit: "bcm/t", value: 6.8, page: 9, snippet: "Nisbah kupas 6,8 bcm per ton", rank: 1 }),
];

export function proposalsResponse(items: ExtractProposal[] = PROPOSALS, documents: ProposalDocument[] = items.length ? [UPLOAD_DOC] : []): ProposalsResponse {
  const count = (s: string) => items.filter((i) => i.status === s).length;
  return { ticker: "COAL", document_id: null, items, counts: { pending: count("pending"), accepted: count("accepted"), rejected: count("rejected"), superseded: count("superseded") }, documents };
}

export function uploadResponse(over: Partial<UploadResponse> = {}, items: ExtractProposal[] = PROPOSALS): UploadResponse {
  return {
    created: true,
    document: { id: UPLOAD_DOC.id, ticker: "COAL", kind: "presentasi", kind_label: "Presentasi", url: null, text: "Unggahan PDF: paparan.pdf · 12 halaman", doc_date: UPLOAD_DOC.doc_date, added_at: UPLOAD_DOC.added_at, evidence: [] },
    upload: { sha256: "0".repeat(64), format: "pdf", bytes: 48_000, pages: 12, pages_with_text: 12, locator: "page" },
    period: { fy: 2026, q: 2, label: "2026Q2" },
    pack: { id: "coal_mining", label: "Pertambangan batubara" },
    proposals: items,
    counts: proposalsResponse(items).counts,
    ...over,
  };
}
