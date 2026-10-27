CREATE TABLE universe (
  ticker TEXT PRIMARY KEY CHECK (ticker GLOB '[A-Z][A-Z][A-Z][A-Z]'),
  role   TEXT NOT NULL CHECK (role IN ('held','watch')),
  origins TEXT NOT NULL,
  synced_at TEXT NOT NULL
);
CREATE TABLE universe_overrides (ticker TEXT PRIMARY KEY, action TEXT NOT NULL CHECK (action IN ('add','remove')), at TEXT NOT NULL);
CREATE TABLE prices_daily (ticker TEXT NOT NULL, date TEXT NOT NULL, open REAL, high REAL, low REAL, close REAL NOT NULL, adj_close REAL, volume INTEGER, PRIMARY KEY (ticker, date));
CREATE TABLE corp_events (ticker TEXT NOT NULL, date TEXT NOT NULL, kind TEXT NOT NULL CHECK (kind IN ('dividend','split')), amount REAL, ratio_num REAL, ratio_den REAL, PRIMARY KEY (ticker, date, kind));
CREATE TABLE fundamentals_q (
  ticker TEXT NOT NULL, fy INTEGER NOT NULL, q INTEGER NOT NULL CHECK (q BETWEEN 1 AND 4), metric TEXT NOT NULL,
  ytd REAL, standalone REAL, ttm REAL,
  available_from TEXT NOT NULL, available_source TEXT NOT NULL CHECK (available_source IN ('published','estimated')),
  source_file TEXT, suspect INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (ticker, fy, q, metric)
);
CREATE TABLE shares (ticker TEXT NOT NULL, as_of TEXT NOT NULL, shares REAL NOT NULL, source TEXT NOT NULL CHECK (source IN ('xbrl_period','xbrl_current')), PRIMARY KEY (ticker, as_of, source));
CREATE TABLE shares_manual (ticker TEXT PRIMARY KEY, shares REAL NOT NULL CHECK (shares > 0), as_of TEXT NOT NULL, source_note TEXT NOT NULL CHECK (length(source_note) BETWEEN 1 AND 200), entered_at TEXT NOT NULL);
CREATE TABLE targets (ticker TEXT PRIMARY KEY, entry_price REAL NOT NULL CHECK (entry_price > 0), set_at TEXT NOT NULL);
CREATE TABLE notes (id TEXT PRIMARY KEY, ticker TEXT NOT NULL, created_at TEXT NOT NULL, text TEXT NOT NULL CHECK (length(text) BETWEEN 1 AND 2000));
CREATE TABLE source_runs (source TEXT NOT NULL, ticker TEXT, started_at TEXT NOT NULL, finished_at TEXT, ok INTEGER NOT NULL, rows INTEGER, error TEXT);
CREATE TABLE snapshots (ticker TEXT NOT NULL, taken_at TEXT NOT NULL, rules_version TEXT NOT NULL, eligibility TEXT NOT NULL, zone TEXT NOT NULL, content_hash TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY (ticker, taken_at));
CREATE TABLE seen (ticker TEXT PRIMARY KEY, seen_at TEXT NOT NULL, eligibility TEXT NOT NULL, zone TEXT NOT NULL);
