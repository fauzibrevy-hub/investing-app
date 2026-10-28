import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { PercentileBand, bandPositions } from "./PercentileBand";

describe("bandPositions", () => {
  it("percentile axis: the whole track is the range and the median is at 50", () => {
    expect(bandPositions(41)).toEqual({ fillStart: 0, fillEnd: 100, median: 50, marker: 41 });
    expect(bandPositions(140).marker).toBe(100);
    expect(bandPositions(-3).marker).toBe(0);
  });

  it("value axis: min to max fills, the median and the current value are placed on it", () => {
    expect(bandPositions(50, { value: 15, min: 10, max: 20, median: 12 })).toEqual({ fillStart: 0, fillEnd: 100, median: 20, marker: 50 });
  });

  it("value axis widens to hold a current value outside the historical range", () => {
    const pos = bandPositions(100, { value: 30, min: 10, max: 20, median: 15 });
    expect(pos.marker).toBe(100);
    expect(pos.fillEnd).toBe(50);
  });
});

describe("PercentileBand", () => {
  it("default: a 120 px meter with a track, a min–max fill, a 1 px median tick, an 8 px accent marker and the caption", () => {
    const { container } = render(<PercentileBand percentile={41} caption="p41 · 10 th" label="Persentil P/E" />);
    const meter = screen.getByRole("meter", { name: "Persentil P/E" });
    expect(meter).toHaveStyle({ width: "120px" });
    expect(meter).toHaveAttribute("aria-valuenow", "41");
    expect(meter).toHaveAttribute("aria-valuetext", "p41 · 10 th");
    expect(container.querySelector("[data-band-fill]")).toHaveClass("bg-surface-sunken");
    expect(container.querySelector("[data-band-median]")).toHaveClass("w-px", "bg-text-3");
    const marker = container.querySelector("[data-band-marker]")!;
    expect(marker).toHaveClass("w-2", "h-2", "bg-accent");
    expect(marker).toHaveStyle({ left: "41%" });
    expect(screen.getByText("p41 · 10 th")).toBeInTheDocument();
  });

  it("builds a caption when the backend gives none", () => {
    render(<PercentileBand percentile={72.4} label="Persentil" />);
    expect(screen.getByText("p72")).toBeInTheDocument();
  });

  it("loading: a 120 × 12 skeleton", () => {
    const { container } = render(<PercentileBand percentile={null} label="Persentil" status="loading" />);
    expect(container.querySelector("[data-skeleton]")).toHaveStyle({ width: "120px", height: "12px" });
    expect(screen.queryByRole("meter")).toBeNull();
  });

  it("empty: only the caption 'riwayat < 3 th', no marker", () => {
    const { container } = render(<PercentileBand percentile={null} label="Persentil P/E" />);
    expect(screen.getByText("riwayat < 3 th")).toBeInTheDocument();
    expect(screen.queryByRole("meter")).toBeNull();
    expect(container.querySelector("[data-band-marker]")).toBeNull();
  });

  it("empty keeps the backend caption when it supplies one", () => {
    render(<PercentileBand percentile={null} caption="riwayat < 3 th" label="Persentil" status="empty" />);
    expect(screen.getByText("riwayat < 3 th")).toBeInTheDocument();
  });

  it("error: a named icon with the reason", () => {
    render(<PercentileBand percentile={50} label="Persentil P/E" status="error" errorMessage="timeout" />);
    expect(screen.getByRole("img", { name: "Persentil P/E: gagal memuat" })).toHaveAttribute("title", "timeout");
  });

  it("never colours the position by judgement", () => {
    const { container } = render(<PercentileBand percentile={3} label="Persentil" />);
    expect(container.innerHTML).not.toMatch(/status-(safe|over|warn)/);
  });
});
