import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TrendChart } from "./TrendChart";
import {
  areaPath,
  clampTooltipLeft,
  defaultXTicks,
  gridValues,
  linePath,
  makeScales,
  nearestIndex,
  niceMax,
  tooltipTop,
  type TrendPoint,
} from "./trendGeometry";

const day = (d: number, value: number): TrendPoint => ({ x: d, value, label: `${d} Okt` });
// Kumulatif Oktober s.d. tanggal 24; September penuh 30 hari.
const OKT = Array.from({ length: 24 }, (_, i) => day(i + 1, (i + 1) * 270_000));
const SEP = Array.from({ length: 30 }, (_, i) => ({ x: i + 1, value: (i + 1) * 237_000, label: `${i + 1} Sep` }));

describe("skala & grid (§5.2)", () => {
  it("maks rapi: 4 interval 1/1,5/2/2,5/3/4/5/6/7,5 × 10^k", () => {
    expect(niceMax(21_830_000)).toBe(24_000_000);
    expect(niceMax(9_100_000)).toBe(10_000_000);
    expect(niceMax(7_110_000)).toBe(8_000_000);
    expect(niceMax(6_480_000)).toBe(8_000_000);
    expect(niceMax(4_000_000)).toBe(4_000_000);
    expect(niceMax(0)).toBe(1);
  });

  it("5 garis grid 0/25/50/75/100%", () => {
    expect(gridValues(10_000_000)).toEqual([0, 2_500_000, 5_000_000, 7_500_000, 10_000_000]);
  });

  it("kolom label Y 36 px; plot 130 px", () => {
    const s = makeScales({ width: 357, plotHeight: 130, xMin: 1, xMax: 31, yMax: 10 });
    expect(s.plotLeft).toBe(36);
    expect(s.x(1)).toBe(36);
    expect(s.x(31)).toBe(357);
    expect(s.y(0)).toBe(130);
    expect(s.y(10)).toBe(0);
    expect(s.y(5)).toBe(65);
  });

  it("path garis & area tertutup ke garis 0", () => {
    const s = makeScales({ width: 136, plotHeight: 100, xMin: 0, xMax: 1, yMax: 10 });
    const pts = [
      { x: 0, value: 0, label: "a" },
      { x: 1, value: 10, label: "b" },
    ];
    expect(linePath(pts, s)).toBe("M 36 100 L 136 0");
    expect(areaPath(pts, s)).toBe("M 36 100 L 136 0 L 136 100 L 36 100 Z");
  });

  it("snap ke tanggal terdekat", () => {
    const s = makeScales({ width: 357, plotHeight: 130, xMin: 1, xMax: 31, yMax: 10 });
    expect(nearestIndex(OKT, s, s.x(10.4))).toBe(9);
    expect(nearestIndex(OKT, s, s.x(10.6))).toBe(10);
    expect(nearestIndex(OKT, s, 9999)).toBe(23); // di kanan data → titik terakhir
  });

  it("tooltip: 12 px di atas titik, di-clamp/dibalik agar tetap di dalam plot", () => {
    expect(tooltipTop(100)).toBe(100 - 12 - 28);
    // tidak muat di atas → dibalik ke bawah titik, tidak menutupinya
    expect(tooltipTop(10)).toBe(22);
    expect(clampTooltipLeft(200, 140, 36, 357)).toBe(130);
    expect(clampTooltipLeft(40, 140, 36, 357)).toBe(36);
    expect(clampTooltipLeft(350, 140, 36, 357)).toBe(217);
  });

  it("tick X bulanan 1, 5, 10, …, 30", () => {
    expect(defaultXTicks(1, 31, OKT).map((t) => t.label)).toEqual(["1", "5", "10", "15", "20", "25", "30"]);
  });
});

