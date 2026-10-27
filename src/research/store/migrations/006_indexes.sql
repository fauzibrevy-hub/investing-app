-- 006_indexes.sql: lookup indexes for the canon tables (plan §4.1, §4.5 step 4).
-- `start` is NULL for instants and SQLite treats NULLs as distinct inside a primary key,
-- so these unique indexes make the key real for instants too.
CREATE UNIQUE INDEX IF NOT EXISTS facts_unique ON facts (filing_id, element, ctx_kind, COALESCE(start, ''), end_, dims);
CREATE UNIQUE INDEX IF NOT EXISTS restatements_unique ON restatements (ticker, element, COALESCE(start, ''), end_, dims, restated_filing);
CREATE INDEX IF NOT EXISTS facts_lookup ON facts (element, end_, dims);
CREATE INDEX IF NOT EXISTS facts_filing ON facts (filing_id);
CREATE INDEX IF NOT EXISTS filings_ticker ON filings (ticker, fy, period);
CREATE INDEX IF NOT EXISTS filings_source ON filings (source_path);
CREATE INDEX IF NOT EXISTS restatements_ticker ON restatements (ticker, element);
CREATE INDEX IF NOT EXISTS journal_ticker ON journal (ticker, at);
CREATE INDEX IF NOT EXISTS claims_ticker ON claims (ticker, status);
CREATE INDEX IF NOT EXISTS events_at ON events (at);
CREATE INDEX IF NOT EXISTS kpi_ticker ON kpi_values (ticker, metric);
