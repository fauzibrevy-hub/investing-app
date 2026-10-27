-- 010_proposals_superseded.sql (R8 hardening, user_version 10): a proposal gets a fourth
-- state, `superseded`: another candidate of the same KPI cell was accepted (its pending
-- siblings) or replaced it (an accepted value that a later accept overwrote). SQLite cannot
-- change a CHECK, so the table is rebuilt; rows are copied as they are.
CREATE TABLE extraction_proposals_new (
  id TEXT PRIMARY KEY,
  document_id TEXT NOT NULL REFERENCES documents(id),
  ticker TEXT NOT NULL,
  kpi_key TEXT NOT NULL,
  value REAL NOT NULL,
  unit TEXT NOT NULL,
  page INTEGER NOT NULL CHECK (page >= 1),
  snippet TEXT NOT NULL CHECK (length(snippet) <= 160),
  rank INTEGER NOT NULL CHECK (rank >= 1),
  status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'rejected', 'superseded')),
  created_at TEXT NOT NULL,
  decided_at TEXT
);
INSERT INTO extraction_proposals_new
  SELECT id, document_id, ticker, kpi_key, value, unit, page, snippet, rank, status, created_at, decided_at
  FROM extraction_proposals;
DROP TABLE extraction_proposals;
ALTER TABLE extraction_proposals_new RENAME TO extraction_proposals;
CREATE INDEX IF NOT EXISTS extraction_proposals_document ON extraction_proposals (document_id, kpi_key, rank);
CREATE INDEX IF NOT EXISTS extraction_proposals_ticker ON extraction_proposals (ticker, status);
