import { fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import calibration from "../../../../../tests/fixtures/research/canon/calibration.json";
import compare from "../../../../../tests/fixtures/research/canon/compare-coal_mining.json";
import desk from "../../../../../tests/fixtures/research/canon/desk.json";
import packs from "../../../../../tests/fixtures/research/canon/packs.json";
import rules from "../../../../../tests/fixtures/research/canon/rules.json";
import universe from "../../../../../tests/fixtures/research/canon/universe.json";
import { mockApi } from "../../../test/mockApi";
import { MACRO } from "../../research/desk/macro.testutil";
import { noPageOverflowProblems } from "../../research/screenAsserts.testutil";
import { setViewport } from "../../research/testing/viewport";
import type { CalibrationRow, CoverageRow, DeskResponse } from "../../research/types";
import { RisetRoutes } from "../RisetRoutes";
import { outcomeText } from "./Calibration";
import { gapOf } from "./Compare";
import { groupCoverage, latestReportQuarter, previousQuarter } from "./landingData";

const DESK = desk as unknown as DeskResponse;

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/riset/*" element={<RisetRoutes />} />
      </Routes>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  setViewport(393);
  mockApi({
    "/api/research/desk": DESK,
    "/api/macro/observations": MACRO,
    "/api/research/calibration": calibration,
    "/api/research/compare": compare,
    "/api/research/packs": packs,
    "/api/research/rules": rules,
    "/api/research/universe": universe,
    "/api/research/lookup": { as_of: "2026-10-24T09:00:00+00:00", query: "", list_available: true, exact: null, items: [] },
  });
});
afterEach(() => setViewport(1280));

describe("landing #/riset (ui-slicing.md §2)", () => {
  it("blocks in order: header with Tambah, companies, inbox, calendar (collapsed), links, footer", async () => {
    renderAt("/riset");
    await screen.findByRole("heading", { level: 2, name: "Perusahaan saya" });
    expect(screen.getByRole("heading", { level: 1, name: "Riset" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Tambah" })).toHaveAttribute("href", "/riset/add");
    const titles = screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent);
    expect(titles.slice(0, 4)).toEqual(["Perusahaan saya", "Kotak masuk", "Kalender", "Lainnya"]);
    expect(screen.getByRole("button", { name: "Kalender" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText("Bukan saran investasi")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: /Kartu cakupan/ })).toBeNull();
  });

  it("one 2-line row per held and manual ticker linking to its company page; peers are collapsed", async () => {
    renderAt("/riset");
    const list = await screen.findByRole("list", { name: "Perusahaan saya" });
    const rows = within(list).getAllByRole("listitem");
    expect(rows.map((r) => r.getAttribute("data-company"))).toEqual(["BANK", "COAL", "NONE", "BUST", "REST"]);
    const coal = rows[1];
    expect(within(coal).getByRole("link")).toHaveAttribute("href", "/riset/COAL");
    expect(coal.querySelector('[data-line="1"]')).toHaveTextContent("COAL");
    expect(coal.querySelector('[data-line="1"]')).toHaveTextContent("Tambang batubara");
    expect(coal.querySelector('[data-line="1"]')).toHaveTextContent("Q2 26");
    expect(coal.querySelector('[data-line="2"]')).toHaveTextContent(/Laporan berikut: Q3 26, batas 30 Okt/);
    expect(within(coal).getByRole("link")).toHaveClass("min-h-16");
    expect(screen.queryByRole("list", { name: "Pembanding" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Pembanding (1)" }));
    expect(within(screen.getByRole("list", { name: "Pembanding" })).getByText("COPR")).toBeInTheDocument();
  });

  it("an overdue next report is said in words", async () => {
    renderAt("/riset");
    const list = await screen.findByRole("list", { name: "Perusahaan saya" });
    const bust = within(list).getAllByRole("listitem").find((r) => r.getAttribute("data-company") === "BUST")!;
    expect(bust).toHaveTextContent(/Laporan berikut: Q1 26, batas 30 Apr \(lewat\)/);
  });

  it("the inbox shows the first five with Lihat semua, and filter chips scroll in one row", async () => {
    renderAt("/riset");
    const region = await screen.findByRole("group", { name: "Kotak masuk" });
    expect(within(region).getAllByRole("listitem")).toHaveLength(5);
    fireEvent.click(screen.getByRole("button", { name: `Lihat semua (${DESK.inbox.length})` }));
    expect(within(region).getAllByRole("listitem")).toHaveLength(DESK.inbox.length);
    const chips = screen.getByRole("group", { name: "Filter kotak masuk" });
    expect(chips).toHaveClass("hscroll");
    expect(within(chips).getAllByRole("button").length).toBe(DESK.filters.length);
    fireEvent.click(within(chips).getByRole("button", { name: /^Klaim/ }));
    expect(within(region).getAllByRole("listitem")).toHaveLength(1);
  });

  it("the chip row keeps the 18 px gutter when snapping, and an inbox line may take two lines (C8)", async () => {
    renderAt("/riset");
    const region = await screen.findByRole("group", { name: "Kotak masuk" });
    expect(screen.getByRole("group", { name: "Filter kotak masuk" })).toHaveClass("scroll-px-[var(--gutter-left)]", "px-[var(--gutter-left)]");
    const second = region.querySelector('[data-line="2"]')!;
    expect(second).toHaveClass("line-clamp-2");
    expect(second).not.toHaveClass("truncate");
    expect(region.querySelector("li")).toHaveClass("min-h-[60px]");
  });

  it("says how many of how many when the inbox list is shorter than the chip count (C8)", async () => {
    const total = 974;
    mockApi({
      "/api/research/desk": { ...DESK, filters: DESK.filters.map((f) => (f.group === "semua" ? { ...f, count: total } : f)) },
      "/api/macro/observations": MACRO,
    });
    renderAt("/riset");
    const region = await screen.findByRole("group", { name: "Kotak masuk" });
    expect(screen.getByRole("button", { name: `Semua ${total}` })).toBeInTheDocument();
    const more = screen.getByRole("button", { name: `Lihat semua (${DESK.inbox.length} terbaru dari ${total})` });
    // At 200% text zoom (197 px) the long label wraps inside the button instead of running past the screen edge (M7).
    expect(more).toHaveClass("max-w-full", "!whitespace-normal", "min-h-11");
    expect(more).not.toHaveClass("h-11");
    fireEvent.click(more);
    expect(within(region).getAllByRole("listitem")).toHaveLength(DESK.inbox.length);
    expect(screen.getByText(`Menampilkan ${DESK.inbox.length} terbaru dari ${total}`)).toBeInTheDocument();
  });

  it("a collapsible block title starts at the column edge like the others, with its arrow after the text (C8)", async () => {
    renderAt("/riset");
    const toggle = await screen.findByRole("button", { name: "Kalender" });
    expect(toggle.firstChild?.nodeType).toBe(Node.TEXT_NODE);
    expect(toggle.lastElementChild?.tagName.toLowerCase()).toBe("svg");
    expect(toggle).not.toHaveClass("-ml-1");
  });

  it("Lainnya links to Bandingkan, Kalibrasi and Paket & aturan at 44 px", async () => {
    renderAt("/riset");
    const more = (await screen.findByRole("heading", { name: "Lainnya" })).closest("section") as HTMLElement;
    const links = within(more).getAllByRole("link");
    expect(links.map((l) => [l.textContent, l.getAttribute("href")])).toEqual([
      ["Bandingkan", "/riset/compare"],
      ["Kalibrasi", "/riset/compare/calibration"],
      ["Paket & aturan", "/riset/packs"],
    ]);
    for (const l of links) expect(l).toHaveClass("min-h-11");
  });

  it("the calendar opens to five two-line rows", async () => {
    renderAt("/riset");
    fireEvent.click(await screen.findByRole("button", { name: "Kalender" }));
    const cal = screen.getByRole("heading", { name: "Kalender" }).closest("section") as HTMLElement;
    expect(within(cal).getAllByRole("listitem")).toHaveLength(Math.min(5, DESK.calendar.length));
  });
});

describe("landingData", () => {
  it("derives the latest report quarter from the next one, across the year boundary", () => {
    expect(previousQuarter("2026Q3")).toBe("2026Q2");
    expect(previousQuarter("2027Q1")).toBe("2026Q4");
    expect(previousQuarter("nonsense")).toBeNull();
    expect(latestReportQuarter({ next_report: null })).toBeNull();
    expect(latestReportQuarter({ next_report: { label: "2026Q3", deadline: "2026-10-30", overdue: false } })).toBe("2026Q2");
  });

  it("groups held, manual and peer rows in backend order", () => {
    const g = groupCoverage(DESK.coverage as CoverageRow[]);
    expect(g.held.map((r) => r.ticker)).toEqual(["BANK", "COAL", "NONE"]);
    expect(g.manual.map((r) => r.ticker)).toEqual(["BUST", "REST"]);
    expect(g.peers.map((r) => r.ticker)).toEqual(["COPR"]);
  });
});

describe("Bandingkan #/riset/compare (§3)", () => {
  it("a pack select and wrapping ticker chips above one table that scrolls inside its own box", async () => {
    const { container } = renderAt("/riset/compare");
    const region = await screen.findByRole("region", { name: /Tabel perbandingan/ });
    expect(region).toHaveClass("overflow-x-auto", "max-w-full");
    expect(region).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("group", { name: "Emiten yang dibandingkan" })).toHaveClass("flex-wrap");
    expect(screen.getByRole("combobox", { name: "Paket" })).toBeInTheDocument();
    const first = within(region).getAllByRole("rowheader")[0];
    expect(first).toHaveClass("sticky", "left-0");
    expect(noPageOverflowProblems(container.firstElementChild as HTMLElement).filter((p) => !p.startsWith("akar layar"))).toEqual([]);
    expect(screen.getByRole("link", { name: /Riset$/ })).toHaveAttribute("href", "/riset");
  });

  it("every action in the empty-cells list, Isi included, is at least 44 × 44 (C8)", async () => {
    renderAt("/riset/compare");
    const summary = await screen.findByText(/^Sel kosong \(/);
    const list = summary.closest("details") as HTMLElement;
    const actions = within(list).getAllByRole("link", { hidden: true });
    expect(actions.length).toBeGreaterThan(0);
    for (const a of actions) expect(a).toHaveClass("min-h-11", "min-w-11", "justify-center");
    expect(actions.some((a) => a.textContent === "Isi")).toBe(true);
  });

  it("a bank pack hides the core rows a bank cannot carry, the same list as the company page (E3)", async () => {
    const BANK_ONLY_HIDDEN = ["gross_profit", "capex", "free_cash_flow", "ebitda", "total_debt", "net_debt"];
    const rows = (compare as never as { rows: Array<{ group: string; metric: { id: string; label: string } }> }).rows;
    const hidden = rows.filter((r) => r.group === "core" && BANK_ONLY_HIDDEN.includes(r.metric.id));
    expect(hidden.length).toBe(BANK_ONLY_HIDDEN.length);
    mockApi({
      "/api/research/compare": { ...compare, pack: { id: "bank_sharia", label: "Bank syariah", version: "2026.10-1" } },
      "/api/research/packs": packs,
      "/api/research/universe": universe,
    });
    renderAt("/riset/compare");
    const region = await screen.findByRole("region", { name: /Tabel perbandingan/ });
    const labels = within(region)
      .getAllByRole("rowheader")
      .filter((h) => h.getAttribute("scope") === "row")
      .map((h) => h.querySelector("span > span")?.textContent ?? "");
    expect(labels).toContain("Pendapatan");
    for (const r of hidden) expect(labels).not.toContain(r.metric.label);
    expect(screen.getByRole("status")).toHaveTextContent(`${rows.length - hidden.length} metrik`);
  });

  it("a non-bank pack keeps every core row (E3)", async () => {
    renderAt("/riset/compare");
    const region = await screen.findByRole("region", { name: /Tabel perbandingan/ });
    const labels = within(region)
      .getAllByRole("rowheader")
      .filter((h) => h.getAttribute("scope") === "row")
      .map((h) => h.querySelector("span > span")?.textContent ?? "");
    const ebitda = (compare as never as { rows: Array<{ metric: { id: string; label: string } }> }).rows.find((r) => r.metric.id === "ebitda")!;
    expect(labels).toContain(ebitda.metric.label);
  });

  it("the metric column wraps its labels and is at least 9 rem, with no ellipsis at any width (M1)", async () => {
    renderAt("/riset/compare");
    const region = await screen.findByRole("region", { name: /Tabel perbandingan/ });
    const header = within(region).getByRole("columnheader", { name: "Metrik" });
    expect(header).toHaveClass("min-w-[9rem]");
    expect(header.className).not.toMatch(/w-\[104px\]/);
    const metricCells = within(region).getAllByRole("rowheader").filter((th) => th.getAttribute("scope") === "row");
    expect(metricCells.length).toBeGreaterThan(0);
    for (const th of metricCells) {
      expect(th).toHaveClass("min-w-[9rem]");
      expect(th.querySelector(".truncate")).toBeNull();
    }
  });

  const unitHeaders = (region: HTMLElement) =>
    within(region)
      .getAllByRole("rowheader")
      .filter((h) => h.getAttribute("scope") === "row")
      .map((h) => [h.querySelector("span > span")?.textContent ?? "", h.querySelector("span > span:last-child")?.textContent ?? ""]);
  const withCurrencies = (currencies: string[]) => ({ ...compare, tickers: compare.tickers.map((t, i) => ({ ...t, currency: currencies[i] })) });

  it("TTM rows carry the same (TTM) suffix as the company page, other rows keep their label (N9)", async () => {
    const rows = (compare as never as { rows: Array<{ metric: { label: string; period?: string | null } }> }).rows;
    const ttm = rows.filter((r) => r.metric.period === "ttm").map((r) => r.metric.label);
    const plain = rows.filter((r) => r.metric.period !== "ttm").map((r) => r.metric.label);
    expect(ttm.length).toBeGreaterThan(0);
    renderAt("/riset/compare");
    const region = await screen.findByRole("region", { name: /Tabel perbandingan/ });
    const labels = unitHeaders(region).map(([label]) => label);
    for (const l of ttm) expect(labels).toContain(`${l} (TTM)`);
    for (const l of plain) expect(labels).not.toContain(`${l} (TTM)`);
  });

  it("rupiah reporters share one scale on every money row, so Rp miliar and Rp triliun never mix (N8)", async () => {
    mockApi({ "/api/research/compare": withCurrencies(["IDR", "IDR"]), "/api/research/packs": packs, "/api/research/universe": universe });
    renderAt("/riset/compare");
    const region = await screen.findByRole("region", { name: /Tabel perbandingan/ });
    const money = unitHeaders(region).map(([, unit]) => unit).filter((u) => /^\(RP /.test(u));
    expect(money.length).toBeGreaterThan(1);
    expect(new Set(money)).toEqual(new Set(["(RP MILIAR)"]));
  });

  it("columns in different currencies write their unit under the ticker and the money rows defer to it (N8)", async () => {
    mockApi({ "/api/research/compare": withCurrencies(["IDR", "USD"]), "/api/research/packs": packs, "/api/research/universe": universe });
    renderAt("/riset/compare");
    const region = await screen.findByRole("region", { name: /Tabel perbandingan/ });
    const heads = within(region).getAllByRole("columnheader").map((h) => h.textContent);
    expect(heads[1]).toMatch(/^COALRp M$/i);
    expect(heads[2]).toMatch(/^COPRUSD( jt| rb| M| T)?$/i);
    expect(unitHeaders(region).some(([, unit]) => unit === "(SESUAI KOLOM)")).toBe(true);
  });

  it("an empty cell explains itself and offers its action", () => {
    const row = (compare as never as { rows: Array<{ cells: Array<{ ticker: string; reason: string | null; state: string; value: number | null }> }> }).rows[0];
    const cell = { ...row.cells[0], state: "gap" as const, value: null, reason: "not_entered" };
    expect(gapOf(cell as never, row as never)).toEqual({ why: "Belum diisi", action: "Isi" });
    expect(gapOf({ ...cell, reason: "not_defined" } as never, { metric: { label: "Produksi" } } as never).action).toBe("Tentukan paket");
  });
});

describe("Kalibrasi #/riset/compare/calibration (§4)", () => {
  it("hit-rate lines first as text, then two-line cards, two selects", async () => {
    renderAt("/riset/compare/calibration");
    const list = await screen.findByRole("list", { name: "Butir kalibrasi" });
    const cards = within(list).getAllByRole("listitem");
    expect(cards).toHaveLength((calibration as { items: unknown[] }).items.length);
    expect(within(cards[0]).getByText("BANK")).toBeInTheDocument();
    expect(screen.getByText("COAL: 2 Berlaku, 1 Patah (67%)")).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: /^Ticker/ })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: /^Jenis/ })).toBeInTheDocument();
    expect(screen.getAllByRole("combobox")).toHaveLength(2);
    expect(cards[0].querySelector('[data-line="1"]')).toBeInTheDocument();
    expect(cards[0].querySelector('[data-line="2"]')).toBeInTheDocument();
  });

  it("writes the outcome as 'Rp1.530 → Rp1.710 (+11,8%)' once matured, the maturity date before", () => {
    const base = { matured: true, matures_on: "2026-11-23", price_then: 1530, price_now: 1710, return_pct: 11.76 } as CalibrationRow;
    expect(outcomeText(base)).toEqual({ text: "Rp1.530 → Rp1.710 (+11,8%)", gap: false });
    expect(outcomeText({ ...base, matured: false }).text).toBe("Matang 23 Nov 2026");
    expect(outcomeText({ ...base, price_now: null })).toEqual({ text: "Harga belum lengkap, hasil belum dihitung", gap: true });
  });
});

describe("Paket & aturan #/riset/packs (§5)", () => {
  it("a select instead of the pack list, one card per dimension, formulas behind Rumus, drivers, rules, Muat ulang", async () => {
    renderAt("/riset/packs");
    const select = await screen.findByRole("combobox", { name: "Paket" });
    expect(select.querySelectorAll("option").length).toBeGreaterThan(1);
    const dims = screen.getAllByRole("region").filter((r) => /^Dimensi /.test(r.getAttribute("aria-label") ?? ""));
    expect(dims.length).toBeGreaterThan(0);
    const metric = dims[0].querySelector("li[data-metric]") as HTMLElement;
    expect(within(metric).getByText("Rumus")).toBeInTheDocument();
    for (const summary of screen.getAllByText("Rumus")) expect(summary).toHaveClass("min-h-11", "min-w-11");
    expect(metric.querySelector("details")).not.toHaveAttribute("open");
    expect(screen.getByRole("heading", { name: "Pendorong" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Aturan" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Muat ulang" })).toBeInTheDocument();
  });

  it("an identifier formula is preceded by the same formula read with labels, identifiers staying behind Rumus (N6)", async () => {
    type Metric = { id: string; layer: string; formula: string | null; bindings: Record<string, string[]> | null };
    const list = (packs as never as { packs: Array<{ id: string; dimensions: Array<{ metrics: Metric[] }> }> }).packs;
    const first = list.filter((p) => p.id !== "core")[0];
    const metric = first.dimensions.flatMap((d) => d.metrics).find((m) => m.layer === "xbrl" && m.formula && /[A-Z][a-z]+[A-Z]/.test(m.formula))!;
    expect(metric).toBeDefined();
    renderAt("/riset/packs");
    await screen.findByRole("combobox", { name: "Paket" });
    const item = document.querySelector(`li[data-metric="${metric.id}"]`) as HTMLElement;
    const lines = [...item.querySelectorAll("details p")].map((p) => p.textContent ?? "");
    expect(lines[0]).not.toBe("");
    expect(lines[0]).not.toMatch(/[a-z][A-Z]/);
    expect(lines.some((l) => l.startsWith("Penanda: ") && l.includes(metric.formula!))).toBe(true);
    expect(item.querySelector("details")).not.toHaveAttribute("open");
  });
});

describe("Tambah emiten #/riset/add (§6)", () => {
  it("full-width with 'Langkah 1 dari 4' text, no step list, and a sticky primary action", async () => {
    renderAt("/riset/add");
    expect(await screen.findByRole("heading", { level: 2, name: "Langkah 1 dari 4: Emiten" })).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Langkah" })).toBeNull();
    const bar = document.querySelector("[data-sticky-actions]") as HTMLElement;
    expect(bar).toHaveClass("sticky");
    expect(within(bar).getByRole("button", { name: "Lanjut" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Riset$/ })).toHaveAttribute("href", "/riset");
  });
});
