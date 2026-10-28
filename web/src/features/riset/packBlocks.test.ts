import { describe, expect, it } from "vitest";
import pack from "../../../../tests/fixtures/research/canon/pack-COAL.json";
import prices from "../../../../tests/fixtures/research/canon/prices-COAL-1y.json";
import { periodLabel } from "../research/format";
import type { PackResponse, PricesResponse } from "../research/types";
import { dataGaps, hasGaps } from "./dataGaps";
import { compareDriver, driverAtQuarter, priceDriver, quarterDots, quarterEnd, ratioStats } from "./harga";
import bankPack from "../../../../tests/fixtures/research/packs/BANK.json";
import { latestHeadline } from "./blocks/AngkaKunci";
import { isBankTaxonomy, lastQuarterGroups, tableScale, unitCaption } from "./lastQuarter";

const GOLDEN = pack as unknown as PackResponse;
const PRICES = prices as unknown as PricesResponse;

describe("bank packs hide the core rows a bank cannot carry (C5)", () => {
  const BANK = bankPack as unknown as PackResponse;
  const ids = (p: PackResponse, taxonomy: string | null) => lastQuarterGroups(p, taxonomy).flatMap((g) => g.rows.map((r) => r.metric.id));

  it("drops gross profit, capex, free cash flow, EBITDA and interest-bearing debt for a financial taxonomy", () => {
    const asOther = ids(BANK, "general");
    const asBank = ids(BANK, "financial_sharia");
    for (const id of ["gross_profit", "capex", "free_cash_flow", "ebitda", "total_debt", "net_debt"]) {
      expect(asBank).not.toContain(id);
    }
    expect(asBank).toContain("revenue");
    expect(asBank).toContain("net_income");
    expect(asBank.length).toBeLessThan(asOther.length);
    expect(lastQuarterGroups(BANK, "financial_sharia").every((g) => g.rows.length > 0)).toBe(true);
  });

  it("hides a core xbrl row that is empty over the whole history, for any taxonomy", () => {
    const empty = {
      ...GOLDEN,
      dimensions: GOLDEN.dimensions.map((d) => ({
        ...d,
        metrics: d.metrics.map((m) => (m.id === "gross_profit" ? { ...m, cells: m.cells.map((c) => ({ ...c, value: null })) } : m)),
      })),
    } as PackResponse;
    expect(ids(empty, "general")).not.toContain("gross_profit");
    expect(ids(GOLDEN, "general")).toContain("gross_profit");
  });

  it("the key-number headline is the first financing or profitability metric with a value, with no interest wording", () => {
    const h = latestHeadline(BANK, "IDR");
    expect(h).not.toBeNull();
    expect(h!.label).toMatch(/Q\d \d\d/);
    expect(`${h!.label} ${h!.value}`.toLowerCase()).not.toContain("bunga");
    const none = { ...BANK, dimensions: BANK.dimensions.filter((d) => d.id !== "pembiayaan" && d.id !== "profitabilitas") } as PackResponse;
    expect(latestHeadline(none, "IDR")).toBeNull();
  });

  it("recognises bank taxonomies only", () => {
    expect(isBankTaxonomy("financial_sharia")).toBe(true);
    expect(isBankTaxonomy("general")).toBe(false);
    expect(isBankTaxonomy(null)).toBe(false);
  });
});

describe("one amount scale per ticker table (M4)", () => {
  const cell = (value: number) => ({ fy: 2026, q: 2, label: "2026Q2", value, gap: null, flags: [], inputs: [] });
  const row = (id: string, latest: number, prior: number) => ({ metric: { id, unit: "ccy" }, latest: cell(latest), prior: cell(prior) });
  const groups = (revenue: number, gross: number) => [{ id: "core.laba_rugi", label: "Laba rugi", core: true, rows: [row("revenue", revenue, revenue * 0.9), row("gross_profit", gross, gross * 0.8)] }] as never;

  it("takes the scale of revenue, so a gross profit several times smaller stays in the same unit", () => {
    // BUMI: revenue in the hundreds of millions of US$, gross profit tens of millions: US$ jt for both.
    const scale = tableScale(groups(449_000_000, 63_059_000), "USD");
    expect(scale?.word).toBe("juta");
    expect(unitCaption("ccy", "USD", scale)).toBe("USD jt");
  });

  it("is null without revenue, and a thousand times smaller revenue gives a smaller scale", () => {
    expect(tableScale([{ id: "x", label: "x", core: true, rows: [] }], "USD")).toBeNull();
    expect(tableScale(groups(449_000, 63_059), "USD")?.word).toBe("ribu");
  });
});

describe("unit caption under the metric label", () => {
  const scale = (word: string, divisor: number) => ({ word, divisor, header: "" });
  it("abbreviates the scale and names the currency", () => {
    expect(unitCaption("ccy", "USD", scale("ribu", 1e3))).toBe("USD rb");
    expect(unitCaption("ccy", "IDR", scale("triliun", 1e12))).toBe("Rp T");
    expect(unitCaption("ccy", "IDR", scale("juta", 1e6))).toBe("Rp jt");
    expect(unitCaption("ccy", "IDR", scale("", 1))).toBe("Rp");
    expect(unitCaption("ccy", null, scale("miliar", 1e9))).toBe("mata uang laporan M");
  });
  it("passes other units through", () => {
    expect(unitCaption("%", "IDR", null)).toBe("%");
    expect(unitCaption("x", "IDR", null)).toBe("×");
    expect(unitCaption("Mt", "USD", null)).toBe("Mt");
  });
});

