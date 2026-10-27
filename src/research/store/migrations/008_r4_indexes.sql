-- 008_r4_indexes.sql (R4): the unique index that makes an evidence link unique despite its
-- NULLable `claim_id` / `kpi_key` (SQLite treats NULLs as distinct in a primary key), and
-- lookup indexes for the R4 queries. `claim_evals` is already served by its primary key
-- `(claim_id, evaluated_at)`. `evidence` had no writer before R4, so the unique index
-- cannot meet duplicates on an upgraded store. Indexes only, safe to run twice.
CREATE UNIQUE INDEX IF NOT EXISTS evidence_unique ON evidence (document_id, COALESCE(claim_id, ''), COALESCE(kpi_key, ''));
CREATE INDEX IF NOT EXISTS journal_annotations_journal ON journal_annotations (journal_id, at);
CREATE INDEX IF NOT EXISTS documents_ticker ON documents (ticker, added_at);
-- The Desk reads open events newest first; restatements are read by detection time.
CREATE INDEX IF NOT EXISTS events_open ON events (at DESC, id DESC) WHERE dismissed_at IS NULL;
CREATE INDEX IF NOT EXISTS restatements_detected ON restatements (detected_at);
