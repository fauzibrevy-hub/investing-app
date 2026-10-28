import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { DeskResponse } from "../types";
import { DeskFooter, ScreenFooter } from "./DeskFooter";

const ages = (labels?: Record<string, string>): DeskResponse["ages"] => ({
  prices: { COAL: 1 },
  filings: { COAL: 60 },
  drivers: { bi_rate: 3, usd_idr: 0 },
  driver_labels: labels,
});

describe("DeskFooter names drivers by their catalogue label", () => {
  it("shows the label, never the driver id", () => {
    render(<DeskFooter ages={ages({ bi_rate: "BI rate", usd_idr: "USD/IDR" })} />);
    const text = screen.getByText(/Umur data/).textContent ?? "";
    expect(text).toContain("BI rate 3 hari");
    expect(text).toContain("USD/IDR");
    expect(text).not.toContain("bi_rate");
    expect(text).not.toContain("usd_idr");
  });

  it("falls back to the id only when the payload carries no label", () => {
    render(<DeskFooter ages={ages()} />);
    expect(screen.getByText(/Umur data/).textContent).toContain("bi_rate");
  });
});

describe("DeskFooter ages and disclaimer", () => {
  it("gives the newest and oldest age per source, or one age when they agree", () => {
    render(<DeskFooter ages={{ prices: { A: 1, B: 5, C: null }, filings: { A: 60 }, drivers: {} }} />);
    const text = screen.getByText(/Umur data/).textContent ?? "";
    expect(text).toMatch(/Harga: terbaru 1 hari, terlama 5 hari/);
    expect(text).toMatch(/Laporan: 2 bulan/);
    expect(text).not.toContain("Pendorong");
  });

  it("a source with no known age says belum ada", () => {
    render(<DeskFooter ages={{ prices: { A: null }, filings: {}, drivers: {} }} />);
    expect(screen.getByText(/Umur data/).textContent).toMatch(/Harga: belum ada · Laporan: belum ada/);
  });

  it("without ages only the disclaimer shows", () => {
    render(<DeskFooter />);
    expect(screen.queryByText(/Umur data/)).toBeNull();
    expect(screen.getByRole("contentinfo")).toHaveTextContent(/^Bukan saran investasi$/);
  });
});

describe("ScreenFooter", () => {
  it("dates the data, then the disclaimer", () => {
    render(<ScreenFooter asOf="2026-10-20T09:21:52Z" />);
    expect(screen.getByRole("contentinfo")).toHaveTextContent("Data per 20 Okt 2026");
    expect(screen.getByText("Bukan saran investasi")).toBeInTheDocument();
  });

  it("without a date only the disclaimer shows", () => {
    render(<ScreenFooter />);
    expect(screen.getByRole("contentinfo")).toHaveTextContent(/^Bukan saran investasi$/);
  });
});