describe("block 3 grouping from a real-shaped PackResponse", () => {
  const groups = lastQuarterGroups(GOLDEN);

  it("one group per dimension label in payload order, core dimensions flagged", () => {
    expect(groups.map((g) => g.label)).toEqual(["Laba rugi", "Arus kas", "Neraca", "Volume", "Harga", "Biaya", "Kualitas aset", "Cadangan"]);
    expect(groups.map((g) => g.core)).toEqual([true, true, true, false, false, false, false, false]);
    expect(new Set(groups.map((g) => g.id)).size).toBe(groups.length);
  });

  it("merges the core and the pack dimension that share a label into one group, core rows first", () => {
    const perDimension = GOLDEN.dimensions.filter((d) => d.label === "Neraca");
    expect(perDimension).toHaveLength(2);
    const neraca = groups.find((g) => g.label === "Neraca")!;
    expect(groups.filter((g) => g.label === "Neraca")).toHaveLength(1);
    expect(neraca.core).toBe(true);
    const expected = perDimension.flatMap((d) => d.metrics.map((m) => m.id)).filter((id) => neraca.rows.some((r) => r.metric.id === id));
    expect(neraca.rows.map((r) => r.metric.id)).toEqual(expected);
  });

  it("pairs the latest standalone quarter with the same quarter a year earlier", () => {
    const rows = groups.flatMap((g) => g.rows);
    const revenue = rows.find((r) => r.metric.id === "revenue")!;
    expect(revenue.latest).toMatchObject({ fy: 2026, q: 2 });
    expect(revenue.prior).toMatchObject({ fy: 2025, q: 2 });
    for (const r of rows) {
      if (r.latest) expect([r.latest.fy, r.latest.q]).toEqual([GOLDEN.latest.fy, GOLDEN.latest.q]);
      if (r.prior) expect([r.prior.fy, r.prior.q]).toEqual([GOLDEN.latest.fy - 1, GOLDEN.latest.q]);
    }
  });

  it("an entered KPI keeps its value, an empty one keeps its Isi action", () => {
    const rows = groups.flatMap((g) => g.rows);
    expect(rows.find((r) => r.metric.id === "production_mt")!.latest).toMatchObject({ state: "entered", value: 6 });
    expect(rows.find((r) => r.metric.id === "asp_usd_t")!.latest).toMatchObject({ state: "not_entered", action: "Isi" });
  });

  it("counts every metric exactly once", () => {
    const total = GOLDEN.dimensions.reduce((n, d) => n + d.metrics.length, 0);
    expect(groups.reduce((n, g) => n + g.rows.length, 0)).toBe(total);
  });
});

