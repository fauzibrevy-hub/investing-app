-- 003_facts.sql: research canon v1 schema (docs/research/plan.md §4.1). The v4 tables stay in
-- place until 005_drop_v4.sql runs in R5, because the v4 screens still read them.

CREATE TABLE filings (
  id TEXT PRIMARY KEY,                 -- "<TICKER>_<FY>_<Q1|Q2|Q3|FY>"
  ticker TEXT NOT NULL, fy INTEGER NOT NULL, period TEXT NOT NULL CHECK (period IN ('Q1','Q2','Q3','FY')),
  taxonomy TEXT NOT NULL,              -- general | infrastructure | financial_sharia | ... from idx-dei EntityMainIndustry
  industry_code TEXT,                  -- idx-dei Subindustry code, e.g. "G111"
  period_start TEXT NOT NULL, period_end TEXT NOT NULL,
  audited INTEGER NOT NULL, rounding TEXT NOT NULL,
  currency TEXT NOT NULL, filed_at TEXT, available_from TEXT NOT NULL,
  available_source TEXT NOT NULL CHECK (available_source IN ('declared','estimated')),
  source_path TEXT NOT NULL, file_hash TEXT NOT NULL, ingested_at TEXT NOT NULL
);
CREATE TABLE facts (
  filing_id TEXT NOT NULL REFERENCES filings(id),
  element TEXT NOT NULL,
  ctx_kind TEXT NOT NULL CHECK (ctx_kind IN ('duration','instant')),
  start TEXT, end_ TEXT NOT NULL,
  dims TEXT NOT NULL DEFAULT '',
  value REAL NOT NULL, unit TEXT NOT NULL, decimals INTEGER,
  PRIMARY KEY (filing_id, element, ctx_kind, start, end_, dims)
);
CREATE TABLE derived (
  ticker TEXT NOT NULL, element TEXT NOT NULL, fy INTEGER NOT NULL, q INTEGER NOT NULL CHECK (q BETWEEN 1 AND 4),
  standalone REAL, ttm REAL, annual REAL,
  flags TEXT NOT NULL DEFAULT '',
  inputs TEXT NOT NULL,
  rules_version TEXT NOT NULL, computed_at TEXT NOT NULL,
  PRIMARY KEY (ticker, element, fy, q)
);
CREATE TABLE restatements (
  ticker TEXT, element TEXT, start TEXT, end_ TEXT, dims TEXT,
  original REAL, original_filing TEXT, restated REAL, restated_filing TEXT, detected_at TEXT,
  PRIMARY KEY (ticker, element, start, end_, dims, restated_filing)
);
-- ASUMSI (R1.2): files the parser could not read are kept here so health can list them;
-- a file is retried only when its hash changes, and a successful ingest clears the row.
CREATE TABLE ingest_errors (source_path TEXT PRIMARY KEY, file_hash TEXT NOT NULL, error TEXT NOT NULL, at TEXT NOT NULL);
CREATE TABLE drivers (series TEXT NOT NULL, date TEXT NOT NULL, value REAL NOT NULL, source TEXT NOT NULL, fetched_at TEXT NOT NULL, PRIMARY KEY (series, date));
CREATE TABLE kpi_values (ticker TEXT NOT NULL, metric TEXT NOT NULL, fy INTEGER NOT NULL, q INTEGER NOT NULL, value REAL, unknown INTEGER NOT NULL DEFAULT 0, unit TEXT NOT NULL, document TEXT NOT NULL, document_date TEXT NOT NULL, entered_at TEXT NOT NULL, PRIMARY KEY (ticker, metric, fy, q));
CREATE TABLE scenarios (ticker TEXT NOT NULL, name TEXT NOT NULL CHECK (name IN ('bear','base','bull')), growth REAL, margin REAL, exit_multiple REAL, years INTEGER, updated_at TEXT NOT NULL, PRIMARY KEY (ticker, name));
CREATE TABLE claims (id TEXT PRIMARY KEY, ticker TEXT NOT NULL, statement TEXT NOT NULL CHECK (length(statement) BETWEEN 1 AND 200), metric TEXT NOT NULL, comparator TEXT NOT NULL CHECK (comparator IN ('<','<=','>','>=','between')), threshold REAL NOT NULL, threshold_hi REAL, window TEXT NOT NULL, due TEXT NOT NULL, status TEXT NOT NULL, created_at TEXT NOT NULL, closed_at TEXT);
CREATE TABLE claim_evals (claim_id TEXT NOT NULL REFERENCES claims(id), evaluated_at TEXT NOT NULL, status TEXT NOT NULL, value REAL, inputs TEXT NOT NULL, PRIMARY KEY (claim_id, evaluated_at));
CREATE TABLE journal (id TEXT PRIMARY KEY, ticker TEXT NOT NULL, at TEXT NOT NULL, kind TEXT NOT NULL CHECK (kind IN ('catatan','level','keputusan','sistem')), text TEXT NOT NULL CHECK (length(text) BETWEEN 1 AND 2000), price_at REAL, level REAL);
CREATE TABLE journal_annotations (id TEXT PRIMARY KEY, journal_id TEXT NOT NULL REFERENCES journal(id), at TEXT NOT NULL, text TEXT NOT NULL);
CREATE TABLE documents (id TEXT PRIMARY KEY, ticker TEXT NOT NULL, kind TEXT NOT NULL, url TEXT, text TEXT, doc_date TEXT, added_at TEXT NOT NULL);
CREATE TABLE evidence (claim_id TEXT, kpi_key TEXT, document_id TEXT NOT NULL REFERENCES documents(id), PRIMARY KEY (document_id, claim_id, kpi_key));
CREATE TABLE events (id TEXT PRIMARY KEY, at TEXT NOT NULL, ticker TEXT, kind TEXT NOT NULL, payload TEXT NOT NULL, dismissed_at TEXT);
CREATE TABLE calendar_manual (ticker TEXT NOT NULL, date TEXT NOT NULL, kind TEXT NOT NULL, note TEXT, PRIMARY KEY (ticker, date, kind));

