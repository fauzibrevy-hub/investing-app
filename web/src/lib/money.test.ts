import { describe, expect, it } from "vitest";
import { formatPercent, formatPp, formatRupiah, HIDDEN_BARE, HIDDEN_FULL, MINUS } from "./money";

describe("formatRupiah — penuh (§6.1)", () => {
  it.each([
    [6_480_000, "Rp6.480.000"],
    [389_000, "Rp389.000"],
    [74_000, "Rp74.000"],
    [0, "Rp0"],
    [999, "Rp999"],
    [1_234_567_890, "Rp1.234.567.890"],
  ])("%d → %s", (value, expected) => {
    expect(formatRupiah(value)).toBe(expected);
  });

  it("tanpa desimal (dibulatkan)", () => {
    expect(formatRupiah(1234.6)).toBe("Rp1.235");
    expect(formatRupiah(1234.4)).toBe("Rp1.234");
  });

  it("negatif memakai U+2212, bukan tanda hubung", () => {
    expect(formatRupiah(-6_480_000)).toBe(`${MINUS}Rp6.480.000`);
    expect(formatRupiah(-1)).not.toContain("-");
  });

  it("signed selalu bertanda", () => {
    expect(formatRupiah(1_200_000, { signed: true })).toBe("+Rp1.200.000");
    expect(formatRupiah(-1_200_000, { signed: true })).toBe(`${MINUS}Rp1.200.000`);
    expect(formatRupiah(0, { signed: true })).toBe("Rp0");
  });
});

describe("formatRupiah — singkat (§6.2)", () => {
  it.each([
    [190_000, "Rp190 rb"],
    [312_000, "Rp312 rb"],
    [189_600, "Rp190 rb"],
    [540_499, "Rp540 rb"],
    [6_480_000, "Rp6,48 jt"],
    [7_680_000, "Rp7,68 jt"],
    [1_520_000, "Rp1,52 jt"],
    [8_000_000, "Rp8 jt"],
    [7_500_000, "Rp7,5 jt"],
    [6_484_999, "Rp6,48 jt"],
    [6_485_000, "Rp6,49 jt"],
    [1_200_000_000, "Rp1,2 M"],
    [12_345_000_000, "Rp12,35 M"],
  ])("%d → %s", (value, expected) => {
    expect(formatRupiah(value, { short: true })).toBe(expected);
  });

  it("satu spasi sebelum satuan", () => {
    expect(formatRupiah(190_000, { short: true })).toMatch(/\d rb$/);
    expect(formatRupiah(6_480_000, { short: true })).toMatch(/\d jt$/);
  });

  it("pembulatan yang melewati batas naik ke satuan berikutnya", () => {
    expect(formatRupiah(999_600, { short: true })).toBe("Rp1 jt");
    expect(formatRupiah(999_996_000, { short: true })).toBe("Rp1 M");
  });

  it("di bawah seribu ditampilkan utuh (tidak jadi 0 rb)", () => {
    expect(formatRupiah(400, { short: true })).toBe("Rp400");
    expect(formatRupiah(0, { short: true })).toBe("Rp0");
  });

  it("negatif", () => {
    expect(formatRupiah(-35_000, { short: true })).toBe(`${MINUS}Rp35 rb`);
  });
});

describe("formatRupiah — bare: axis & tabel (§6.2, §6.4)", () => {
  it("axis: singkat tanpa Rp", () => {
    expect([0, 2_500_000, 5_000_000, 7_500_000, 10_000_000].map((v) => formatRupiah(v, { short: true, bare: true }))).toEqual([
      "0",
      "2,5 jt",
      "5 jt",
      "7,5 jt",
      "10 jt",
    ]);
  });

  it("tabel: penuh tanpa Rp, keluar memakai −", () => {
    expect(formatRupiah(6_480_000, { bare: true })).toBe("6.480.000");
    expect(formatRupiah(-6_480_000, { bare: true })).toBe(`${MINUS}6.480.000`);
  });
});

describe("formatRupiah — mode sembunyi (§6.8)", () => {
  it("penuh → Rp•••, singkat/tabel → •••", () => {
    expect(formatRupiah(6_480_000, { hidden: true })).toBe(HIDDEN_FULL);
    expect(formatRupiah(6_480_000, { hidden: true, short: true })).toBe(HIDDEN_BARE);
    expect(formatRupiah(6_480_000, { hidden: true, bare: true })).toBe(HIDDEN_BARE);
  });
});

describe("formatRupiah — input tidak valid", () => {
  it("NaN/Infinity → em dash", () => {
    expect(formatRupiah(Number.NaN)).toBe("—");
    expect(formatRupiah(Number.POSITIVE_INFINITY, { short: true })).toBe("—");
  });
});

describe("formatPercent (§6.7)", () => {
  it.each([
    [28, "28%"],
    [4.1, "4,1%"],
    [4.14, "4,1%"],
    [4.15, "4,2%"],
    [100, "100%"],
    [0, "0%"],
  ])("%d → %s", (value, expected) => {
    expect(formatPercent(value)).toBe(expected);
  });

  it("P/L selalu bertanda (§6.6)", () => {
    expect(formatPercent(4.1, { signed: true })).toBe("+4,1%");
    expect(formatPercent(-2.3, { signed: true })).toBe(`${MINUS}2,3%`);
    expect(formatPercent(0, { signed: true })).toBe("0%");
  });

  it("tanpa spasi", () => {
    expect(formatPercent(28)).not.toContain(" ");
  });
});

describe("formatPp", () => {
  it("drift alokasi", () => {
    expect(formatPp(2.1)).toBe("+2,1 pp");
    expect(formatPp(-1.4)).toBe(`${MINUS}1,4 pp`);
    expect(formatPp(0)).toBe("0 pp");
  });
});
