import { describe, expect, it } from "vitest";
import {
  MINUS,
  chooseScale,
  formatAge,
  formatDate,
  formatDay,
  formatDelta,
  formatExact,
  formatMultiple,
  formatPct,
  formatScaled,
  formatValue,
  periodLabel,
  scaleTitle,
  unitHeader,
} from "./format";

const digits = (text: string) => text.replace(/[^0-9]/g, "").length;

describe("number formatting", () => {
  it("groups with dots, uses a comma decimal and U+2212 for negatives", () => {
    expect(formatValue(1234567)).toBe("1.234.567");
    expect(formatValue(-1234.5, 1)).toBe(`${MINUS}1.234,5`);
    expect(formatValue(-0.04)).toBe("0");
    expect(MINUS.codePointAt(0)).toBe(0x2212);
  });

  it("percent has one decimal and no percent sign", () => {
    expect(formatPct(28.34)).toBe("28,3");
    expect(formatPct(-1.25)).toBe(`${MINUS}1,3`);
    expect(formatPct(9)).toBe("9,0");
    expect(formatPct(9)).not.toContain("%");
  });

  it("multiples have one decimal and no multiplication sign", () => {
    expect(formatMultiple(12.34)).toBe("12,3");
    expect(formatMultiple(12.34)).not.toContain("×");
  });

  it("deltas carry an explicit sign", () => {
    expect(formatDelta(2.14)).toBe("+2,1");
    expect(formatDelta(-1.4)).toBe(`${MINUS}1,4`);
    expect(formatDelta(0)).toBe("0,0");
  });

  it("never prints a currency symbol", () => {
    for (const text of [formatValue(1500), formatPct(3), formatMultiple(2), formatDelta(1)]) {
      expect(text).not.toMatch(/Rp|%|×|USD/);
    }
  });
});

describe("chooseScale", () => {
  it("picks the scale that leaves the median with 3 to 6 digits", () => {
    for (const mid of [150, 4_321, 98_765, 1_500_000, 82_000_000_000, 7_400_000_000_000, 3.2e15]) {
      const scale = chooseScale([mid * 0.4, mid, mid * 3], "IDR");
      const n = digits(formatScaled(mid, scale));
      expect(n).toBeGreaterThanOrEqual(3);
      expect(n).toBeLessThanOrEqual(6);
    }
  });

  it("returns the RP MILIAR style header suffix", () => {
    const scale = chooseScale([182_000_000_000, 191_000_000_000, 160_000_000_000], "IDR");
    expect(scale.header).toBe("(RP MILIAR)");
    expect(scale.word).toBe("miliar");
    expect(chooseScale([250_000_000_000, 310_000_000_000], "USD").header).toBe("(USD MILIAR)");
    expect(chooseScale([400_000_000, 600_000_000], "IDR").header).toBe("(RP JUTA)");
  });

  it("uses the median, so one outlier does not move the scale", () => {
    const scale = chooseScale([300_000_000, 320_000_000, 310_000_000, 9_000_000_000_000], "IDR");
    expect(scale.header).toBe("(RP JUTA)");
  });

  it("ignores gaps and falls back to units without data", () => {
    expect(chooseScale([null, undefined, 500_000_000], "IDR").header).toBe("(RP JUTA)");
    expect(chooseScale([], "IDR")).toMatchObject({ divisor: 1, header: "(RP)", word: "" });
    expect(chooseScale([12, 40], "IDR").header).toBe("(RP)");
  });

  it("uses magnitudes, not signs", () => {
    expect(chooseScale([-800_000_000_000, -900_000_000_000], "IDR").header).toBe("(RP MILIAR)");
  });

  it("states the scale in the table title", () => {
    const scale = chooseScale([500e9, 600e9], "IDR");
    expect(scaleTitle("Laba rugi", scale)).toBe("Laba rugi · dalam miliar");
    expect(scaleTitle("Laba rugi", chooseScale([12], "IDR"))).toBe("Laba rugi");
  });
});

describe("unit headers", () => {
  it.each([
    ["%", "(%)"],
    ["x", "(×)"],
    ["pp", "(PP)"],
    ["USD/t", "(USD/T)"],
    ["Mt", "(MT)"],
    ["IDR miliar", "(RP MILIAR)"],
    ["USD juta", "(USD JUTA)"],
    ["hari", "(HARI)"],
  ])("%s → %s", (unit, header) => {
    expect(unitHeader(unit)).toBe(header);
  });

  it("resolves the filing currency", () => {
    expect(unitHeader("ccy", "IDR")).toBe("(RP)");
    expect(unitHeader("ccy", "USD")).toBe("(USD)");
  });
});

