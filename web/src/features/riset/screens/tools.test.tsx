import { fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import calibration from "../../../../../tests/fixtures/research/canon/calibration.json";
import compare from "../../../../../tests/fixtures/research/canon/compare-coal_mining.json";
import packs from "../../../../../tests/fixtures/research/canon/packs.json";
import rules from "../../../../../tests/fixtures/research/canon/rules.json";
import universe from "../../../../../tests/fixtures/research/canon/universe.json";
import { mockApi } from "../../../test/mockApi";
import { noPageOverflowProblems } from "../../research/screenAsserts.testutil";
import { setViewport } from "../../research/testing/viewport";
import type { CalibrationRow } from "../../research/types";
import { Calibration, outcomeText } from "./Calibration";
import { Compare, gapOf } from "./Compare";
import { Packs } from "./Packs";

/** The three tools at their design.md §2.2 routes, phone layout only (393 px). */
function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/alat/bandingkan" element={<Compare />} />
        <Route path="/alat/kalibrasi" element={<Calibration />} />
        <Route path="/alat/paket" element={<Packs />} />
      </Routes>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  setViewport(393);
  mockApi({
    "/api/research/calibration": calibration,
    "/api/research/compare": compare,
    "/api/research/packs": packs,
    "/api/research/rules": rules,
    "/api/research/universe": universe,
  });
});
afterEach(() => setViewport(1280));

