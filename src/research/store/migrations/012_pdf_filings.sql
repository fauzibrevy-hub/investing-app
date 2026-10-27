-- 012_pdf_filings.sql (user_version 12): a filing can come from an original PDF that the
-- owner read outside the app and checked into a JSON file (xbrl/pdf.rs), not only from an
-- IDX XBRL instance. `source_kind` says which; every existing row is `xbrl` by default.
-- A PDF fact carries the page it was read from and the label as printed there, so the fact
-- drawer and the pack cells can cite it; both stay NULL for XBRL facts. Columns only, no
-- data change: facts rebuild from the source directories (`research-ingest --reingest`).
ALTER TABLE filings ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'xbrl' CHECK (source_kind IN ('xbrl', 'pdf'));
ALTER TABLE facts ADD COLUMN page INTEGER CHECK (page IS NULL OR page >= 1);
ALTER TABLE facts ADD COLUMN label TEXT;
