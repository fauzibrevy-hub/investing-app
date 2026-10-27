-- 011_structure_maps.sql (Riset redesign G1, user_version 11): the operational structure map
-- of a company, one JSON document per ticker (ui-slicing.md §7). `doc` is the array of
-- nodes (id, parent, label, kind, metric, note, state, owner, source); the API validates
-- depth, node count, label length and metric ids before anything is written. Every write
-- replaces the whole document and is logged as one `StructureSet` line, so `rebuild`
-- replays the last one per ticker.
CREATE TABLE IF NOT EXISTS structure_maps (
  ticker TEXT PRIMARY KEY,
  doc TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