describe("age, period and date text", () => {
  it("formats ages", () => {
    expect(formatAge(0)).toBe("hari ini");
    expect(formatAge(3)).toBe("3 hari");
    expect(formatAge(90)).toBe("3 bulan");
    expect(formatAge(800)).toBe("2 tahun");
    expect(formatAge(-2)).toBe("hari ini");
  });

  it("shortens period labels", () => {
    expect(periodLabel("2026Q2")).toBe("Q2 26");
    expect(periodLabel("2024Q4")).toBe("Q4 24");
    expect(periodLabel("2025")).toBe("FY 25");
    expect(periodLabel("TTM")).toBe("TTM");
  });

  it("formats dates in Indonesian without a locale lookup", () => {
    expect(formatDate("2026-10-24")).toBe("24 Okt 2026");
    expect(formatDate("2026-05-03T10:00:00+07:00")).toBe("3 Mei 2026");
    expect(formatDay("2026-08-29")).toBe("29 Agu");
    expect(formatDate("belum")).toBe("belum");
  });
});

describe("exact values, rounding and boundaries", () => {
  it("formatExact keeps the reported fraction without trailing zeros", () => {
    expect(formatExact(12.5)).toBe("12,5");
    expect(formatExact(-0.123456)).toBe(`${MINUS}0,123456`);
    expect(formatExact(250_000_000_000)).toBe("250.000.000.000");
    expect(formatExact(0)).toBe("0");
  });

  it("values that round to zero carry no sign", () => {
    expect(formatPct(-0.04)).toBe("0,0");
    expect(formatDelta(0.04)).toBe("0,0");
    expect(formatDelta(-0.04)).toBe("0,0");
  });

  it("a non-zero scaled value is never printed as a bare 0", () => {
    const miliar = chooseScale([500e9, 600e9], "IDR");
    expect(formatScaled(400_000_000, miliar)).toBe("0,4");
    expect(formatScaled(-30_000_000, miliar)).toBe(`${MINUS}0,03`);
    expect(formatScaled(1_000_000, miliar)).toBe("<0,01");
    expect(formatScaled(0, miliar)).toBe("0");
    expect(formatScaled(500_000_000, miliar)).toBe("1");
  });

  it("every value formatter produces digits and separators only", () => {
    const scale = chooseScale([5e11], "IDR");
    const inputs = [-1e12, -2500, -0.04, 0, 0.4, 7, 1234.5, 1e15];
    for (const v of inputs) {
      for (const text of [formatValue(v), formatPct(v), formatMultiple(v), formatDelta(v), formatScaled(v, scale), formatExact(v)]) {
        expect(text, `${v}`).toMatch(/^[−+]?<?[0-9.,]+$/);
      }
    }
  });

  it("chooseScale at the 100 boundary of every step", () => {
    const header = (median: number) => chooseScale([median], "IDR").header;
    expect(header(99)).toBe("(RP)");
    expect(header(100)).toBe("(RP)");
    expect(header(99_999)).toBe("(RP)");
    expect(header(100_000)).toBe("(RP RIBU)");
    expect(header(99_999_999)).toBe("(RP RIBU)");
    expect(header(100_000_000)).toBe("(RP JUTA)");
    expect(header(100_000_000_000)).toBe("(RP MILIAR)");
    expect(header(100_000_000_000_000)).toBe("(RP TRILIUN)");
    expect(header(1e17)).toBe("(RP TRILIUN)"); // no step above triliun
  });

  it("an all-gap column in another currency still gets a header", () => {
    expect(chooseScale([null, null], "usd").header).toBe("(USD)");
  });
});

describe("dates and ages", () => {
  it("shows the WIB day of a UTC timestamp", () => {
    expect(formatDate("2026-10-24T20:00:00+00:00")).toBe("25 Okt 2026");
    expect(formatDate("2026-10-24T16:59:00+00:00")).toBe("24 Okt 2026");
    expect(formatDate("2026-10-25T03:00:00+07:00")).toBe("25 Okt 2026");
  });

  it("does not invent a day for an impossible date", () => {
    expect(formatDate("2026-02-31")).toBe("2026-02-31");
    expect(formatDate("2026-13-01")).toBe("2026-13-01");
  });

  it("ages: month and year boundaries, and non-numbers", () => {
    expect(formatAge(44)).toBe("44 hari");
    expect(formatAge(45)).toBe("2 bulan");
    expect(formatAge(Number.NaN)).toBe("");
  });

  it("unit headers for per-share units are not rewritten to rupiah", () => {
    expect(unitHeader("IDR/shares")).toBe("(RP/SHARES)".replace("RP", "IDR"));
  });
});
