-- 002_held_positions.sql: average cost of held positions (from the portfolio sheet), kept so
-- the file can show "price vs cost" without asking the sheet on every request.
CREATE TABLE held_positions (ticker TEXT PRIMARY KEY CHECK (ticker GLOB '[A-Z][A-Z][A-Z][A-Z]'), avg_price REAL NOT NULL);