describe("block 6 data gaps from the pack payload", () => {
  it("lists unentered KPI cells by label and period, and drivers by what is missing", () => {
    const g = dataGaps(GOLDEN);
    expect(g.kpi).toContain("Produksi Q3 25");
    expect(g.kpi).toHaveLength(GOLDEN.coverage.kpi.missing.length);
    const newcastle = g.drivers.find((d) => d.id === "newcastle")!;
    expect(newcastle).toMatchObject({ text: "belum diambil", fetchable: true });
    const hba = g.drivers.find((d) => d.id === "hba")!;
    expect(hba).toMatchObject({ fetchable: false });
    const proxy = GOLDEN.drivers.find((d) => d.id === "newcastle")!.label;
    expect(hba.text).toBe(`belum ada sumber, ${proxy} dipakai sebagai pengganti`);
    expect(hasGaps(g)).toBe(true);
  });

  it("a quarter with no filing at all is reported as a gap, not an error", () => {
    const quarter = GOLDEN.quarters[2];
    const blank: PackResponse = {
      ...GOLDEN,
      dimensions: GOLDEN.dimensions.map((d) => ({
        ...d,
        metrics: d.metrics.map((m) =>
          m.layer === "xbrl" ? { ...m, cells: m.cells.map((c) => (c.fy === quarter.fy && c.q === quarter.q ? { ...c, value: null, gap: { kind: "missing" as const, text: "Tidak ada" } } : c)) } : m,
        ),
      })),
    };
    const g = dataGaps(blank);
    expect(g.quarters).toHaveLength(1);
    expect(g.quarters[0]).toMatch(/tidak ada laporan untuk kuartal ini/);
  });

  it("says what is missing in the metric's own words, one line per metric, never a raw element name (C6)", () => {
    const latest = GOLDEN.latest;
    const gapped: PackResponse = {
      ...GOLDEN,
      dimensions: GOLDEN.dimensions.map((d) => ({
        ...d,
        metrics: d.metrics.map((m) => {
          if (m.layer !== "xbrl" || (m.id !== "capex" && m.id !== "gross_profit")) return m;
          const ident = m.id === "capex" ? "Capex" : "Ocf";
          const gap = { kind: "missing" as const, ident, text: `${ident} tidak ada untuk ${latest.label} (kuartal)` };
          return { ...m, cells: m.cells.map((c) => (c.fy === latest.fy && c.q === latest.q ? { ...c, value: null, gap } : c)) };
        }),
      })),
    };
    const lines = dataGaps(gapped).xbrl;
    const capex = lines.filter((l) => l.startsWith("Belanja modal:"));
    expect(capex).toEqual([`Belanja modal: tidak ada di laporan ${periodLabel(latest.label)}`]);
    // an input of the formula that is missing is named as an input, not by its element
    expect(lines.some((l) => l.startsWith("Laba kotor: bahan perhitungannya tidak ada di laporan"))).toBe(true);
    for (const l of lines) expect(l).not.toMatch(/Capex|GrossProfit|Ocf|NonCurrentStripping/);
  });

  it("several quarters without a filing are one line, not one per quarter (C6)", () => {
    const quarters = [GOLDEN.quarters[1], GOLDEN.quarters[2]];
    const blank: PackResponse = {
      ...GOLDEN,
      dimensions: GOLDEN.dimensions.map((d) => ({
        ...d,
        metrics: d.metrics.map((m) =>
          m.layer === "xbrl" ? { ...m, cells: m.cells.map((c) => (quarters.some((q) => q.fy === c.fy && q.q === c.q) ? { ...c, value: null, gap: { kind: "missing" as const, text: "Tidak ada" } } : c)) } : m,
        ),
      })),
    };
    const g = dataGaps(blank);
    expect(g.quarters).toHaveLength(1);
    expect(g.quarters[0]).toMatch(/^Q\d \d\d, Q\d \d\d: tidak ada laporan untuk kuartal-kuartal ini/);
  });

  it("a bank's hidden core rows are not counted as gaps", () => {
    const BANK = bankPack as unknown as PackResponse;
    const asBank = dataGaps(BANK, "financial_sharia").xbrl.join("\n");
    expect(asBank).not.toMatch(/Laba kotor|Belanja modal|Arus kas bebas|EBITDA|utang/i);
  });

  it("nothing missing hides the block", () => {
    const clean: PackResponse = { ...GOLDEN, coverage: { ...GOLDEN.coverage, kpi: { ...GOLDEN.coverage.kpi, missing: [] } }, drivers: [], missing_drivers: [] };
    expect(hasGaps(dataGaps({ ...clean, dimensions: clean.dimensions.map((d) => ({ ...d, metrics: d.metrics.filter((m) => m.layer !== "xbrl") })) }))).toBe(false);
  });
});

describe("block 8 driver ↔ metric comparison (G2)", () => {
  it("quarter ends are calendar quarter ends", () => {
    expect([1, 2, 3, 4].map((q) => quarterEnd(2026, q))).toEqual(["2026-03-31", "2026-06-30", "2026-09-30", "2026-12-31"]);
  });

  it("the driver linked by compare_metric is the one compared; an empty series is kept so the gap can be written", () => {
    const d = compareDriver(PRICES)!;
    expect(d.id).toBe("newcastle");
    expect(d.metricId).toBe("asp_usd_t");
    expect(priceDriver(PRICES)).toBeNull();
  });

  const asp = GOLDEN.dimensions.flatMap((x) => x.metrics).find((m) => m.id === "asp_usd_t")!;

  it("dots come only from entered values; empty quarters are markers without a value", () => {
    const dots = quarterDots(GOLDEN, asp, "2025-01-01", "2026-12-31");
    expect(dots.length).toBeGreaterThan(0);
    expect(dots.every((d) => (d.value === null) === (d.state !== "entered"))).toBe(true);
    expect(dots.find((d) => d.label === "2026Q2")).toMatchObject({ value: null, state: "empty", date: "2026-06-30" });
  });

  it("the ratio is reported ÷ driver per quarter, only where both exist", () => {
    const dots = [
      { label: "2026Q1", date: "2026-03-31", value: 60, state: "entered" as const },
      { label: "2026Q2", date: "2026-06-30", value: 54, state: "entered" as const },
      { label: "2025Q4", date: "2025-12-31", value: null, state: "empty" as const },
    ];
    const series: Array<readonly [string, number]> = [
      ["2026-01-15", 100],
      ["2026-02-15", 100],
      ["2026-03-15", 100],
      ["2026-04-15", 90],
      ["2026-05-15", 90],
      ["2026-06-15", 90],
    ];
    expect(driverAtQuarter(series, 2026, 2)).toBeCloseTo(90);
    const r = ratioStats(dots, series);
    expect(r.latest).toEqual({ label: "2026Q2", ratio: 54 / 90 });
    expect(r.average?.n).toBe(2);
    expect(r.average?.ratio).toBeCloseTo((0.6 + 0.6) / 2);
    expect(ratioStats([], series)).toEqual({ latest: null, average: null });
  });
});
