import { fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it } from "vitest";
import desk from "../../../../tests/fixtures/research/canon/desk.json";
import { mockApi } from "../../test/mockApi";
import type { CoverageRow, DeskResponse } from "../research/types";
import { CompanyRow, CoverageList } from "./CoverageList";

const DESK = desk as unknown as DeskResponse;
const rows = (role: CoverageRow["role"]) => DESK.coverage.filter((r) => r.role === role);
const withCoverage = (coverage: CoverageRow[]): DeskResponse => ({ ...DESK, coverage });

let fetchMock: ReturnType<typeof mockApi>;
beforeEach(() => {
  fetchMock = mockApi({});
});

function renderList(d: DeskResponse) {
  return render(
    <MemoryRouter initialEntries={["/perusahaan"]}>
      <CoverageList desk={d} />
    </MemoryRouter>,
  );
}

const tickers = (list: HTMLElement) => within(list).getAllByRole("listitem").map((li) => li.getAttribute("data-company"));

describe("CoverageList (plan.md §3.3 contract, design.md §3.3 blocks 2–4)", () => {
  it("groups held under Dipegang, manual under Manual and peers under a collapsed Pembanding (n), in backend order", () => {
    renderList(DESK);
    expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent)).toEqual(["Dipegang", "Manual", "Pembanding (1)"]);
    expect(tickers(screen.getByRole("list", { name: "Dipegang" }))).toEqual(["BANK", "COAL", "NONE"]);
    expect(tickers(screen.getByRole("list", { name: "Manual" }))).toEqual(["BUST", "REST"]);
    const toggle = screen.getByRole("button", { name: "Pembanding (1)" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("list", { name: "Pembanding" })).toBeNull();
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(tickers(screen.getByRole("list", { name: "Pembanding" }))).toEqual(["COPR"]);
  });

  it("a collapsible block title starts at the column edge like the others, with its arrow after the text (C8)", () => {
    renderList(DESK);
    const toggle = screen.getByRole("button", { name: "Pembanding (1)" });
    expect(toggle.firstChild?.nodeType).toBe(Node.TEXT_NODE);
    expect(toggle.lastElementChild?.tagName.toLowerCase()).toBe("svg");
    expect(toggle).not.toHaveClass("-ml-1");
  });

  it("omits Manual and Pembanding when they are empty", () => {
    renderList(withCoverage(rows("held")));
    expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent)).toEqual(["Dipegang"]);
    expect(screen.queryByRole("list", { name: "Manual" })).toBeNull();
    expect(screen.queryByRole("button", { name: /Pembanding/ })).toBeNull();
  });

  it("with no held and no manual ticker: finance-app's empty text and a Tambah emiten link to the wizard; peers stay", () => {
    renderList(withCoverage(rows("peer")));
    expect(screen.getByText(/^Belum ada emiten di Riset\. Tambah satu kode emiten/)).toBeInTheDocument();
    const add = screen.getByRole("link", { name: "Tambah emiten" });
    expect(add).toHaveAttribute("href", "/perusahaan/tambah");
    expect(add).toHaveClass("min-h-11");
    expect(screen.queryByRole("list", { name: "Dipegang" })).toBeNull();
    expect(screen.getByRole("button", { name: "Pembanding (1)" })).toBeInTheDocument();
  });

  it("with an empty coverage list renders only the empty state", () => {
    renderList(withCoverage([]));
    expect(screen.getByRole("link", { name: "Tambah emiten" })).toBeInTheDocument();
    expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent)).toEqual(["Dipegang"]);
  });

  it("with manual tickers but none held, Dipegang says so without the add prompt", () => {
    renderList(withCoverage(rows("manual")));
    expect(screen.getByText("Tidak ada emiten yang dipegang.")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Tambah emiten" })).toBeNull();
    expect(tickers(screen.getByRole("list", { name: "Manual" }))).toEqual(["BUST", "REST"]);
  });

  it("does not fetch and has no header, Tambah, footer or Tesis lain (the Perusahaan screen owns them)", () => {
    renderList(DESK);
    expect(fetchMock).not.toHaveBeenCalled();
    expect(screen.queryByRole("heading", { level: 1 })).toBeNull();
    expect(screen.queryByRole("link", { name: "Tambah" })).toBeNull();
    expect(screen.queryByText("Bukan saran investasi")).toBeNull();
    expect(screen.queryByText(/Tesis lain/)).toBeNull();
    expect(screen.queryByRole("contentinfo")).toBeNull();
  });
});

describe("CompanyRow", () => {
  const coal = DESK.coverage.find((r) => r.ticker === "COAL")!;
  const renderRow = (row: CoverageRow) =>
    render(
      <MemoryRouter>
        <ul>
          <CompanyRow row={row} />
        </ul>
      </MemoryRouter>,
    );

  it("is one 2-line link to the company page under Perusahaan, at least 64 px tall", () => {
    renderRow(coal);
    const link = screen.getByRole("link");
    expect(link).toHaveAttribute("href", "/perusahaan/COAL");
    expect(link).toHaveClass("min-h-16");
    const [one, two] = [link.querySelector('[data-line="1"]'), link.querySelector('[data-line="2"]')];
    expect(one).toHaveTextContent("COAL");
    expect(one).toHaveTextContent("Tambang batubara");
    expect(one).toHaveTextContent("Q2 26");
    expect(two).toHaveTextContent(/Laporan berikut: Q3 26, batas 30 Okt/);
    expect(within(link).getByRole("meter", { name: "Cakupan KPI COAL" })).toBeInTheDocument();
  });

  it("an overdue next report is said in words", () => {
    renderRow(DESK.coverage.find((r) => r.ticker === "BUST")!);
    expect(screen.getByRole("link")).toHaveTextContent(/Laporan berikut: Q1 26, batas 30 Apr \(lewat\)/);
  });

  it("a ticker without reports or pack says so", () => {
    renderRow({ ...coal, pack: { ...coal.pack, label: null }, next_report: null });
    const one = screen.getByRole("link").querySelector('[data-line="1"]');
    expect(one).toHaveTextContent("Tanpa paket");
    expect(one).toHaveTextContent("belum ada laporan");
  });
});
