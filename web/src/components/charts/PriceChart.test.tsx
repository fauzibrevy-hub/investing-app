import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { PriceChart } from "./PriceChart";
import {
  BOX,
  buildScale,
  decollideLabels,
  polyline,
  type Point,
} from "./priceGeometry";

describe("priceGeometry label de-collision (M4-3)", () => {
  const GAP = 22; // PILL_H (18) + 4
  const LO = 19;  // top (10) + PILL_H/2 (9)
  const HI = 197; // height (232) - bottom (26) - PILL_H/2 (9)

  it("empty input returns empty array", () => {
    expect(decollideLabels([], GAP, LO, HI)).toEqual([]);
  });

  it("single label is clamped within [LO, HI]", () => {
    expect(decollideLabels([100], GAP, LO, HI)).toEqual([100]);
    expect(decollideLabels([5], GAP, LO, HI)).toEqual([LO]);
    expect(decollideLabels([250], GAP, LO, HI)).toEqual([HI]);
  });

  it("already separated labels preserve positions and order", () => {
    const ys = [40, 90, 150];
    const res = decollideLabels(ys, GAP, LO, HI);
    expect(res).toEqual([40, 90, 150]);
  });

  it("overlapping identical y values are separated by >= gap and clamped inside plot", () => {
    // Dua label dengan y sama persis (misal harga modal == harga incaran)
    const ys = [100, 100];
    const res = decollideLabels(ys, GAP, LO, HI);
    expect(res[1]! - res[0]!).toBeGreaterThanOrEqual(GAP);
    expect(res[0]).toBeGreaterThanOrEqual(LO);
    expect(res[1]).toBeLessThanOrEqual(HI);
  });

  it("overlapping labels near top boundary are pushed down and clamped >= LO", () => {
    const ys = [LO, LO];
    const res = decollideLabels(ys, GAP, LO, HI);
    expect(res[0]).toBeGreaterThanOrEqual(LO);
    expect(res[1]! - res[0]!).toBeGreaterThanOrEqual(GAP);
    expect(res[1]).toBeLessThanOrEqual(HI);
  });

  it("overlapping labels near bottom boundary are pushed up and clamped <= HI", () => {
    const ys = [HI, HI];
    const res = decollideLabels(ys, GAP, LO, HI);
    expect(res[0]).toBeGreaterThanOrEqual(LO);
    expect(res[1]! - res[0]!).toBeGreaterThanOrEqual(GAP);
    expect(res[1]).toBeLessThanOrEqual(HI);
  });

  it("three overlapping values separated by >= gap without shrinking below label height", () => {
    const ys = [120, 120, 120];
    const res = decollideLabels(ys, GAP, LO, HI);
    expect(res).toHaveLength(3);
    expect(res[1]! - res[0]!).toBeGreaterThanOrEqual(GAP);
    expect(res[2]! - res[1]!).toBeGreaterThanOrEqual(GAP);
    expect(res[0]).toBeGreaterThanOrEqual(LO);
    expect(res[2]).toBeLessThanOrEqual(HI);
  });

  it("degenerate case (3 labels, tiny plot): drops lowest-priority label rather than spacing below gap", () => {
    const ys = [10, 10, 10];
    const tinyLo = 0;
    const tinyHi = 25; // only 2 labels can fit in 25px with GAP 22
    const res = decollideLabels(ys, GAP, tinyLo, tinyHi);
    expect(res).toHaveLength(3);
    expect(res[0]).not.toBeNull();
    expect(res[1]).not.toBeNull();
    expect(res[2]).toBeNull(); // 3rd label dropped to prevent spacing below label height
    expect(res[1]! - res[0]!).toBeGreaterThanOrEqual(GAP);
    expect(res[0]!).toBeGreaterThanOrEqual(tinyLo);
    expect(res[1]!).toBeLessThanOrEqual(tinyHi);
  });

  it("preserves original array index order even when input is descending", () => {
    const ys = [150, 40];
    const res = decollideLabels(ys, GAP, LO, HI);
    expect(res[0]).toBe(150);
    expect(res[1]).toBe(40);
  });
});

