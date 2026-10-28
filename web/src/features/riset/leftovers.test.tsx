import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { CompareResponse, PackDefinition } from "../research/types";
import { MIXED_HEADER, columnScales, columnUnit, moneyRowHeader } from "./compareScale";
import { DriverVsReported } from "./HargaCharts";
import { kpiMoneyUnit, quarterTableLabel } from "./lastQuarter";
import { humanize, metricLabels, readableFormula } from "./packFormula";
import { displayText } from "./structureText";

describe("structure text at render time (N5)", () => {
  it("writes English-grouped numbers in id-ID and one casing for Mt", () => {
    expect(displayText("Cadangan 1,175,412,847 ton (MT)")).toBe("Cadangan 1.175.412.847 ton (Mt)");
    expect(displayText("Produksi 52 MT per tahun")).toBe("Produksi 52 Mt per tahun");
    expect(displayText("Produksi 12 juta MT")).toBe("Produksi 12 juta Mt");
    expect(displayText("Margin 12.5% dan 0.125")).toBe("Margin 12,5% dan 0,125");
    expect(displayText("1,234.56 ton")).toBe("1.234,56 ton");
  });

  it("leaves id-ID numbers, years, dates and names alone, so it is idempotent", () => {
    for (const s of ["1.175.412.847 ton", "2,5 juta", "FY 2025", "dilaporkan 12.05.2025", "MT Haryono", "(Mt)", "versi 1.175 final"]) {
      expect(displayText(s)).toBe(s);
      expect(displayText(displayText(s))).toBe(displayText(s));
    }
  });
});

describe("pack formulas read with labels (N6)", () => {
  const labels = new Map([
    ["third_party_funds", "Dana pihak ketiga"],
    ["total_financing", "Total pembiayaan"],
  ]);

  it("names identifiers from their words and bound metrics from their labels, keeping functions", () => {
    expect(humanize("WadiahDemand")).toBe("Wadiah demand");
    expect(humanize("NpfGross")).toBe("Npf gross");
    const bound = { formula: "100 * ttm(ProfitSharingExpense) / avg4q(ThirdPartyFunds)", bindings: { ProfitSharingExpense: ["idx-cor:ProfitSharingExpense"], ThirdPartyFunds: ["metric:third_party_funds"] } };
    expect(readableFormula(bound, labels)).toBe("100 * TTM(Profit sharing expense) / rata-rata 4 kuartal(Dana pihak ketiga)");
    expect(readableFormula({ formula: "100 * Financing / ThirdPartyFunds", bindings: { Financing: ["metric:total_financing"], ThirdPartyFunds: ["metric:third_party_funds"] } }, labels)).toBe("100 * Total pembiayaan / Dana pihak ketiga");
  });

  it("reads an XBRL element another metric already labels with that metric's Indonesian label, and falls back to the words only without one (Nit 2)", () => {
    const metric = (id: string, label: string, formula: string, bindings: Record<string, string[]>) => ({ id, label, layer: "xbrl", formula, bindings }) as unknown as PackDefinition["dimensions"][number]["metrics"][number];
    const packs = [
      {
        dimensions: [
          {
            metrics: [
              metric("revenue", "Pendapatan", "Revenue", { Revenue: ["idx-cor:SalesAndRevenue", "idx-cor:InterestIncome"] }),
              metric("capex", "Belanja modal", "Capex", { Capex: ["abs:idx-cor:PaymentsForAcquisitionOfPropertyPlantAndEquipment"] }),
              metric("fcf", "Arus kas bebas", "Ocf - Capex", { Ocf: ["idx-cor:NetCashFlows"], Capex: ["abs:idx-cor:PaymentsForAcquisitionOfPropertyPlantAndEquipment"] }),
            ],
          },
        ],
      },
    ] as unknown as PackDefinition[];
    const map = metricLabels(packs);
    // Only metrics whose formula is one identifier label their elements; `fcf` (two identifiers) does not label `Ocf`.
    expect(map.get("element:idx-cor:InterestIncome")).toBe("Pendapatan");
    expect(map.get("element:idx-cor:PaymentsForAcquisitionOfPropertyPlantAndEquipment")).toBe("Belanja modal");
    expect(map.has("element:idx-cor:NetCashFlows")).toBe(false);
    const margin = { formula: "100 * ttm(Gross) / ttm(Revenue)", bindings: { Gross: ["idx-cor:GrossProfit"], Revenue: ["idx-cor:Other", "idx-cor:InterestIncome"] } };
    expect(readableFormula(margin, map)).toBe("100 * TTM(Gross) / TTM(Pendapatan)");
    expect(readableFormula({ formula: "Ocf - Capex", bindings: { Ocf: ["idx-cor:NetCashFlows"], Capex: ["abs:idx-cor:PaymentsForAcquisitionOfPropertyPlantAndEquipment"] } }, map)).toBe("Ocf - Belanja modal");
    // A metric binding still wins over an element label, and a literal binding never becomes an element.
    expect(readableFormula({ formula: "Revenue", bindings: { Revenue: ["metric:third_party_funds", "idx-cor:InterestIncome"] } }, new Map([...map, ["third_party_funds", "Dana pihak ketiga"]]))).toBe("Dana pihak ketiga");
    expect(readableFormula({ formula: "Depreciation", bindings: { Depreciation: ["0"] } }, map)).toBe("Depreciation");
  });

  it("is null without a formula, and an unknown metric binding falls back to the identifier's words", () => {
    expect(readableFormula({ formula: null, bindings: null }, labels)).toBeNull();
    expect(readableFormula({ formula: "Cash", bindings: { Cash: ["metric:nope"] } }, labels)).toBe("Cash");
  });
});

