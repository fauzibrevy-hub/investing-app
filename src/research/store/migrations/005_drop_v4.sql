-- 005_drop_v4.sql: remove the v4-only tables and columns (plan §4.5 step 3, D17).
-- WRITTEN in R1.7, REGISTERED in R5-E1 as schema v8 (`drop_v4_step` in store/mod.rs): the
-- step re-runs 004 first, then applies this file. Requires SQLite >= 3.35 (DROP COLUMN).
-- `held_positions` stays: it is the average cost the canon header and price payload read
-- (plan §4.5 ASUMSI R5-E1).

DROP TABLE IF EXISTS snapshots;
DROP TABLE IF EXISTS fundamentals_q;
DROP TABLE IF EXISTS shares;
DROP TABLE IF EXISTS universe_overrides;
DROP TABLE IF EXISTS targets;
DROP TABLE IF EXISTS notes;

-- seen: only ticker and seen_at remain.
CREATE TABLE seen_new (ticker TEXT PRIMARY KEY, seen_at TEXT NOT NULL);
INSERT INTO seen_new (ticker, seen_at) SELECT ticker, seen_at FROM seen;
DROP TABLE seen;
ALTER TABLE seen_new RENAME TO seen;

ALTER TABLE prices_daily DROP COLUMN adj_close;

-- universe: drop origins and the v4 role `watch`.
CREATE TABLE universe_v5 (
  ticker TEXT PRIMARY KEY CHECK (ticker GLOB '[A-Z][A-Z][A-Z][A-Z]'),
  role TEXT NOT NULL CHECK (role IN ('held','manual','peer')),
  peer_of TEXT, pack_override TEXT, business_note TEXT,
  added_at TEXT NOT NULL, synced_at TEXT NOT NULL
);
INSERT INTO universe_v5 (ticker, role, peer_of, pack_override, business_note, added_at, synced_at)
  SELECT ticker, CASE role WHEN 'watch' THEN 'manual' ELSE role END, peer_of, pack_override, business_note,
         CASE added_at WHEN '' THEN synced_at ELSE added_at END, synced_at
  FROM universe;
DROP TABLE universe;
ALTER TABLE universe_v5 RENAME TO universe;
