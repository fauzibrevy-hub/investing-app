import { describe, expect, it } from "vitest";
import { formatCompactMoney } from "./money";

describe("formatCompactMoney", () => {
  it("prefixes the report currency and never guesses Rp when the currency is unknown", () => {
    expect(formatCompactMoney(1.23e12, "IDR")).toBe("Rp1,23 T");
    expect(formatCompactMoney(95.2e6, "USD")).toBe("US$95,2 jt");
    expect(formatCompactMoney(95.2e6, null)).toBe("95,2 jt");
  });
});
