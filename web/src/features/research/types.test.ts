import { describe, expect, it } from "vitest";
import bankJson from "../../../../tests/fixtures/research/packs/BANK.json";
import noneJson from "../../../../tests/fixtures/research/packs/NONE.json";
import valuationJson from "../../../../tests/fixtures/research/valuation/COAL.json";
import pricesJson from "../../../../tests/fixtures/research/valuation/COAL-prices-1y.json";
import type {
  KpiPackCell,
  PackDimension,
  PackMetric,
  PackResponse,
  PricesResponse,
  ValuationResponse,
  XbrlPackCell,
} from "./types";

/**
 * Tipe harus mengikuti serializer Rust. Golden backend (`tests/fixtures/research/**`) adalah bukti:
 * setiap key di golden harus dikenal tipe, dan key wajib harus ada di golden. Daftar key diketik
 * `Array<keyof T>` sehingga mengubah tipe tanpa mengubah tes ini gagal saat build.
 */
function expectKeys<T>(actual: object, allowed: Array<keyof T>, required: Array<keyof T> = allowed): void {
  const keys = Object.keys(actual);
  expect(keys.filter((k) => !(allowed as string[]).includes(k))).toEqual([]);
  expect((required as string[]).filter((k) => !keys.includes(k))).toEqual([]);
}

// Casting JSON literal types would blow up on heterogeneous arrays; the cast is the contract under test.
const bank = bankJson as unknown as PackResponse;
const none = noneJson as unknown as PackResponse;
const coalValuation = valuationJson as unknown as ValuationResponse;
const coalPrices = pricesJson as unknown as PricesResponse;

describe("payload types against backend goldens", () => {
  it("PackResponse (GET /{ticker}/pack)", () => {
    const all: Array<keyof PackResponse> = [
      "ticker", "as_of", "role", "business_note", "pack", "core", "latest", "quarters",
      "dimensions", "drivers", "missing_drivers", "peers", "coverage",
    ];
    for (const golden of [bank, none]) expectKeys<PackResponse>(golden, all);
    expectKeys<PackResponse["pack"]>(bank.pack, ["id", "source", "filing_code", "filing_default", "override", "version", "label", "action"]);
    expect(none.pack.action).toBe("Tentukan paket");
  });

  it("PackDimension and PackMetric", () => {
    const dimension = bank.dimensions[0];
    expectKeys<PackDimension>(dimension, ["pack", "id", "label", "kind", "metrics"]);
    const metrics: PackMetric[] = bank.dimensions.flatMap((d) => d.metrics);
    const common: Array<keyof PackMetric> = ["id", "label", "layer", "unit", "direction", "why", "note", "cells", "latest", "yoy", "yoy_reason", "percentile", "peers"];
    const optional: Array<keyof PackMetric> = ["formula", "period", "bindings", "cadence", "driver", "range"];
    for (const m of metrics) expectKeys<PackMetric>(m, [...common, ...optional], common);
    expect(new Set(metrics.map((m) => m.layer))).toEqual(new Set(["xbrl", "kpi"]));
  });

  it("pack cells: formula cells and KPI cells", () => {
    const metrics: PackMetric[] = bank.dimensions.flatMap((d) => d.metrics);
    const xbrl = metrics.find((m) => m.layer === "xbrl")!.cells[0];
    expectKeys<XbrlPackCell>(xbrl, ["fy", "q", "label", "value", "gap", "flags", "inputs"]);
    const kpi = metrics.find((m) => m.layer === "kpi")!.cells[0];
    expectKeys<KpiPackCell>(kpi, ["fy", "q", "label", "state", "action"], ["fy", "q", "label", "state"]);
  });

  it("ValuationResponse (GET /{ticker}/valuation)", () => {
    expectKeys<ValuationResponse>(coalValuation, [
      "ticker", "as_of", "rules_version", "currency", "fx_usd_idr", "price", "shares", "market_cap",
      "latest_quarter", "history", "reverse_dcf", "multiples", "earnings_yield", "scenarios",
    ], [
      "ticker", "as_of", "rules_version", "currency", "price", "shares", "market_cap",
      "latest_quarter", "history", "reverse_dcf", "multiples", "earnings_yield", "scenarios",
    ]);
    expect(Object.keys(coalValuation.multiples).sort()).toEqual(["dividend_yield", "ev_ebitda", "pb", "pe"]);
    expect(coalValuation.scenarios.rows.map((r) => r.name)).toEqual(["bear", "base", "bull"]);
  });

  it("PricesResponse (GET /{ticker}/prices)", () => {
    expectKeys<PricesResponse>(coalPrices, [
      "ticker", "as_of", "range", "from", "to", "pack", "price", "ma200", "drivers", "drivers_plotted",
      "index_relative", "index_symbol", "levels", "events", "stats", "empty",
    ]);
    expectKeys<PricesResponse["stats"]>(coalPrices.stats, [
      "close", "high_52w", "volatility_1y_pct", "vs_index_6m_pct", "dividend_yield_ttm_pct", "avg_cost",
    ]);
  });
});
