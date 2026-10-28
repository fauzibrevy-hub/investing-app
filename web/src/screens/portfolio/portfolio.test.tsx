import { fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { HideAmountsProvider } from "../../lib/privacy";
import { json, mockApi } from "../../test/mockApi";
import { PortfolioScreen } from "./PortfolioScreen";

const pos = (ticker: string, valuation: number, cost: number, pl_pct: number, w: number, target: number) => ({
  no: 1,
  category: "CORE",
  ticker,
  full_code: `IDX:${ticker}`,
  company_name: `${ticker} Tbk`,
  shares: 100,
  lots: 1,
  avg_price: 4150,
  current_price: 4300,
  cost,
  valuation,
  pl: valuation - cost,
  pl_pct,
  fund_alloc_pct: 0,
  val_alloc_pct: w,
  target_alloc_pct: target,
  drift_pct: w - target,
});
const data = {
  summary: { current_valuation: 1_100_000, total_cost: 900_000, total_pl: 100_000, total_pl_pct: 11.11, cash_balance: 100_000 },
  positions: [pos("SOCI", 400_000, 300_000, 33.3, 40, 15), pos("BUMI", 600_000, 700_000, -14.3, 60, 30)],
  available: true,
};

const renderAt = (path: string) =>
  render(
    <HideAmountsProvider initial={false}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/portfolio/*" element={<PortfolioScreen />} />
        </Routes>
      </MemoryRouter>
    </HideAmountsProvider>,
  );

beforeEach(() => mockApi({ "/api/finance/stock-portfolio": data }));
afterEach(() => vi.unstubAllGlobals());

describe("Portofolio (mobile)", () => {
  it("hero = current_valuation (termasuk kas), kartu Nilai pasar = Σ valuation, posisi urut nilai", async () => {
    renderAt("/portfolio");
    expect(await screen.findByText("Rp1.100.000")).toBeInTheDocument();
    const card = screen.getByLabelText("Ringkasan portofolio");
    expect(within(card).getByText("Rp900 rb")).toBeInTheDocument(); // modal
    expect(within(card).getByText("Rp1 jt")).toBeInTheDocument(); // Σ valuation
    expect(within(card).getByText("Rp100 rb")).toBeInTheDocument(); // kas
    const rows = screen.getAllByRole("button").filter((b) => /lot · avg/.test(b.textContent ?? ""));
    expect(rows.map((r) => r.textContent?.slice(0, 4))).toEqual(["BUMI", "SOCI"]);
    expect(screen.getByText("+30 pp")).toBeInTheDocument(); // drift BUMI
    expect(screen.getByText("−14,3%")).toHaveClass("text-status-over");
  });

  it("detail ticker: angka lengkap + tautan dossier dan matriks IDX", async () => {
    renderAt("/portfolio");
    fireEvent.click(await screen.findByRole("button", { name: /^BUMI\s*1 lot/ }));
    expect(await screen.findByRole("link", { name: "Dossier tesis BUMI" })).toHaveAttribute("href", "#/thesis/BUMI");
    expect(screen.getByRole("link", { name: "Riset BUMI" })).toHaveAttribute("href", "#/riset/BUMI");
    expect(screen.getByRole("link", { name: "Matriks XBRL BUMI" })).toHaveAttribute("href", "#/idx/BUMI");
    expect(screen.getByText("Rp700.000")).toBeInTheDocument();
  });

  it("tab PORTOFOLIO kosong → empty state, gagal → Coba lagi", async () => {
    mockApi({ "/api/finance/stock-portfolio": { summary: {}, positions: [], available: false } });
    const first = renderAt("/portfolio");
    expect(await screen.findByText("Belum ada posisi di tab PORTOFOLIO")).toBeInTheDocument();
    first.unmount();
    mockApi({ "/api/finance/stock-portfolio": () => json({ detail: "x" }, 502) });
    renderAt("/portfolio");
    expect((await screen.findAllByRole("button", { name: "Coba lagi" })).length).toBeGreaterThan(0);
  });
});