describe("Bandingkan money scale and TTM label (N8, N9)", () => {
  const row = (unit: string, values: Array<number | null>) => ({ group: "core", group_label: "Inti", metric: { id: unit, label: unit, unit, layer: "xbrl" as const, direction: "higher" as const }, cells: values.map((value, i) => ({ ticker: `T${i}`, value, period: "2026Q2", percentile: null, state: "value" as const, fact: null, reason: null, flags: [] })) });
  const data = (tickers: Array<{ currency: string | null }>, rows: ReturnType<typeof row>[]) => ({ tickers: tickers.map((t, i) => ({ ticker: `T${i}`, name: null, role: "held" as const, currency: t.currency })), rows }) as Pick<CompareResponse, "tickers" | "rows">;

  it("every Rp reporter is in miliar whatever the size of the row, so Rp M and Rp T never mix in a column", () => {
    const scales = columnScales(data([{ currency: "IDR" }, { currency: null }], [row("ccy", [51_041_000_000, 3_000_000_000]), row("ccy", [349_000_000_000_000, null])]));
    expect([...scales.values()].map((s) => s.scale.divisor)).toEqual([1e9, 1e9]);
    expect(moneyRowHeader(scales)).toBe("(RP MILIAR)");
  });

  it("a USD reporter picks its own scale from all its money rows, and mixed columns say their unit in the header", () => {
    const scales = columnScales(data([{ currency: "IDR" }, { currency: "USD" }], [row("ccy", [51_041_000_000, 449_000]), row("ccy", [349_000_000_000, 63_059])]));
    expect(scales.get("T1")?.scale.word).toBe("ribu");
    expect(columnUnit(scales.get("T1")!)).toBe("USD rb");
    expect(columnUnit(scales.get("T0")!)).toBe("Rp M");
    expect(moneyRowHeader(scales)).toBe(MIXED_HEADER);
  });

  it("USD reporters that share a scale share the row header", () => {
    const scales = columnScales(data([{ currency: "USD" }, { currency: "USD" }], [row("ccy", [449_000, 380_000]), row("ccy", [63_059_000, 55_000_000])]));
    expect(moneyRowHeader(scales)).toBe("(USD RIBU)");
  });

  it("money KPIs written as `USD juta` convert to full units, other units do not", () => {
    expect(kpiMoneyUnit("USD juta")).toEqual({ currency: "USD", multiplier: 1e6 });
    expect(kpiMoneyUnit("IDR miliar")).toEqual({ currency: "IDR", multiplier: 1e9 });
    expect(kpiMoneyUnit("Mt")).toBeNull();
    expect(kpiMoneyUnit("USD/t")).toBeNull();
  });

  it("the TTM suffix is one helper for the detail page and the compare rows, tolerant of a missing period", () => {
    expect(quarterTableLabel({ label: "Margin bersih (%)", period: "ttm" })).toBe("Margin bersih (%) (TTM)");
    expect(quarterTableLabel({ label: "Pendapatan", period: "standalone" })).toBe("Pendapatan");
    expect(quarterTableLabel({ label: "Pendapatan", period: null })).toBe("Pendapatan");
    expect(quarterTableLabel({ label: "Pendapatan" })).toBe("Pendapatan");
  });
});

describe("chart data tables name periods like the visible table (N10)", () => {
  it("the screen-reader table headers are Q4 23, not 2023Q4", () => {
    const dots = [
      { label: "2023Q4", date: "2023-12-31", value: 10, state: "entered" as const },
      { label: "2024Q1", date: "2024-03-31", value: null, state: "empty" as const },
    ];
    const { container } = render(<DriverVsReported from="2023-10-01" to="2024-06-30" driverLabel="Newcastle" driverUnit="USD/t" proxyLabel={null} driver={[["2023-10-31", 120], ["2024-01-31", 130]]} metricLabel="Tarif" metricUnit="USD/hari" dots={dots} sameUnit={false} />);
    const heads = [...container.querySelectorAll(".sr-only-table tbody th")].map((th) => th.textContent);
    expect(heads).toEqual(["Q4 23", "Q1 24"]);
    expect(container.querySelector(".sr-only")?.textContent).not.toMatch(/20\d\dQ\d/);
  });
});
