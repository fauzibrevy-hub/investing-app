import { describe, expect, it } from "vitest";
import { relativeChange, signChangePhrase, yoyCellText, yoySentence } from "./change";

describe("relativeChange: one rule for every YoY", () => {
  it("both positive", () => {
    expect(relativeChange(150, 100)).toEqual({ kind: "pct", value: 50 });
  });

  it("both negative and worse is a fall, not a rise", () => {
    const c = relativeChange(-124.789, -27.163);
    expect(c?.kind).toBe("pct");
    expect(c && c.kind === "pct" ? c.value : 0).toBeCloseTo(-359.4, 1);
  });

  it("both negative and better is positive", () => {
    expect(relativeChange(-60, -100)).toEqual({ kind: "pct", value: 40 });
  });

  it("crossing zero has no percentage", () => {
    expect(relativeChange(5, -3)).toEqual({ kind: "sign_change", from: -3, to: 5 });
    expect(relativeChange(-1, 8)).toEqual({ kind: "sign_change", from: 8, to: -1 });
  });

  it("a zero base or non-finite input has none, and landing on zero is −100 % or +100 %", () => {
    expect(relativeChange(3, 0)).toBeNull();
    expect(relativeChange(Number.NaN, 3)).toBeNull();
    expect(relativeChange(0, 50)).toEqual({ kind: "pct", value: -100 });
    expect(relativeChange(0, -50)).toEqual({ kind: "pct", value: 100 });
  });
});

describe("a multiple's YoY is a difference in times (N2)", () => {
  it("reads +3,1 in the cell and 'naik 3,1× YoY' in the sentence, never a percent", () => {
    const yoy = { kind: "x" as const, value: 3.1 };
    expect(yoyCellText(yoy)).toBe("+3,1");
    expect(yoySentence("Utang bersih / EBITDA", yoy)).toBe("Utang bersih / EBITDA naik 3,1× YoY");
    expect(yoySentence("Belanja modal / penyusutan", { kind: "x", value: -0.4 })).toBe("Belanja modal / penyusutan turun 0,4× YoY");
  });
});

describe("sign change wording", () => {
  it("writes profit lines as rugi/laba and others as negatif/positif", () => {
    expect(signChangePhrase(-1, 2, true)).toBe("dari rugi ke laba");
    expect(signChangePhrase(2, -1, true)).toBe("dari laba ke rugi");
    expect(signChangePhrase(-1, 2, false)).toBe("dari negatif ke positif");
    expect(signChangePhrase(2, -1, false)).toBe("dari positif ke negatif");
    expect(yoyCellText({ kind: "sign_change", from: -1, to: 2 }, true)).toBe("rugi → laba");
    expect(yoyCellText({ kind: "pct", value: -359.44 })).toBe("−359,4");
    expect(yoySentence("Laba bersih", { kind: "sign_change", from: 4, to: -2 }, true)).toBe("Laba bersih berbalik dari laba ke rugi YoY");
  });
});
