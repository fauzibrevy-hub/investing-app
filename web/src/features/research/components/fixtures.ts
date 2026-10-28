import type { FactDetail, FactTarget, FinancialsResponse } from "../types";

/**
 * Data sintetis bertipe untuk tes komponen murni (tidak ada angka dari Sheets). Tes yang memakai payload API
 * memakai golden backend di `tests/fixtures/research/**`.
 */

export const TARGET: FactTarget = {
  filing_id: "AAAA_2026_Q2",
  element: "idx-cor:ProfitLoss",
  ctx_kind: "duration",
  start: "2026-01-01",
  end: "2026-06-30",
  dims: "",
  ctx: "2026-01-01_2026-06-30",
};

export const OPERAND_TARGET: FactTarget = {
  ...TARGET,
  filing_id: "AAAA_2026_Q1",
  start: "2026-01-01",
  end: "2026-03-31",
  ctx: "2026-01-01_2026-03-31",
};

export const FACT_DETAIL: FactDetail = {
  fact: {
    filing_id: TARGET.filing_id,
    element: TARGET.element,
    label: { id: "Laba (rugi)", en: "Profit (loss)" },
    ctx_kind: "duration",
    start: "2026-01-01",
    end: "2026-06-30",
    dims: "",
    value: 250_000_000_000,
    unit: "IDR",
    decimals: -6,
    cite: null,
  },
  filing: {
    id: TARGET.filing_id,
    ticker: "AAAA",
    fy: 2026,
    period: "Q2",
    taxonomy: "general",
    industry_code: "A121",
    period_start: "2026-01-01",
    period_end: "2026-06-30",
    audited: false,
    rounding: "Jutaan",
    currency: "IDR",
    filed_at: null,
    available_from: "2026-08-29",
    available_source: "estimated",
    source_path: "data/XBRL/AAAA/2026/Q2/instance.xbrl",
    file_hash: "hash-a",
    source_kind: "xbrl",
    ingested_at: "2026-09-01T00:00:00+07:00",
    fact_count: 500,
  },
  history: [
    { filing_id: "AAAA_2026_Q2", value: 250_000_000_000, decimals: -6, source_kind: "xbrl", cite: null, available_from: "2026-08-29", original: true },
    { filing_id: "AAAA_2026_Q3", value: 262_000_000_000, decimals: -6, source_kind: "xbrl", cite: null, available_from: "2026-11-29", original: false },
  ],
  restatements: [
    {
      original: 250_000_000_000,
      original_filing: "AAAA_2026_Q2",
      restated: 262_000_000_000,
      restated_filing: "AAAA_2026_Q3",
      detected_at: "2026-11-30T09:00:00+07:00",
    },
  ],
  derived: [
    {
      fy: 2026,
      q: 2,
      view: "standalone",
      value: 150_000_000_000,
      flags: [],
      formula: "ytd_q2 − ytd_q1",
      inputs: [
        { role: "ytd_q2", value: 250_000_000_000, fact: TARGET, cite: null },
        { role: "ytd_q1", value: 100_000_000_000, fact: OPERAND_TARGET, cite: null },
      ],
      rules_version: "2026.10-r1",
    },
  ],
};

const target = (filing: string, end: string, start = "2026-01-01"): FactTarget => ({
  filing_id: filing,
  element: "idx-cor:Revenue",
  ctx_kind: "duration",
  start,
  end,
  dims: "",
  ctx: `${start}_${end}`,
});

const instant = (filing: string, end: string): FactTarget => ({
  filing_id: filing,
  element: "idx-cor:Equity",
  ctx_kind: "instant",
  start: null,
  end,
  dims: "",
  ctx: end,
});

