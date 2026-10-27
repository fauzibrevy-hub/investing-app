-- 004_migrate_owner_data.sql: the owner's v4 data into the canon tables (plan §4.5, D17).
-- Idempotent: deterministic ids and INSERT OR IGNORE, so R5 re-runs it before 005 to pick
-- up notes or targets written between R1 and R5. The v4 tables are not touched.

-- Every note becomes a journal entry with its original timestamp and the close known at
-- that moment: the same day's close only once the exchange has closed (16:00 WIB = 09:00
-- UTC; v4 timestamps are RFC 3339 in UTC), otherwise the last close before that day.
INSERT OR IGNORE INTO journal (id, ticker, at, kind, text, price_at, level)
SELECT 'v4note:' || n.id, n.ticker, n.created_at, 'catatan', n.text,
       (SELECT p.close FROM prices_daily p
         WHERE p.ticker = n.ticker
           AND p.date <= CASE WHEN substr(n.created_at, 12, 5) >= '09:00'
                              THEN substr(n.created_at, 1, 10)
                              ELSE date(substr(n.created_at, 1, 10), '-1 day') END
         ORDER BY p.date DESC LIMIT 1),
       NULL
FROM notes n;

-- Every target price becomes a journal level. The id carries set_at so a target changed
-- later becomes a new level and the earlier one stays as history (the journal is append-only).
INSERT OR IGNORE INTO journal (id, ticker, at, kind, text, price_at, level)
SELECT 'v4target:' || t.ticker || ':' || t.set_at, t.ticker, t.set_at, 'level', 'Harga incaran v4',
       (SELECT p.close FROM prices_daily p
         WHERE p.ticker = t.ticker
           AND p.date <= CASE WHEN substr(t.set_at, 12, 5) >= '09:00'
                              THEN substr(t.set_at, 1, 10)
                              ELSE date(substr(t.set_at, 1, 10), '-1 day') END
         ORDER BY p.date DESC LIMIT 1),
       t.entry_price
FROM targets t;

-- Manual additions without a universe row (sold since, or never synced) become manual members.
INSERT OR IGNORE INTO universe (ticker, role, origins, synced_at, added_at)
SELECT o.ticker, 'manual', '["manual"]', o.at, o.at
FROM universe_overrides o
WHERE o.action = 'add'
  AND NOT EXISTS (SELECT 1 FROM universe u WHERE u.ticker = o.ticker);

UPDATE universe SET role = 'manual' WHERE role = 'watch';

UPDATE universe
SET added_at = COALESCE(
      (SELECT o.at FROM universe_overrides o WHERE o.ticker = universe.ticker AND o.action = 'add'),
      synced_at)
WHERE added_at = '';