-- universe: role widened to held|manual|peer, new columns. `origins` and the v4 role `watch`
-- stay accepted until 005 because the v4 universe sync rewrites the table until R2.2 (ASUMSI).
CREATE TABLE universe_new (
  ticker TEXT PRIMARY KEY CHECK (ticker GLOB '[A-Z][A-Z][A-Z][A-Z]'),
  role TEXT NOT NULL CHECK (role IN ('held','manual','peer','watch')),
  origins TEXT NOT NULL DEFAULT '[]',
  peer_of TEXT, pack_override TEXT, business_note TEXT,
  added_at TEXT NOT NULL DEFAULT '', synced_at TEXT NOT NULL
);
-- added_at is left empty here; 004 fills it from the manual-add override, else synced_at.
INSERT INTO universe_new (ticker, role, origins, synced_at, added_at)
  SELECT ticker, role, origins, synced_at, '' FROM universe;
DROP TABLE universe;
ALTER TABLE universe_new RENAME TO universe;

-- shares_manual: one row per (ticker, as_of) instead of one per ticker; the v4 length check
-- on source_note is kept (the API relies on the store rejecting an empty note).
CREATE TABLE shares_manual_new (
  ticker TEXT NOT NULL, as_of TEXT NOT NULL, shares REAL NOT NULL CHECK (shares > 0),
  source_note TEXT NOT NULL CHECK (length(source_note) BETWEEN 1 AND 200), entered_at TEXT NOT NULL,
  PRIMARY KEY (ticker, as_of)
);
INSERT INTO shares_manual_new (ticker, as_of, shares, source_note, entered_at)
  SELECT ticker, as_of, shares, source_note, entered_at FROM shares_manual;
DROP TABLE shares_manual;
ALTER TABLE shares_manual_new RENAME TO shares_manual;
