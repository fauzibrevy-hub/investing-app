import { useResource, type Resource } from "../../lib/useResource";
import type { Draft, DraftsResponse, ThesisDetail, ThesisSummary, VaultDraft } from "./types";

export const THESIS_BASE = "/api/thesis";

const enc = encodeURIComponent;

/** `GET /api/thesis/summary`: satu baris per ticker yang punya dossier di vault. */
export const useThesisSummary = (): Resource<ThesisSummary> => useResource<ThesisSummary>(`${THESIS_BASE}/summary`);

/** `GET /api/thesis/drafts`: draf ber-ticker (`drafts`) dan draf vault tanpa ticker (`vault_drafts`). */
export const useThesisDrafts = (): Resource<DraftsResponse> => useResource<DraftsResponse>(`${THESIS_BASE}/drafts`);

/** `GET /api/thesis/drafts/vault`: dimuat hanya saat `enabled` (Sheet terbuka). */
export const useVaultDrafts = (enabled: boolean): Resource<DraftsResponse> => useResource<DraftsResponse>(enabled ? `${THESIS_BASE}/drafts/vault` : null);

/** `GET /api/thesis/drafts/{id}`: draf biasa atau draf vault (`is_readonly`). `null` menunda. */
export const useThesisDraft = (id: string | null): Resource<Draft | VaultDraft> => useResource<Draft | VaultDraft>(id ? `${THESIS_BASE}/drafts/${enc(id)}` : null);

/** `GET /api/thesis/{T}`: dossier vault; 404 berarti belum ada dossier. */
export const useThesisDetail = (ticker: string | null): Resource<ThesisDetail> => useResource<ThesisDetail>(ticker ? `${THESIS_BASE}/${enc(ticker.toUpperCase())}` : null);

/** Teks falsifier draf vault: kolom terakhir yang terisi (`#`, `Batas`, `Falsifier`), seperti layar Tesis finance-app. */
export function vaultFalsifierText(row: Record<string, string>): string {
  return Object.values(row).filter(Boolean).slice(-1)[0] ?? "";
}