describe("TrendChart", () => {
  const renderChart = (props = {}) =>
    render(<TrendChart current={OKT} previous={SEP} currentLabel="Oktober" previousLabel="September" xDomain={[1, 31]} size="mobile" {...props} />);

  it("dua seri dengan gaya sesuai spesifikasi", () => {
    const { container } = renderChart();
    const cur = container.querySelector('[data-series="current"]')!;
    expect(cur.getAttribute("stroke")).toBe("var(--accent)");
    expect(cur.getAttribute("stroke-width")).toBe("2.5");
    expect(cur.getAttribute("stroke-linejoin")).toBe("round");
    const prev = container.querySelector('[data-series="previous"]')!;
    expect(prev.getAttribute("stroke")).toBe("var(--series-compare)");
    expect(prev.getAttribute("stroke-width")).toBe("1.5");
    const area = container.querySelector('[data-series="area"]')!;
    // Area: fade vertikal accent (moni), lewat <linearGradient> SVG.
    expect(area.getAttribute("fill")).toMatch(/^url\(#.+\)$/);
    const stops = container.querySelectorAll("linearGradient stop");
    expect([...stops].map((s) => s.getAttribute("stop-opacity"))).toEqual(["0.16", "0"]);
    // seri utama berhenti di tanggal 24
    expect(cur.getAttribute("d")!.split("L")).toHaveLength(24);
  });

  it("grid: 5 garis, 0 solid border-strong, lainnya dashed 4 4", () => {
    const { container } = renderChart();
    const lines = [...container.querySelectorAll("[data-grid] line")];
    expect(lines).toHaveLength(5);
    expect(lines[0].getAttribute("stroke")).toBe("var(--border-strong)");
    expect(lines[0].getAttribute("stroke-dasharray")).toBeNull();
    lines.slice(1).forEach((l) => {
      expect(l.getAttribute("stroke")).toBe("var(--grid)");
      expect(l.getAttribute("stroke-dasharray")).toBe("4 4");
    });
    const labels = [...container.querySelectorAll("[data-grid] text")].map((t) => t.textContent);
    expect(labels).toEqual(["0", "2 jt", "4 jt", "6 jt", "8 jt"]);
  });

  it("default: titik aktif = data terakhir seri utama + tooltip tampil", () => {
    const { container } = renderChart();
    const tip = container.querySelector("[data-tooltip]")!;
    expect(tip.textContent).toContain("24 Okt");
    expect(tip.textContent).toContain("Rp6.480.000");
    expect(screen.getByRole("slider")).toHaveAttribute("aria-valuenow", "23");
  });

  it("keyboard: panah kiri/kanan, Home/End", () => {
    const onActive = vi.fn();
    renderChart({ onActiveChange: onActive });
    const slider = screen.getByRole("slider");
    fireEvent.keyDown(slider, { key: "ArrowLeft" });
    expect(slider).toHaveAttribute("aria-valuenow", "22");
    expect(slider).toHaveAttribute("aria-valuetext", "23 Okt, Rp6.210.000");
    fireEvent.keyDown(slider, { key: "Home" });
    expect(slider).toHaveAttribute("aria-valuenow", "0");
    fireEvent.keyDown(slider, { key: "ArrowLeft" });
    expect(slider).toHaveAttribute("aria-valuenow", "0");
    fireEvent.keyDown(slider, { key: "End" });
    expect(slider).toHaveAttribute("aria-valuenow", "23");
    fireEvent.keyDown(slider, { key: "ArrowRight" });
    expect(slider).toHaveAttribute("aria-valuenow", "23");
    expect(onActive).toHaveBeenCalledWith(22);
  });

  it("drag: snap ke tanggal terdekat, tetap setelah dilepas", () => {
    renderChart();
    const slider = screen.getByRole("slider");
    // Plot 321 px (357 − 36) untuk domain 1..31 → 10,7 px per hari.
    slider.getBoundingClientRect = () => ({ left: 0, top: 0, width: 321, height: 130, right: 321, bottom: 130, x: 0, y: 0, toJSON() {} });
    const xForDay = (d: number) => ((d - 1) / 30) * 321;
    fireEvent.pointerDown(slider, { clientX: xForDay(5) + 2, pointerId: 1, pointerType: "touch" });
    expect(slider).toHaveAttribute("aria-valuenow", "4");
    fireEvent.pointerMove(slider, { clientX: xForDay(12) - 3, pointerId: 1, pointerType: "touch" });
    expect(slider).toHaveAttribute("aria-valuenow", "11");
    fireEvent.pointerUp(slider, { pointerId: 1, pointerType: "touch" });
    fireEvent.pointerMove(slider, { clientX: xForDay(20), pointerId: 1, pointerType: "touch" });
    expect(slider).toHaveAttribute("aria-valuenow", "11");
  });

  it("legenda: Oktober (accent) + September (abu)", () => {
    const { container } = renderChart();
    expect(container.querySelector("[data-legend]")!.textContent).toBe("OktoberSeptember");
  });

  it("state loading / empty / error: grid tetap tampil", () => {
    const onRetry = vi.fn();
    const { container, rerender } = renderChart({ status: "loading" });
    expect(container.querySelectorAll("[data-grid] line")).toHaveLength(5);
    expect(container.querySelector("[data-skeleton]")).not.toBeNull();
    rerender(<TrendChart current={[]} currentLabel="Oktober" size="mobile" />);
    expect(screen.getByText("Belum cukup data")).toBeInTheDocument();
    expect(container.querySelectorAll("[data-grid] line")).toHaveLength(5);
    rerender(<TrendChart current={OKT} currentLabel="Oktober" size="mobile" status="error" onRetry={onRetry} />);
    expect(screen.getByText("Gagal memuat")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Coba lagi" }));
    expect(onRetry).toHaveBeenCalled();
  });
});
