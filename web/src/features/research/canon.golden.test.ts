import { describe, expect, it } from "vitest";
import type {
  CalendarItem,
  CalendarResponse,
  CalibrationResponse,
  CalibrationRow,
  Claim,
  ClaimsResponse,
  CompareCell,
  CompareResponse,
  CompareRow,
  CoverageRow,
  DeskResponse,
  DocumentsResponse,
  DossierChange,
  DossierResponse,
  FactDetail,
  FilingRow,
  FilingsResponse,
  FinancialsCell,
  FinancialsGroup,
  FinancialsPeriod,
  FinancialsResponse,
  FinancialsRow,
  HitRate,
  JournalEntry,
  JournalResponse,
  PackDefinition,
  PacksResponse,
  PackResponse,
  PricesResponse,
  ResearchDocument,
  ResearchEvent,
  RulesResponse,
  StructureNode,
  StructureResponse,
} from "./types";

/**
 * Golden canon dari backend (`tests/fixtures/research/canon/`, lane A1) dibandingkan dengan tipe. Berkasnya datang
 * dengan lane A1; sampai itu masuk ke cabang integrasi, glob ini kosong dan tes dilewati, bukan lulus palsu
 * (tes pertama membedakan keduanya). Tiap key di golden harus dikenal tipe; key wajib harus muncul di golden.
 * Array diperiksa dengan gabungan key seluruh itemnya.
 */
const files = import.meta.glob("../../../../tests/fixtures/research/canon/*.json", { eager: true, import: "default" }) as Record<string, unknown>;
const byName = new Map(Object.entries(files).map(([path, value]) => [path.split("/").pop()!.replace(/\.json$/, ""), value]));
const present = byName.size > 0;
const named = (prefix: string): Array<[string, unknown]> => [...byName].filter(([name]) => name === prefix || name.startsWith(`${prefix}-`));

function keysOf(x: unknown): string[] {
  const items = Array.isArray(x) ? x : [x];
  return [...new Set(items.flatMap((i) => (i && typeof i === "object" ? Object.keys(i) : [])))];
}

function expectKeys<T>(actual: unknown, allowed: Array<keyof T>, required: Array<keyof T> = allowed): void {
  const keys = keysOf(actual);
  expect(keys.filter((k) => !(allowed as string[]).includes(k)), "keys the type does not know").toEqual([]);
  expect((required as string[]).filter((k) => !keys.includes(k)), "required keys missing from golden").toEqual([]);
}

const get = (x: unknown, key: string): unknown => (x as Record<string, unknown>)[key];

