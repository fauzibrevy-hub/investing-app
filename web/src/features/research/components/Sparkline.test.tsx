import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Sparkline, sparklineGeometry } from "./Sparkline";

const TWELVE = [4, 6, 5, 8, 9, 7, 10, 12, 11, 13, 12, 14];

describe("sparklineGeometry", () => {
  it("draws high values near the top and ends on the last point", () => {
    const geo = sparklineGeometry([1, 2, 3], 96, 24);
    expect(geo.path.startsWith("M ")).toBe(true);
    expect(geo.last).toEqual({ x: 94, y: 2 });
    expect(geo.zeroY).toBeNull();
  });

  it("puts negative values below a hairline zero line", () => {
    const geo = sparklineGeometry([-4, -1, 3, 5], 96, 24);
    expect(geo.zeroY).not.toBeNull();
    const first = geo.path.split(" L ")[0].split(" ");
    const firstY = Number(first[first.length - 1]);
    expect(firstY).toBeGreaterThan(geo.zeroY!);
    expect(geo.last!.y).toBeLessThan(geo.zeroY!);
  });

  it("breaks the line at a gap instead of bridging it", () => {
    const geo = sparklineGeometry([1, 2, null, 4, 5], 96, 24);
    expect(geo.path.match(/M /g)).toHaveLength(2);
  });

  it("centres a flat series and handles a single value", () => {
    expect(sparklineGeometry([5, 5, 5], 96, 24).last!.y).toBe(12);
    expect(sparklineGeometry([5], 96, 24).last).not.toBeNull();
    expect(sparklineGeometry([null, null], 96, 24)).toEqual({ path: "", last: null, zeroY: null });
  });
});

describe("Sparkline", () => {
  it("default: a 96 × 24 img with a 1.5 px text-2 line, a 4 px accent last point and no fill", () => {
    const { container } = render(<Sparkline values={TWELVE} label="Laba bersih, 12 kuartal" />);
    const svg = screen.getByRole("img", { name: "Laba bersih, 12 kuartal" });
    expect(svg).toHaveAttribute("width", "96");
    expect(svg).toHaveAttribute("height", "24");
    const path = container.querySelector("path")!;
    expect(path).toHaveAttribute("stroke-width", "1.5");
    expect(path).toHaveAttribute("fill", "none");
    expect(path).toHaveClass("stroke-text-2");
    const dot = container.querySelector("[data-last-point]")!;
    expect(dot).toHaveAttribute("r", "2");
    expect(dot).toHaveClass("fill-accent");
    expect(container.querySelector("[data-zero-line]")).toBeNull();
  });

  it("draws the zero line only when a value is negative", () => {
    const { container } = render(<Sparkline values={[-2, 1, 3, -1]} label="Arus kas" />);
    expect(container.querySelector("[data-zero-line]")).toHaveClass("stroke-border-hairline");
  });

  it("loading: a skeleton of the same size, no img", () => {
    const { container } = render(<Sparkline values={[]} label="Laba" status="loading" />);
    expect(container.querySelector("[data-skeleton]")).toHaveStyle({ width: "96px", height: "24px" });
    expect(screen.queryByRole("img")).toBeNull();
  });

  it("empty: the caption Data belum cukup (never n/a), also when fewer than two values exist", () => {
    const { rerender } = render(<Sparkline values={[]} label="Laba" status="empty" />);
    expect(screen.getByText("Data belum cukup")).toBeInTheDocument();
    expect(screen.queryByText("n/a")).toBeNull();
    rerender(<Sparkline values={[3, null, null]} label="Laba" />);
    expect(screen.getByText("Data belum cukup")).toBeInTheDocument();
    expect(screen.queryByRole("img")).toBeNull();
  });

  it("error: a named icon with the reason as a tooltip", () => {
    render(<Sparkline values={TWELVE} label="Laba" status="error" errorMessage="gagal" />);
    expect(screen.getByRole("img", { name: "Laba: gagal memuat" })).toHaveAttribute("title", "gagal");
  });
});