describe("priceGeometry buildScale and polyline", () => {
  const sampleCloses: Point[] = [
    ["2026-01-01", 1000],
    ["2026-01-02", 1200],
    ["2026-01-03", 1100],
  ];

  it("buildScale returns null for empty closes", () => {
    expect(buildScale({ closes: [], ma200: [], band: null, refs: [] })).toBeNull();
  });

  it("buildScale computes valid min, max, and x/y mapping", () => {
    const scale = buildScale({
      closes: sampleCloses,
      ma200: [],
      band: { low: 900, high: 1300 },
      refs: [{ id: "modal", value: 1050 }],
    });
    expect(scale).not.toBeNull();
    if (!scale) return;
    expect(scale.min).toBeLessThanOrEqual(900);
    expect(scale.max).toBeGreaterThanOrEqual(1300);
    expect(scale.x(0, 3)).toBe(BOX.left);
    expect(scale.x(2, 3)).toBe(BOX.width - BOX.right);
    expect(scale.y(scale.min)).toBe(BOX.height - BOX.bottom);
    expect(scale.y(scale.max)).toBe(BOX.top);
  });

  it("polyline creates valid SVG points string", () => {
    const scale = buildScale({ closes: sampleCloses, ma200: [], band: null, refs: [] })!;
    const dates = sampleCloses.map((p) => p[0]);
    const pts = polyline(sampleCloses, dates, scale);
    expect(pts.split(" ")).toHaveLength(3);
  });
});

describe("PriceChart Component", () => {
  const closes: Point[] = [
    ["2026-01-01", 2000],
    ["2026-01-02", 2100],
    ["2026-01-03", 2200],
  ];
  const ma200: Point[] = [
    ["2026-01-01", 1900],
    ["2026-01-02", 1950],
    ["2026-01-03", 2000],
  ];
  const band = { low: 1800, high: 2400 };

  it("renders SVG with role=img and proper aria-label", () => {
    render(<PriceChart ticker="BBCA" closes={closes} ma200={ma200} band={band} />);
    const svg = screen.getByRole("img", { name: /Harga BBCA: close 2\.200/ });
    expect(svg).toBeInTheDocument();
  });

  it("renders flat fair-value band with accent-soft token and no gradient", () => {
    const { container } = render(
      <PriceChart ticker="BBCA" closes={closes} ma200={ma200} band={band} />
    );
    // Flat fill using accent-soft token
    const rects = container.querySelectorAll("rect");
    const bandRect = Array.from(rects).find((r) =>
      r.getAttribute("fill")?.includes("var(--accent-soft)") ||
      r.getAttribute("fill")?.toLowerCase() === "#e3ecf8"
    );
    expect(bandRect).toBeDefined();

    // No linearGradient or radialGradient (anti-AI slop rule)
    expect(container.querySelectorAll("linearGradient, radialGradient")).toHaveLength(0);
  });

  it("renders modal reference line and label ONLY when held (defaults to false / fail-closed)", () => {
    // When isHeld is omitted, it defaults to false (fail closed)
    const { rerender, queryByText } = render(
      <PriceChart
        ticker="BBCA"
        closes={closes}
        ma200={ma200}
        band={band}
        avgPrice={2050}
      />
    );
    expect(queryByText(/Modal/)).not.toBeInTheDocument();

    // When explicitly false
    rerender(
      <PriceChart
        ticker="BBCA"
        closes={closes}
        ma200={ma200}
        band={band}
        avgPrice={2050}
        isHeld={false}
      />
    );
    expect(queryByText(/Modal/)).not.toBeInTheDocument();

    // When explicitly true
    rerender(
      <PriceChart
        ticker="BBCA"
        closes={closes}
        ma200={ma200}
        band={band}
        avgPrice={2050}
        isHeld={true}
      />
    );
    expect(screen.getByText("Modal 2.050")).toBeInTheDocument();
  });

  it("renders harga incaran reference line and label when entryPrice provided", () => {
    render(
      <PriceChart
        ticker="BBCA"
        closes={closes}
        ma200={ma200}
        band={band}
        entryPrice={2150}
      />
    );
    expect(screen.getByText("Incaran 2.150")).toBeInTheDocument();
  });

  it("de-collides reference-line labels when modal and incaran have identical price", () => {
    render(
      <PriceChart
        ticker="BBCA"
        closes={closes}
        ma200={ma200}
        band={band}
        avgPrice={2100}
        entryPrice={2100}
        isHeld={true}
      />
    );

    const modalText = screen.getByText("Modal 2.100");
    const incaranText = screen.getByText("Incaran 2.100");

    const modalY = Number(modalText.getAttribute("y"));
    const incaranY = Number(incaranText.getAttribute("y"));

    // Overlapping y values must be separated by >= label height
    expect(Math.abs(incaranY - modalY)).toBeGreaterThanOrEqual(18);
  });
});