describe("canon goldens against payload types (activates when lane A1's goldens are on the branch)", () => {
  it("knows whether the goldens are present", () => {
    // Both outcomes are legitimate; this test only makes a skipped suite visible in the output.
    expect(typeof present).toBe("boolean");
  });

  it.runIf(present)("GET /{ticker} → DossierResponse", () => {
    for (const [, g] of named("ticker")) {
      expectKeys<DossierResponse>(g, ["ticker", "name", "as_of", "role", "peer_of", "avg_price", "pack", "business_note", "latest_filing", "latest_close", "seen_at", "claims", "coverage", "changes"]);
      expectKeys<DossierChange>(get(g, "changes"), ["id", "kind", "at", "text", "section", "dismissed"]);
    }
  });

  it.runIf(present)("GET /compare → CompareResponse", () => {
    for (const [, g] of named("compare")) {
      expectKeys<CompareResponse>(g, ["as_of", "pack", "tickers", "truncated", "rows"]);
      const rows = get(g, "rows") as CompareRow[];
      expectKeys<CompareRow>(rows, ["group", "group_label", "metric", "cells"]);
      expectKeys<CompareCell>(rows.flatMap((r) => r.cells), ["ticker", "value", "period", "percentile", "state", "fact", "reason", "flags"]);
    }
  });

  it.runIf(present)("GET /{ticker}/financials → FinancialsResponse", () => {
    for (const [, g] of named("financials")) {
      expectKeys<FinancialsResponse>(g, ["ticker", "view", "rules_version", "currency", "rounding", "taxonomy", "periods", "groups", "shares", "filings"]);
      expectKeys<FinancialsPeriod>(get(g, "periods"), ["fy", "q", "label", "end", "filing_id", "source_kind", "audited", "derived_fy"]);
      const groups = get(g, "groups") as FinancialsGroup[];
      expectKeys<FinancialsGroup>(groups, ["id", "label", "rows"]);
      const rows = groups.flatMap((x) => x.rows);
      expectKeys<FinancialsRow>(rows, ["id", "label", "element", "unit", "cells"]);
      expectKeys<FinancialsCell>(rows.flatMap((r) => r.cells), ["value", "flags", "fact", "inputs"]);
      expectKeys<FilingRow>(get(g, "filings"), ["id", "ticker", "fy", "period", "taxonomy", "industry_code", "period_start", "period_end", "audited", "rounding", "currency", "filed_at", "available_from", "available_source", "source_path", "file_hash", "source_kind", "ingested_at", "fact_count"]);
      const shares = (get(g, "shares") as unknown[]).filter(Boolean);
      if (shares.length) expectKeys<{ shares: number; source_label: string; source: unknown }>(shares, ["shares", "source_label", "source"]);
    }
  });

  it.runIf(present)("GET /facts/{filing}/{element} → FactDetail", () => {
    for (const [, g] of named("facts")) {
      expectKeys<FactDetail>(g, ["fact", "filing", "history", "restatements", "derived"]);
      expectKeys<FactDetail["fact"]>(get(g, "fact"), ["filing_id", "element", "label", "ctx_kind", "start", "end", "dims", "value", "unit", "decimals", "cite"]);
      expectKeys<FactDetail["derived"][number]>(get(g, "derived"), ["fy", "q", "view", "value", "flags", "formula", "inputs", "rules_version"]);
    }
  });

  it.runIf(present)("GET /filings → FilingsResponse", () => {
    for (const [, g] of named("filings")) expectKeys<FilingsResponse>(g, ["items", "unparsed"]);
  });

  it.runIf(present)("GET /desk → DeskResponse", () => {
    for (const [, g] of named("desk")) {
      expectKeys<DeskResponse>(g, ["as_of", "inbox", "inbox_total", "filters", "coverage", "calendar", "ages"]);
      expectKeys<ResearchEvent>(get(g, "inbox"), ["id", "at", "ticker", "kind", "group", "section", "text", "age_days", "dismissed_at", "payload"]);
      expectKeys<CoverageRow>(get(g, "coverage"), ["ticker", "role", "peer_of", "business_note", "pack", "last_seen", "claims", "xbrl", "kpi", "next_report", "data"]);
      expectKeys<CalendarItem>(get(g, "calendar"), ["date", "ticker", "kind", "label", "source", "window", "projected", "overdue", "amount", "note"]);
    }
  });

  it.runIf(present)("claims, journal, documents, calendar", () => {
    for (const [, g] of named("claims")) {
      expectKeys<ClaimsResponse>(g, ["ticker", "items", "counts", "metrics", "windows", "comparators"]);
      expectKeys<Claim>(get(g, "items"), ["id", "ticker", "statement", "metric", "metric_label", "unit", "comparator", "threshold", "threshold_hi", "window", "due", "status", "status_label", "created_at", "closed_at", "preview", "last_eval", "evidence_count"]);
    }
    for (const [, g] of named("journal")) {
      expectKeys<JournalResponse>(g, ["ticker", "items", "total", "limit", "offset"]);
      expectKeys<JournalEntry>(get(g, "items"), ["id", "ticker", "at", "kind", "text", "price_at", "level", "annotations"]);
    }
    for (const [, g] of named("documents")) {
      expectKeys<DocumentsResponse>(g, ["ticker", "items", "kinds"]);
      expectKeys<ResearchDocument>(get(g, "items"), ["id", "ticker", "kind", "kind_label", "url", "text", "doc_date", "added_at", "evidence"]);
    }
    for (const [, g] of named("calendar")) expectKeys<CalendarResponse>(g, ["ticker", "days", "items"]);
  });

  it.runIf(present)("GET /calibration → CalibrationResponse", () => {
    for (const [, g] of named("calibration")) {
      expectKeys<CalibrationResponse>(g, ["as_of", "ticker", "type", "maturity_days", "total", "matured", "pending", "items", "hit_rates"]);
      expectKeys<CalibrationRow>(get(g, "items"), ["id", "ticker", "kind", "kind_label", "date", "text", "price_then", "price_now", "price_now_date", "days", "matured", "matures_on", "return_pct", "level", "reached", "status", "reason"]);
      expectKeys<HitRate>(get(g, "hit_rates"), ["ticker", "holding", "broken", "undecided", "rate_pct", "text"]);
    }
  });

  it.runIf(present)("pack, packs, prices, rules", () => {
    for (const [, g] of named("pack")) expectKeys<PackResponse>(g, ["ticker", "as_of", "role", "business_note", "pack", "core", "latest", "quarters", "dimensions", "drivers", "missing_drivers", "peers", "coverage"]);
    const packs = byName.get("packs");
    expectKeys<PacksResponse>(packs, ["status", "packs", "driver_catalogue"]);
    expectKeys<PackDefinition>(get(packs, "packs"), ["id", "version", "label", "industry_codes", "taxonomies", "members", "dimensions", "drivers"]);
    for (const [, g] of named("prices")) expectKeys<PricesResponse>(g, ["ticker", "as_of", "range", "from", "to", "pack", "price", "ma200", "drivers", "drivers_plotted", "index_relative", "index_symbol", "levels", "events", "stats", "empty"]);
    expectKeys<RulesResponse>(byName.get("rules"), ["rules_version", "derive_rules_version", "discount_rates", "terminal_growth", "horizon_years", "cycle_windows", "min_cycle_years", "percentile_window_years", "percentile_min_years", "pojk_offsets", "changelog", "source", "file", "error", "loaded_at"]);
  });

  it.runIf(present)("GET /{ticker}/structure → StructureResponse", () => {
    const g = byName.get("structure-COAL");
    expectKeys<StructureResponse>(g, ["ticker", "pack", "metrics", "kinds", "nodes", "counts", "updated_at", "limits", "draft"]);
    expectKeys<StructureNode>(get(g, "nodes"), ["id", "parent", "label", "kind", "kind_label", "metric", "metric_label", "note", "state", "state_label", "owner", "source"]);
  });
});