/** Empat periode: Q3 25, Q4 25 (turunan dari FY), Q1 26, Q2 26 (satu sel kosong, satu disajikan kembali). */
export const FINANCIALS: FinancialsResponse = {
  ticker: "AAAA",
  view: "standalone",
  rules_version: "2026.10-r1",
  currency: "IDR",
  rounding: "Jutaan",
  taxonomy: "general",
  periods: [
    { fy: 2025, q: 3, label: "2025Q3", end: "2025-09-30", filing_id: "AAAA_2025_Q3", source_kind: "xbrl", audited: false, derived_fy: false },
    { fy: 2025, q: 4, label: "2025Q4", end: "2025-12-31", filing_id: "AAAA_2025_FY", source_kind: "xbrl", audited: true, derived_fy: true },
    { fy: 2026, q: 1, label: "2026Q1", end: "2026-03-31", filing_id: "AAAA_2026_Q1", source_kind: "xbrl", audited: false, derived_fy: false },
    { fy: 2026, q: 2, label: "2026Q2", end: "2026-06-30", filing_id: "AAAA_2026_Q2", source_kind: "xbrl", audited: false, derived_fy: false },
  ],
  groups: [
    {
      id: "laba_rugi",
      label: "Laba rugi",
      rows: [
        {
          id: "revenue",
          label: "Pendapatan",
          element: "idx-cor:Revenue",
          unit: "IDR",
          cells: [
            { value: 410_000_000_000, flags: [], fact: target("AAAA_2025_Q3", "2025-09-30"), inputs: 1 },
            { value: 450_000_000_000, flags: ["derived_fy"], fact: target("AAAA_2025_FY", "2025-12-31"), inputs: 2 },
            { value: 430_000_000_000, flags: [], fact: target("AAAA_2026_Q1", "2026-03-31"), inputs: 1 },
            { value: 470_000_000_000, flags: ["restated"], fact: target("AAAA_2026_Q2", "2026-06-30"), inputs: 2 },
          ],
        },
        {
          id: "net_income",
          label: "Laba bersih",
          element: "idx-cor:ProfitLoss",
          unit: "IDR",
          cells: [
            { value: -12_000_000_000, flags: [], fact: target("AAAA_2025_Q3", "2025-09-30"), inputs: 1 },
            { value: 30_000_000_000, flags: ["derived_fy"], fact: target("AAAA_2025_FY", "2025-12-31"), inputs: 2 },
            { value: 45_000_000_000, flags: [], fact: target("AAAA_2026_Q1", "2026-03-31"), inputs: 1 },
            { value: null, flags: ["gap"], fact: null, inputs: 0 },
          ],
        },
        {
          id: "eps",
          label: "Laba per saham",
          element: "idx-cor:BasicEarningsLossPerShare",
          unit: "IDR/shares",
          cells: [
            { value: 4.5, flags: [], fact: target("AAAA_2025_Q3", "2025-09-30"), inputs: 1 },
            { value: 6, flags: ["derived_fy"], fact: target("AAAA_2025_FY", "2025-12-31"), inputs: 2 },
            { value: 7.25, flags: ["from_comparative"], fact: target("AAAA_2026_Q1", "2026-03-31"), inputs: 1 },
            { value: null, flags: ["gap"], fact: null, inputs: 0 },
          ],
        },
      ],
    },
    {
      id: "neraca",
      label: "Neraca",
      rows: [
        {
          id: "equity",
          label: "Total ekuitas",
          element: "idx-cor:Equity",
          unit: "IDR",
          cells: [
            { value: 900_000_000_000, flags: ["instant"], fact: instant("AAAA_2025_Q3", "2025-09-30"), inputs: 1 },
            { value: 930_000_000_000, flags: ["instant"], fact: instant("AAAA_2025_FY", "2025-12-31"), inputs: 1 },
            { value: 975_000_000_000, flags: ["instant"], fact: instant("AAAA_2026_Q1", "2026-03-31"), inputs: 1 },
            { value: 1_010_000_000_000, flags: ["instant"], fact: instant("AAAA_2026_Q2", "2026-06-30"), inputs: 1 },
          ],
        },
      ],
    },
  ],
  shares: [null, null, null, null],
  filings: [],
};
