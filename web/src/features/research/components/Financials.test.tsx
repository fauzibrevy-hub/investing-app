import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { json, mockApi } from "../../../test/mockApi";
import { FactDrawerProvider } from "./FactDrawer";
import { golden } from "../testing/canon";
import type { DossierResponse, FilingsResponse, FinancialsResponse, ShareCount } from "../types";
import { badgeLegend } from "./ProvenanceBadge";
import { Financials, filingsForPeriods, groupFinancials, shareRuns, shareScale, shareTarget, withoutAnnualDerived } from "./Financials";

const COAL = golden<DossierResponse>("ticker-COAL");
const pending = () => new Promise<Response>(() => {});

function show() {
  return render(
    <MemoryRouter>
      <FactDrawerProvider>
        <Financials ticker="COAL" dossier={COAL} />
      </FactDrawerProvider>
    </MemoryRouter>,
  );
}

function backend() {
  const calls: string[] = [];
  mockApi({
    "/api/research/COAL/financials": (u: URL) => {
      calls.push(u.search);
      const view = u.searchParams.get("view");
      return json(golden<FinancialsResponse>(`financials-COAL-${view}`));
    },
    "/api/research/filings": golden<FilingsResponse>("filings-COAL"),
  });
  return calls;
}

describe("Keuangan (design.md §4.2)", () => {
  it("ready: grouped QuarterTables with scale titles, FactChip cells, shares with source, filings with audit flag", async () => {
    backend();
    show();
    const income = await screen.findByRole("table", { name: "Laba rugi, kuartal" });
    expect(screen.getByRole("heading", { name: /^Laba rugi · dalam / })).toBeInTheDocument();
    expect(screen.getByRole("table", { name: "Arus kas, kuartal" })).toBeInTheDocument();
    expect(screen.getByRole("table", { name: "Neraca, kuartal" })).toBeInTheDocument();
    expect(within(income).getAllByRole("button", { name: /^Asal angka Pendapatan / }).length).toBeGreaterThan(0);
    // Q4 columns carry the derived badge (§3.2).
    expect(within(income).getAllByText("turunan").length).toBeGreaterThan(0);

    // Saham beredar goes through QuarterTable (same sticky-fit as the other tables); the bespoke table is gone.
    expect(screen.queryByRole("table", { name: "Saham beredar per periode dan sumbernya" })).toBeNull();
    const shares = screen.getByRole("table", { name: "Saham beredar, kuartal" });
    expect(within(shares).getByRole("rowheader", { name: "Saham beredar" })).toBeInTheDocument();
    expect(within(shares).getAllByRole("button", { name: /^Asal angka Saham beredar Q\d \d\d: \S+/ })).toHaveLength(12);
    // Manual provenance stays visible: one run for the 12 manual periods, with badge, date and note.
    const sources = screen.getByRole("list", { name: "Sumber saham beredar" });
    const run = within(sources).getByRole("listitem");
    expect(run).toHaveTextContent(/^Q3 23 – Q2 26/);
    expect(within(run).getByText("manual")).toBeInTheDocument();
    expect(within(run).getByText(/^Diisi pemilik per .+: data sintetis$/)).toBeInTheDocument();

    // Laporan: one-column list, no table, one row per filing.
    expect(screen.queryByRole("table", { name: "Laporan untuk periode yang tampil" })).toBeNull();
    const filings = await screen.findByRole("list", { name: "Laporan untuk periode yang tampil" });
    const items = within(filings).getAllByRole("listitem");
    expect(items.length).toBeGreaterThan(0);
    expect(items[0].firstElementChild).toHaveClass("text-body-strong");
    expect(within(items[0]).getByText(/^Tersedia /)).toBeInTheDocument();
    expect(within(filings).getAllByText(/^diaudit · /).length).toBeGreaterThan(0);
    expect(within(filings).getAllByText(/^belum diaudit · /).length).toBeGreaterThan(0);
    for (const li of items) expect(li).toHaveClass("min-h-11");
  });

  it("a manual share count moves focus to its source instead of opening the fact drawer", async () => {
    backend();
    show();
    const shares = await screen.findByRole("table", { name: "Saham beredar, kuartal" });
    const cell = within(shares).getAllByRole("button", { name: /^Asal angka Saham beredar Q2 26/ })[0];
    // No dialog opens, so none is announced; real figures elsewhere still announce theirs.
    expect(cell).not.toHaveAttribute("aria-haspopup");
    expect(within(screen.getByRole("table", { name: "Laba rugi, kuartal" })).getAllByRole("button", { name: /^Asal angka / })[0]).toHaveAttribute("aria-haspopup", "dialog");
    fireEvent.click(cell);
    expect(screen.queryByRole("dialog")).toBeNull();
    const run = within(screen.getByRole("list", { name: "Sumber saham beredar" })).getByRole("listitem");
    expect(run).toHaveFocus();
  });

  it("share counts use one scale for the table with the unit in the header, like the money tables", async () => {
    backend();
    show();
    const shares = await screen.findByRole("table", { name: "Saham beredar, kuartal" });
    // COAL golden: 500.000.000 lembar → (RIBU LEMBAR) 500.000.
    expect(within(shares).getAllByRole("columnheader")[0]).toHaveTextContent("(RIBU LEMBAR)");
    expect(screen.getByRole("heading", { name: "Saham beredar · dalam ribu" })).toBeInTheDocument();
    expect(within(shares).getAllByRole("button", { name: "Asal angka Saham beredar Q2 26: 500.000" })).toHaveLength(1);
  });

  it("a legend line under the tables explains the compact badges in view, and only those", async () => {
    backend();
    show();
    await screen.findByRole("table", { name: "Laba rugi, kuartal" });
    // Quarterly view: Q4 cells derived from FY.
    expect(screen.getByText("FY = turunan FY")).toHaveAttribute("data-badge-legend");
    fireEvent.click(screen.getByRole("radio", { name: "Tahunan" }));
    await screen.findByRole("table", { name: "Laba rugi, tahunan" });
    // Annual view: every column is a full year, so no FY badge and no legend for it.
    expect(within(screen.getByRole("table", { name: "Laba rugi, tahunan" })).queryAllByText("FY")).toHaveLength(0);
    expect(screen.queryByText(/FY = turunan FY/)).toBeNull();
  });

  it("the two segmented controls stack full width on a phone, no fixed minimum width", async () => {
    backend();
    show();
    await screen.findByRole("table", { name: "Laba rugi, kuartal" });
    for (const name of ["Tampilan laporan", "Jumlah periode"]) {
      const group = screen.getByRole("radiogroup", { name });
      expect(group).toHaveClass("w-full");
      expect(group.className).not.toMatch(/(^|\s)min-w-|(^|\s)w-auto/);
    }
    expect(screen.getByRole("radiogroup", { name: "Tampilan laporan" }).parentElement).toHaveClass("flex-col");
  });

  it("the view toggle and period count drive GET /financials?view=&periods=", async () => {
    const calls = backend();
    show();
    await screen.findByRole("table", { name: "Laba rugi, kuartal" });
    expect(screen.getByRole("radio", { name: "Kuartal" })).toHaveAttribute("aria-checked", "true");
    fireEvent.click(screen.getByRole("radio", { name: "TTM" }));
    expect(await screen.findByRole("table", { name: "Laba rugi, TTM" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "20" }));
    await waitFor(() => expect(calls).toContain("?view=ttm&periods=20"));
    expect(calls).toEqual(expect.arrayContaining(["?view=standalone&periods=12", "?view=ttm&periods=12"]));
    expect(screen.getByRole("radio", { name: "20" })).toHaveAttribute("aria-checked", "true");
  });

  it("the annual view shows the year tables and maps every period to its FY filing", async () => {
    const calls = backend();
    show();
    await screen.findByRole("table", { name: "Laba rugi, kuartal" });
    fireEvent.click(screen.getByRole("radio", { name: "Tahunan" }));
    expect(await screen.findByRole("table", { name: "Laba rugi, tahunan" })).toBeInTheDocument();
    expect(calls).toContain("?view=annual&periods=12");
    const annual = golden<FinancialsResponse>("financials-COAL-annual");
    const shown = filingsForPeriods(golden<FilingsResponse>("filings-COAL").items, annual);
    expect(shown.length).toBeGreaterThan(0);
    expect(shown.every((f) => f.period === "FY")).toBe(true);
  });

  it("loading, error and empty states", async () => {
    mockApi({ "/api/research/COAL/financials": pending, "/api/research/filings": pending });
    const first = show();
    expect(screen.getByText("Memuat Keuangan, kuartal…")).toBeInTheDocument();
    first.unmount();
    mockApi({ "/api/research/COAL/financials": () => json({ detail: "laporan rusak" }, 500) });
    const second = show();
    expect(await screen.findByText("laporan rusak")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Coba lagi" })).toBeInTheDocument();
    second.unmount();
    mockApi({ "/api/research/COAL/financials": { ...golden<FinancialsResponse>("financials-COAL-standalone"), periods: [], groups: [], shares: [] } });
    show();
    expect(await screen.findByText("Belum ada laporan")).toBeInTheDocument();
  });
});

describe("Keuangan data", () => {
  it("bank taxonomies regroup into Pendapatan, Pembiayaan dan pendanaan, Kualitas aset, Neraca, Arus kas", () => {
    // TBD golden: no bank financials golden exists yet; typed inline rows over the COAL golden.
    const base = golden<FinancialsResponse>("financials-COAL-standalone");
    const cells = base.groups[0].rows[0].cells;
    const bank: FinancialsResponse = {
      ...base,
      taxonomy: "sharia",
      groups: [
        { id: "laba_rugi", label: "Laba rugi", rows: [{ id: "revenue", label: "Pendapatan", element: "idx-cor:IncomeFromFundManagementAsMudharib", unit: "IDR", cells }] },
        { id: "arus_kas", label: "Arus kas", rows: [{ id: "operating_cash_flow", label: "Arus kas operasi", element: "idx-cor:NetCashFlowsReceivedFromUsedInOperatingActivities", unit: "IDR", cells }] },
        {
          id: "neraca",
          label: "Neraca",
          rows: [
            { id: "deposits", label: "Dana pihak ketiga", element: "idx-cor:TotalThirdPartyFunds", unit: "IDR", cells },
            { id: "allowance", label: "Cadangan kerugian", element: "idx-cor:AllowanceForImpairmentLossesOnFinancing", unit: "IDR", cells },
            { id: "financing", label: "Pembiayaan murabahah", element: "idx-cor:MurabahahReceivables", unit: "IDR", cells },
            { id: "equity", label: "Total ekuitas", element: "idx-cor:Equity", unit: "IDR", cells },
          ],
        },
      ],
    };
    const groups = groupFinancials(bank);
    expect(groups.map((g) => g.label)).toEqual(["Pendapatan", "Pembiayaan dan pendanaan", "Kualitas aset", "Neraca", "Arus kas"]);
    expect(groups[1].rows.map((r) => r.id)).toEqual(["deposits", "financing"]);
    expect(groups[2].rows.map((r) => r.id)).toEqual(["allowance"]);
    expect(groupFinancials(base).map((g) => g.label)).toEqual(["Laba rugi", "Arus kas", "Neraca"]);
  });

  it("share counts from a filing open their fact; manual and derived counts carry their source", () => {
    const filing: ShareCount = { shares: 1, source_label: "laporan", source: { kind: "filing", filing_id: "X_2026_Q2", element: "idx-cor:Shares", period_end: "2026-06-30" } };
    expect(shareTarget(filing)).toEqual({ filing_id: "X_2026_Q2", element: "idx-cor:Shares", ctx_kind: "instant", start: null, end: "2026-06-30", dims: "", ctx: "2026-06-30" });
    const manual = golden<FinancialsResponse>("financials-COAL-standalone").shares.find(Boolean)!;
    expect(shareTarget(manual)).toBeNull();
  });

  it("share sources collapse into runs of consecutive periods with the same source", () => {
    const fin = golden<FinancialsResponse>("financials-COAL-standalone");
    const filing: ShareCount = { shares: 2, source_label: "laporan", source: { kind: "filing", filing_id: "X_2026_Q2", element: "idx-cor:Shares", period_end: "2026-06-30" } };
    const shares = [...fin.shares.slice(0, -2), null, filing];
    expect(shareRuns({ ...fin, shares }).map((r) => [r.from, r.to, r.first, r.last, r.share?.source_label ?? null])).toEqual([
      ["Q3 23", "Q4 25", 0, 9, "manual"],
      ["Q1 26", "Q1 26", 10, 10, null],
      ["Q2 26", "Q2 26", 11, 11, "laporan"],
    ]);
  });

  it("the share scale keeps four digits or more: 371.335.392.068 reads (JUTA LEMBAR) 371.335", () => {
    expect(shareScale([371_335_392_068, null, 371_335_392_068])).toEqual({ divisor: 1e6, word: "juta", header: "(JUTA LEMBAR)" });
    expect(shareScale([500_000_000]).header).toBe("(RIBU LEMBAR)");
    expect(shareScale([800]).header).toBe("(LEMBAR)");
    expect(shareScale([]).header).toBe("(LEMBAR)");
  });

  it("the annual view drops only the derived_fy flag; other views and other flags are untouched", () => {
    const annual = golden<FinancialsResponse>("financials-COAL-annual");
    const flagged = { ...annual, groups: [{ ...annual.groups[0], rows: [{ ...annual.groups[0].rows[0], cells: [{ ...annual.groups[0].rows[0].cells[0], flags: ["derived_fy", "restated"] as const }] }] }] } as unknown as FinancialsResponse;
    expect(withoutAnnualDerived(flagged).groups[0].rows[0].cells[0].flags).toEqual(["restated"]);
    const quarterly = { ...flagged, view: "standalone" } as FinancialsResponse;
    expect(withoutAnnualDerived(quarterly)).toBe(quarterly);
  });

  it("the badge legend lists the given compact badges in a fixed order and skips ones whose marker is the name", () => {
    expect(badgeLegend(["per_saham", "turunan", "pdf"])).toBe("FY = turunan FY · ≈ = Per saham, perkiraan");
    expect(badgeLegend([])).toBe("");
  });

  it("the filing list covers the periods shown; Q4 maps to the FY filing", () => {
    const fin = golden<FinancialsResponse>("financials-COAL-standalone");
    const rows = filingsForPeriods(golden<FilingsResponse>("filings-COAL").items, fin);
    expect(rows.length).toBe(fin.periods.length);
    expect(rows.some((f) => f.period === "FY")).toBe(true);
  });
});
