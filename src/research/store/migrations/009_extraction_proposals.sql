-- 009_extraction_proposals.sql (R8, user_version 9): KPI values the rule-based extractor
-- proposes from an uploaded PDF or presentation. A proposal is only a suggestion with its
-- page and a snippet; nothing reaches `kpi_values` until the owner accepts it. Owner-decided
-- rows are logged (`ProposalAdd`, `ProposalDecide`) and `rebuild` replays them.
CREATE TABLE IF NOT EXISTS extraction_proposals (
  id TEXT PRIMARY KEY,
  document_id TEXT NOT NULL REFERENCES documents(id),
  ticker TEXT NOT NULL,
  kpi_key TEXT NOT NULL,                -- "<metric>:<FY>Q<q>", the key an evidence link carries
  value REAL NOT NULL,
  unit TEXT NOT NULL,
  page INTEGER NOT NULL CHECK (page >= 1),   -- page of a PDF, slide of a presentation
  snippet TEXT NOT NULL CHECK (length(snippet) <= 160),
  rank INTEGER NOT NULL CHECK (rank >= 1),   -- 1 is the best candidate for that kpi_key
  status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'rejected')),
  created_at TEXT NOT NULL,
  decided_at TEXT
);
CREATE INDEX IF NOT EXISTS extraction_proposals_document ON extraction_proposals (document_id, kpi_key, rank);
CREATE INDEX IF NOT EXISTS extraction_proposals_ticker ON extraction_proposals (ticker, status);
