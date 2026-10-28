import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import golden from "../../../../../tests/fixtures/research/canon/prices-COAL-1y.json";
import type { PricesDriver, PricesResponse } from "../types";
import { DriverChart, EMPTY_PRICES, formatAxis, plottedDrivers } from "./DriverChart";

const prices = golden as unknown as PricesResponse;

/** Golden tanpa seri driver dan indeks; driver bulanan dan indeks sintetis ditambahkan di sini (tidak ada golden). */
const NEWCASTLE: PricesDriver = {
  ...prices.drivers[0],
  status: "tersedia",
  last: "2026-10-01",
  series: [
    ["2026-08-01", 118.2],
    ["2026-09-01", 121.4],
    ["2026-10-01", 125.9],
  ],
};

const withDriver: PricesResponse = {
  ...prices,
  drivers: [NEWCASTLE, ...prices.drivers.slice(1)],
  drivers_plotted: 1,
  index_relative: prices.price.map(([d], i) => [d, 100 + i * 0.05] as [string, number]),
  events: [
    { date: "2026-04-28", kind: "dividend", amount: 45 },
    { date: "2026-06-01", kind: "split", ratio_num: 2, ratio_den: 1 },
  ],
};

describe("DriverChart, full mode on the golden prices", () => {
  it("is an image whose label summarises the latest price, the driver and the level count", () => {
    render(<DriverChart prices={withDriver} status="ready" />);
    const img = screen.getByRole("img");
    expect(img).toHaveAttribute("aria-label", expect.stringContaining("1.290"));
    expect(img).toHaveAttribute("aria-label", expect.stringContaining("Batubara Newcastle 125,9 (USD/T)"));
    expect(img).toHaveAttribute("aria-label", expect.stringContaining("2 level"));
  });

  it("says there is no driver series when none can be drawn (the golden as is)", () => {
    render(<DriverChart prices={prices} status="ready" />);
    expect(screen.getByRole("img")).toHaveAttribute("aria-label", expect.stringContaining("tanpa seri pendorong"));
  });

  it("has a screen-reader table with one row per plotted close", () => {
    const { container } = render(<DriverChart prices={withDriver} status="ready" />);
    const table = container.querySelector("table.sr-only-table") as HTMLElement;
    expect(within(table).getAllByRole("row")).toHaveLength(prices.price.length + 1);
    const last = within(table).getAllByRole("row").at(-1)!;
    expect(last).toHaveTextContent("27 Okt 2026");
    expect(last).toHaveTextContent("1.290");
    expect(last).toHaveTextContent("125,9");
  });

  it("puts the legend in a list below the chart and never labels levels on the plot", () => {
    const { container } = render(<DriverChart prices={withDriver} status="ready" />);
    const legend = screen.getByRole("list", { name: "Legenda grafik" });
    const svg = container.querySelector("svg[role=img]")!;
    expect(svg.compareDocumentPosition(legend) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(within(legend).getByText(/Level 1\.300 · Harga incaran · 24 Okt 2026/)).toBeInTheDocument();
    expect(within(legend).getByText(/Level 1\.100 · Level lama/)).toBeInTheDocument();
    expect(svg.textContent).not.toMatch(/Harga incaran|Level/);
    // Dua level → dua garis putus-putus.
    const lines = container.querySelectorAll("line[data-level]");
    expect(lines).toHaveLength(2);
    lines.forEach((l) => expect(l).toHaveAttribute("stroke-dasharray"));
  });

  it("draws the series in the design colours and widths", () => {
    const { container } = render(<DriverChart prices={withDriver} status="ready" />);
    const path = (s: string) => container.querySelector(`path[data-series=${s}]`)!;
    expect(path("price")).toHaveAttribute("stroke", "var(--accent)");
    expect(path("price")).toHaveAttribute("stroke-width", "1.5");
    expect(path("ma200")).toHaveAttribute("stroke", "var(--text-3)");
    expect(path("ma200")).toHaveAttribute("stroke-width", "1");
    expect(path("driver")).toHaveAttribute("stroke", "var(--cat-4)");
    expect(container.querySelector("[data-axis=right]")).not.toBeNull();
    container.querySelectorAll("line[data-grid]").forEach((l) => expect(l).toHaveAttribute("stroke", "var(--grid)"));
  });

  it("marks dividends as ticks and splits as × on the axis and lists them in the legend", () => {
    const { container } = render(<DriverChart prices={withDriver} status="ready" />);
    expect(container.querySelectorAll("[data-marker=dividend]")).toHaveLength(1);
    expect(container.querySelector("[data-marker=split]")).toHaveTextContent("×");
    const legend = screen.getByRole("list", { name: "Legenda grafik" });
    expect(within(legend).getByText("Dividen 45 · 28 Apr 2026")).toBeInTheDocument();
    expect(within(legend).getByText("Split 2:1 · 1 Jun 2026")).toBeInTheDocument();
  });

  it("hides the index relative until the toggle is pressed", () => {
    const { container } = render(<DriverChart prices={withDriver} status="ready" />);
    expect(container.querySelector("path[data-series=index]")).toBeNull();
    const toggle = screen.getByRole("button", { name: /Indeks relatif \^JKSE/ });
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    expect(container.querySelector("path[data-series=index]")).toHaveAttribute("stroke", "var(--cat-8)");
    expect(screen.getByRole("columnheader", { name: "Indeks relatif" })).toBeInTheDocument();
  });

  it("offers no index toggle when the payload has no index series (the golden)", () => {
    render(<DriverChart prices={prices} status="ready" />);
    expect(screen.queryByRole("button", { name: /Indeks relatif/ })).toBeNull();
  });

  it("moves the tooltip point with the keyboard", () => {
    render(<DriverChart prices={withDriver} status="ready" />);
    const slider = screen.getByRole("slider");
    expect(slider).toHaveAttribute("aria-valuetext", expect.stringContaining("27 Okt 2026"));
    fireEvent.keyDown(slider, { key: "Home" });
    expect(slider).toHaveAttribute("aria-valuetext", expect.stringContaining("29 Okt 2025, harga 1.020"));
    fireEvent.keyDown(slider, { key: "ArrowRight" });
    expect(slider).toHaveAttribute("aria-valuenow", "1");
  });

  it("describes the slider with a one-line hint, not with the whole series table", () => {
    render(<DriverChart prices={withDriver} status="ready" />);
    expect(screen.getByRole("slider")).toHaveAccessibleDescription("Panah kiri dan kanan memindah titik; tabel seri lengkap ada setelah legenda.");
  });

  it("a new series puts the point back on its latest close", () => {
    const { rerender } = render(<DriverChart prices={withDriver} status="ready" />);
    fireEvent.keyDown(screen.getByRole("slider"), { key: "Home" });
    expect(screen.getByRole("slider")).toHaveAttribute("aria-valuenow", "0");
    rerender(<DriverChart prices={{ ...withDriver, price: [...withDriver.price] }} status="ready" />);
    expect(screen.getByRole("slider")).toHaveAttribute("aria-valuenow", String(withDriver.price.length - 1));
  });
});

describe("DriverChart, compact mode", () => {
  it("draws price and the primary driver only", () => {
    const { container } = render(<DriverChart prices={withDriver} status="ready" compact />);
    expect(container.querySelector("path[data-series=price]")).not.toBeNull();
    expect(container.querySelector("path[data-series=driver]")).not.toBeNull();
    expect(container.querySelector("path[data-series=ma200]")).toBeNull();
    expect(container.querySelectorAll("line[data-level]")).toHaveLength(0);
    expect(screen.queryByRole("button", { name: /Indeks relatif/ })).toBeNull();
    expect(screen.getByRole("img")).not.toHaveAttribute("aria-label", expect.stringContaining("level"));
  });
});

describe("DriverChart states", () => {
  it("loading: grid and plot skeleton, busy", () => {
    const { container } = render(<DriverChart prices={undefined} status="loading" />);
    expect(container.querySelector("figure")).toHaveAttribute("aria-busy", "true");
    expect(container.querySelector("[data-skeleton]")).not.toBeNull();
    expect(container.querySelectorAll("line[data-grid]").length).toBeGreaterThan(0);
  });

  it("empty: too few closes", () => {
    render(<DriverChart prices={{ ...prices, price: [prices.price[0]] }} status="ready" />);
    expect(screen.getByText(EMPTY_PRICES)).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Legenda grafik" })).toBeNull();
  });

  it("empty: the payload says there are no prices", () => {
    render(<DriverChart prices={{ ...prices, empty: true, price: [] }} status="ready" />);
    expect(screen.getByText(EMPTY_PRICES)).toBeInTheDocument();
  });

  it("error: message and retry", () => {
    const retry = vi.fn();
    render(<DriverChart prices={undefined} status="error" onRetry={retry} />);
    expect(screen.getByRole("alert")).toHaveTextContent("Gagal memuat");
    fireEvent.click(screen.getByRole("button", { name: "Coba lagi" }));
    expect(retry).toHaveBeenCalledOnce();
  });
});

describe("plottedDrivers", () => {
  it("skips drivers without a series and uses a labelled proxy for a TBD driver", () => {
    const tbd: PricesDriver = {
      ...prices.drivers[1],
      status: "TBD",
      series: [],
      proxy: {
        id: "brent",
        label: "Brent",
        source: "Yahoo BZ=F",
        unit: "USD/bbl",
        cadence: "harian",
        note: null,
        status: "tersedia",
        last: "2026-10-01",
        series: [["2026-10-01", 80]],
        proxy: true,
        proxy_for: prices.drivers[1].id,
      },
    };
    const out = plottedDrivers([prices.drivers[0], tbd, NEWCASTLE]);
    expect(out.map((d) => d.id)).toEqual([tbd.id, NEWCASTLE.id]);
    expect(out[0]).toMatchObject({ label: tbd.label, proxyLabel: "Brent", unit: "USD/bbl" });
    expect(out[1].proxyLabel).toBeNull();
  });

  it("draws the chosen driver when several can be drawn", () => {
    const other: PricesDriver = { ...NEWCASTLE, id: "usd_idr", label: "USD/IDR", unit: "IDR", series: [["2026-10-01", 16300]] };
    const { container } = render(<DriverChart prices={{ ...withDriver, drivers: [NEWCASTLE, other] }} status="ready" driverId="usd_idr" />);
    expect(screen.getByRole("img")).toHaveAttribute("aria-label", expect.stringContaining("USD/IDR 16.300"));
    expect(container.querySelector("path[data-series=driver]")).not.toBeNull();
  });
});

describe("formatAxis (label sumbu besar dipadatkan, C5.4)", () => {
  it("keeps ordinary prices in full and compacts from 10 juta up", () => {
    expect(formatAxis(12500, 0)).toBe("12.500");
    expect(formatAxis(1.5, 1)).toBe("1,5");
    expect(formatAxis(12_000_000, 0)).toBe("12 jt");
    expect(formatAxis(2_500_000_000, 0)).toBe("2,5 M");
    expect(formatAxis(1.2e15, 0)).toBe("1.200 T");
    expect(formatAxis(-3e12, 0)).toBe("−3 T");
  });
});