describe("Bandingkan #/alat/bandingkan (§3)", () => {
  it("a pack select and wrapping ticker chips above one table that scrolls inside its own box", async () => {
    const { container } = renderAt("/alat/bandingkan");
    const region = await screen.findByRole("region", { name: /Tabel perbandingan/ });
    expect(region).toHaveClass("overflow-x-auto", "max-w-full");
    expect(region).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("group", { name: "Emiten yang dibandingkan" })).toHaveClass("flex-wrap");
    expect(screen.getByRole("combobox", { name: "Paket" })).toBeInTheDocument();
    const first = within(region).getAllByRole("rowheader")[0];
    expect(first).toHaveClass("sticky", "left-0");
    expect(noPageOverflowProblems(container.firstElementChild as HTMLElement).filter((p) => !p.startsWith("akar layar"))).toEqual([]);
    expect(screen.getByRole("link", { name: "Kembali ke Alat" })).toHaveAttribute("href", "/alat");
  });

  it("every action in the empty-cells list, Isi included, is at least 44 × 44 (C8)", async () => {
    renderAt("/alat/bandingkan");
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
    renderAt("/alat/bandingkan");
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
    renderAt("/alat/bandingkan");
    const region = await screen.findByRole("region", { name: /Tabel perbandingan/ });
    const labels = within(region)
      .getAllByRole("rowheader")
      .filter((h) => h.getAttribute("scope") === "row")
      .map((h) => h.querySelector("span > span")?.textContent ?? "");
    const ebitda = (compare as never as { rows: Array<{ metric: { id: string; label: string } }> }).rows.find((r) => r.metric.id === "ebitda")!;
    expect(labels).toContain(ebitda.metric.label);
  });

  it("the metric column wraps its labels and is at least 9 rem, with no ellipsis at any width (M1)", async () => {
    renderAt("/alat/bandingkan");
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
    renderAt("/alat/bandingkan");
    const region = await screen.findByRole("region", { name: /Tabel perbandingan/ });
    const labels = unitHeaders(region).map(([label]) => label);
    for (const l of ttm) expect(labels).toContain(`${l} (TTM)`);
    for (const l of plain) expect(labels).not.toContain(`${l} (TTM)`);
  });

  it("rupiah reporters share one scale on every money row, so Rp miliar and Rp triliun never mix (N8)", async () => {
    mockApi({ "/api/research/compare": withCurrencies(["IDR", "IDR"]), "/api/research/packs": packs, "/api/research/universe": universe });
    renderAt("/alat/bandingkan");
    const region = await screen.findByRole("region", { name: /Tabel perbandingan/ });
    const money = unitHeaders(region).map(([, unit]) => unit).filter((u) => /^\(RP /.test(u));
    expect(money.length).toBeGreaterThan(1);
    expect(new Set(money)).toEqual(new Set(["(RP MILIAR)"]));
  });

  it("columns in different currencies write their unit under the ticker and the money rows defer to it (N8)", async () => {
    mockApi({ "/api/research/compare": withCurrencies(["IDR", "USD"]), "/api/research/packs": packs, "/api/research/universe": universe });
    renderAt("/alat/bandingkan");
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

describe("Kalibrasi #/alat/kalibrasi (§4)", () => {
  it("hit-rate lines first as text, then two-line cards, two selects", async () => {
    renderAt("/alat/kalibrasi");
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

describe("Paket & aturan #/alat/paket (§5)", () => {
  it("a select instead of the pack list, one card per dimension, formulas behind Rumus, drivers, rules, Muat ulang", async () => {
    renderAt("/alat/paket");
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
    renderAt("/alat/paket");
    await screen.findByRole("combobox", { name: "Paket" });
    const item = document.querySelector(`li[data-metric="${metric.id}"]`) as HTMLElement;
    const lines = [...item.querySelectorAll("details p")].map((p) => p.textContent ?? "");
    expect(lines[0]).not.toBe("");
    expect(lines[0]).not.toMatch(/[a-z][A-Z]/);
    expect(lines.some((l) => l.startsWith("Penanda: ") && l.includes(metric.formula!))).toBe(true);
    expect(item.querySelector("details")).not.toHaveAttribute("open");
  });
});

describe("the three tools on the phone (design.md §2.4, §3.5)", () => {
  const BACK = { name: "Kembali ke Alat" };

  it.each([
    ["Bandingkan", "/alat/bandingkan"],
    ["Kalibrasi", "/alat/kalibrasi"],
    ["Paket & aturan", "/alat/paket"],
  ])("%s: the h1, and a 44 px back control '‹ Alat' to the Alat list", async (title, path) => {
    renderAt(path);
    expect(await screen.findByRole("heading", { level: 1, name: title })).toBeInTheDocument();
    const back = screen.getByRole("link", BACK);
    expect(back).toHaveAttribute("href", "/alat");
    expect(back).toHaveClass("h-11");
    expect(back).toHaveTextContent("Alat");
    expect(screen.queryByRole("link", { name: /Riset$/ })).toBeNull();
  });

  it.each(["/alat/bandingkan", "/alat/kalibrasi", "/alat/paket"])("%s: the column has no 720 px cap and no desktop breakpoint", async (path) => {
    const { container } = renderAt(path);
    await screen.findByRole("heading", { level: 1 });
    const page = container.firstElementChild as HTMLElement;
    expect(page.className).not.toMatch(/max-w-\[720px\]/);
    expect(container.innerHTML).not.toMatch(/\b(sm|md|lg|xl|2xl):/);
  });

  it("Bandingkan: a ticker column head and a source-less value link to the company page carrying from=alat", async () => {
    renderAt("/alat/bandingkan");
    const region = await screen.findByRole("region", { name: /Tabel perbandingan/ });
    const heads = within(region).getAllByRole("columnheader").flatMap((h) => within(h).queryAllByRole("link"));
    expect(heads.length).toBeGreaterThan(0);
    for (const a of heads) expect(a.getAttribute("href")).toMatch(/^\/perusahaan\/[A-Z]+\?from=alat$/);
    for (const a of within(region).queryAllByRole("link")) expect(a.getAttribute("href")).toMatch(/^\/perusahaan\/[A-Z]+\?(section=kuartal&)?from=alat$/);
  });

  it("Bandingkan: empty-cell actions go to the company page's Kuartal block with from=alat", async () => {
    renderAt("/alat/bandingkan");
    const summary = await screen.findByText(/^Sel kosong \(/);
    const actions = within(summary.closest("details") as HTMLElement).getAllByRole("link", { hidden: true });
    for (const a of actions) expect(a.getAttribute("href")).toMatch(/^\/perusahaan\/[A-Z]+\?section=kuartal&from=alat$/);
  });

  it("Bandingkan: with no pack, Lihat paket goes to Paket & aturan under Alat", async () => {
    mockApi({ "/api/research/packs": { ...packs, packs: [] }, "/api/research/universe": universe });
    renderAt("/alat/bandingkan");
    expect(await screen.findByRole("link", { name: "Lihat paket" })).toHaveAttribute("href", "/alat/paket");
  });

  it("Kalibrasi: Lihat harga goes to the company page's Harga block with from=alat", async () => {
    const items = [{ ...(calibration as { items: CalibrationRow[] }).items[0], matured: true, price_now: null }];
    mockApi({ "/api/research/calibration": { ...calibration, items }, "/api/research/universe": universe });
    renderAt("/alat/kalibrasi");
    const link = await screen.findByRole("link", { name: `Lihat harga ${items[0].ticker}` });
    expect(link).toHaveAttribute("href", `/perusahaan/${items[0].ticker}?section=harga&from=alat`);
  });

  it("Kalibrasi: the empty state points to Perusahaan, not to Riset", async () => {
    mockApi({ "/api/research/calibration": { ...calibration, items: [], hit_rates: [] }, "/api/research/universe": universe });
    renderAt("/alat/kalibrasi");
    expect(await screen.findByRole("link", { name: "Ke Perusahaan" })).toHaveAttribute("href", "/perusahaan");
    expect(screen.queryByText("Ke Riset")).toBeNull();
  });

  it("Bandingkan: a value with a source opens FactDrawer as a bottom Sheet at every width, never a 380 px side panel", async () => {
    setViewport(1440);
    renderAt("/alat/bandingkan");
    const region = await screen.findByRole("region", { name: /Tabel perbandingan/ });
    const chip = within(region).getAllByRole("button").find((b) => /^Asal angka /.test(b.getAttribute("aria-label") ?? ""));
    expect(chip).toBeDefined();
    fireEvent.click(chip!);
    const dialog = await screen.findByRole("dialog", { name: "Asal angka" });
    expect(dialog).not.toHaveClass("w-[380px]");
    expect(dialog).toHaveClass("max-w-mobile");
  });
});
