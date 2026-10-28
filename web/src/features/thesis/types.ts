/** Bentuk JSON Area 3 (backend `src/thesis/`, = MC). Hanya field yang dipakai UI. */

export interface IssuerSummary {
  ticker: string;
  company: string | null;
  status: "holdings" | "watchlist" | "archived" | string;
  verdict: string | null;
  narrative_status: string | null;
  fundamental_status: string | null;
  growth_status: string | null;
  falsifiers: { count: number; nearest: { date: string; days_left: number; text: string } | null };
  review: { last_date: string | null; days_ago: number | null; due: boolean };
  evidence: { resolved: number; doc_quoted: number; missing: number; total: number };
  tahap: string | null;
  errors: string[];
}

export interface ThesisSummary {
  root_ok: boolean;
  generated_at: number;
  issuers: IssuerSummary[];
}

export interface ThesisDetail extends IssuerSummary {
  alasan_masuk: string | null;
  verdict_text: string | null;
  lens: { metrics: { metrik: string; definisi: string; alasan: string; sumber_data: string; field: string }[]; locked: string | null; approved_at: string | null };
  falsifier_items: { n: number; text: string; dates: string[] }[];
  review_log: { date: string | null; title: string; bullets: { level: 0 | 1; text: string }[] }[];
  doc_evidence: { metrik: string; periode: string; raw: string; unit: string; dokumen: string; hal: string }[];
  chain: { pernyataan: string; status: string; bukti: string }[];
  trigger_tambah: string[];
  trigger_buang: string[];
}

export const THESIS_TYPES = ["fundamental", "narrative", "hybrid"] as const;
export const SCENARIOS = ["Menunggu", "Berjalan", "Melemah", "Gugur", "Tercapai"] as const;
export const STORY_STATUSES = ["Belum terkonfirmasi", "Terdukung sebagian", "Terkonfirmasi", "Terbantahkan", "Tidak berlaku"] as const;

export interface Draft {
  id: string;
  ticker: string;
  title: string;
  thesis_statement: string;
  thesis_type: (typeof THESIS_TYPES)[number];
  is_primary: boolean;
  scenario_status: string;
  story_status: string;
  falsifiers: string[];
  triggers_add: string[];
  triggers_exit: string[];
  notes: string;
  created_at: string;
  updated_at: string;
}

/** Draft dari vault: baca saja, tidak bisa dihapus (backend 404). */
export interface VaultDraft {
  id: string;
  title: string;
  status_lifecycle: string;
  thesis_statement: string;
  tahap: string;
  is_readonly: true;
  falsifiers: Record<string, string>[];
  file_info: { name: string };
}

export interface DraftsResponse {
  drafts: Draft[];
  vault_drafts: VaultDraft[];
  total: number;
}

export interface DraftInput {
  ticker: string;
  title: string;
  thesis_statement: string;
  thesis_type: string;
  is_primary: boolean;
  scenario_initial: string;
  story_status_initial: string;
  falsifiers: string[];
  triggers_add: string[];
  triggers_exit: string[];
  notes: string;
}
