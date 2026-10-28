import { describe, expect, it } from "vitest";
import pack from "../../../../tests/fixtures/research/canon/pack-COAL.json";
import type { MetricDirection, PackMetric, PackResponse, YoY } from "../research/types";
import { computeMovers, moverSentence } from "./movers";

const GOLDEN = pack as unknown as PackResponse;

function metric(id: string, label: string, direction: MetricDirection, yoy: YoY | null): PackMetric {
  return { id, label, layer: "kpi", unit: "x", direction, why: "", note: null, cells: [], latest: null, yoy, percentile: null, peers: [] };
}

/** Paket nyata (golden backend) dengan metrik diganti, supaya bentuk payload tetap yang dikirim backend. */
function withMetrics(metrics: PackMetric[], kpiMissing: string[] = []): PackResponse {
  return {
    ...GOLDEN,
    dimensions: [{ ...GOLDEN.dimensions[0], metrics }],
    coverage: { ...GOLDEN.coverage, kpi: { ...GOLDEN.coverage.kpi, missing: kpiMissing } },
  };
}

describe("Membaik / Diawasi from pack yoy × direction (D1)", () => {
  it("a lower-is-better metric that fell improves, a higher-is-better one that fell is watched", () => {
    const m = computeMovers(
      withMetrics([
        metric("cash_cost", "Biaya tunai", "lower", { kind: "pct", value: -8.2 }),
        metric("production", "Produksi", "higher", { kind: "pct", value: -12 }),
        metric("asp", "Harga jual rata-rata", "higher", { kind: "pct", value: 3.4 }),
        metric("strip", "Nisbah kupas", "lower", { kind: "pct", value: 5 }),
      ]),
    );
    expect(m.improving.map((x) => x.sentence)).toEqual(["Biaya tunai turun 8,2% YoY", "Harga jual rata-rata naik 3,4% YoY"]);
    expect(m.watching.map((x) => x.sentence)).toEqual(["Produksi turun 12,0% YoY", "Nisbah kupas naik 5,0% YoY"]);
    expect(m.improvingEmpty).toBeNull();
    expect(m.watchingEmpty).toBeNull();
  });

  it("excludes neutral, null and zero yoy, however large", () => {
    const m = computeMovers(
      withMetrics([
        metric("capex", "Belanja modal", "neutral", { kind: "pct", value: 90 }),
        metric("missing", "Cadangan", "higher", null),
        metric("flat", "Royalti", "lower", { kind: "pp", value: 0 }),
        metric("real", "Produksi", "higher", { kind: "pct", value: 1.5 }),
      ]),
    );
    expect(m.improving.map((x) => x.id)).toEqual(["real"]);
    expect(m.watching).toEqual([]);
  });

  it("keeps the three largest moves per list and writes percentage-point metrics in points", () => {
    const rows = [5, 20, 10, 40, 30].map((v, i) => metric(`up${i}`, `Naik ${v}`, "higher", { kind: "pct", value: v }));
    const m = computeMovers(withMetrics([...rows, metric("margin", "Margin bersih", "higher", { kind: "pp", value: -0.7 })]));
    expect(m.improving.map((x) => x.id)).toEqual(["up3", "up4", "up1"]);
    expect(m.watching.map((x) => x.sentence)).toEqual(["Margin bersih turun 0,7 poin YoY"]);
    expect(moverSentence("Margin", { kind: "pp", value: 1.25 })).toBe("Margin naik 1,3 poin YoY");
  });

  it("says why nothing qualifies: unfilled KPIs, or no prior-year comparison, or no directional change", () => {
    const noYoy = [metric("a", "A", "higher", null), metric("b", "B", "lower", null)];
    const kpi = computeMovers(withMetrics(noYoy, ["production_mt 2026Q2"]));
    expect(kpi.improving).toEqual([]);
    expect(kpi.improvingEmpty).toBe("KPI belum diisi, jadi belum ada pembanding tahun lalu");
    expect(kpi.watchingEmpty).toBe(kpi.improvingEmpty);
    expect(computeMovers(withMetrics(noYoy)).improvingEmpty).toBe("Belum ada pembanding tahun lalu");
    expect(computeMovers(withMetrics([metric("n", "N", "neutral", { kind: "pct", value: 4 })])).watchingEmpty).toBe("Tidak ada metrik berarah yang berubah dibanding tahun lalu");
  });

  it("a negative base that got worse is watched even though the backend pct is now negative (BUMI free cash flow −27.163 → −124.789)", () => {
    const m = computeMovers(
      withMetrics([
        metric("free_cash_flow", "Arus kas bebas", "higher", { kind: "pct", value: ((-124.789 + 27.163) / 27.163) * 100 }),
        metric("operating_cash_flow", "Arus kas operasi", "higher", { kind: "pct", value: ((-60 + 100) / 100) * 100 }),
      ]),
    );
    expect(m.watching.map((x) => x.sentence)).toEqual(["Arus kas bebas turun 359,4% YoY"]);
    expect(m.improving.map((x) => x.sentence)).toEqual(["Arus kas operasi naik 40,0% YoY"]);
  });

  it("a sign change qualifies with its sentence, ranks above any percent, and is judged by to − from × direction", () => {
    const m = computeMovers(
      withMetrics([
        metric("net_income", "Laba bersih", "higher", { kind: "sign_change", from: -3, to: 5 }),
        metric("operating_profit", "Laba usaha", "higher", { kind: "sign_change", from: 8, to: -1 }),
        metric("debt", "Rasio utang", "lower", { kind: "sign_change", from: 2, to: -1 }),
        metric("revenue", "Pendapatan", "higher", { kind: "pct", value: 500 }),
      ]),
    );
    expect(m.improving.map((x) => x.sentence)).toEqual(["Laba bersih berbalik dari rugi ke laba YoY", "Rasio utang berbalik dari positif ke negatif YoY", "Pendapatan naik 500,0% YoY"]);
    expect(m.watching.map((x) => x.sentence)).toEqual(["Laba usaha berbalik dari laba ke rugi YoY"]);
  });

  it("leaves out a ratio whose yoy the backend marked not meaningful (negative denominator)", () => {
    const meaningless = { ...metric("net_debt_to_ebitda", "Utang bersih / EBITDA", "lower", null), yoy_reason: "tidak bermakna: penyebut negatif" };
    const m = computeMovers(withMetrics([meaningless, metric("real", "Produksi", "higher", { kind: "pct", value: 2 })]));
    expect(m.improving.map((x) => x.id)).toEqual(["real"]);
    expect(m.watching).toEqual([]);
  });

  it("when only one side has movers the other says so plainly", () => {
    const m = computeMovers(withMetrics([metric("p", "Produksi", "higher", { kind: "pct", value: 2 })]));
    expect(m.watchingEmpty).toBe("Belum ada metrik yang memburuk dibanding tahun lalu");
  });

  it("reads the real COAL payload: every core yoy is favourable, ranked by size, neutral capex left out", () => {
    const m = computeMovers(GOLDEN);
    expect(m.improving).toHaveLength(3);
    const sizes = m.improving.map((x) => (x.yoy.kind === "sign_change" ? Infinity : Math.abs(x.yoy.value)));
    expect(sizes).toEqual([...sizes].sort((a, b) => b - a));
    expect(m.improving.map((x) => x.id)).not.toContain("capex");
    // A multiple moves by a difference in times (N2), so it reads "turun 0,1× YoY" and no longer outranks percent moves.
    const ratio = GOLDEN.dimensions.flatMap((d) => d.metrics).find((x) => x.id === "net_debt_to_ebitda")!;
    expect(ratio.yoy?.kind).toBe("x");
    expect(moverSentence(ratio.label, ratio.yoy!)).toMatch(/ turun \d+,\d× YoY$/);
    expect(m.improving[0].id).not.toBe("net_debt_to_ebitda");
    expect(m.watching).toEqual([]);
  });
});
