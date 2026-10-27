-- 007_prices_drop_adj_close.sql (R3.1, plan §4.1): prices_daily without adj_close. Yahoo's
-- adjusted close is not stored; dividends and splits live in corp_events. Table rebuild
-- rather than DROP COLUMN so the primary key stays declared. 005_drop_v4.sql also names
-- this column; `Store::apply_drop_v4` skips that statement once the column is gone.
CREATE TABLE prices_daily_new (
  ticker TEXT NOT NULL, date TEXT NOT NULL,
  open REAL, high REAL, low REAL, close REAL NOT NULL, volume INTEGER,
  PRIMARY KEY (ticker, date)
);
INSERT INTO prices_daily_new (ticker, date, open, high, low, close, volume)
  SELECT ticker, date, open, high, low, close, volume FROM prices_daily;
DROP TABLE prices_daily;
ALTER TABLE prices_daily_new RENAME TO prices_daily;
