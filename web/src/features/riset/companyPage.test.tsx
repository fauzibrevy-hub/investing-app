import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import claims from "../../../../tests/fixtures/research/canon/claims-COAL.json";
import documents from "../../../../tests/fixtures/research/canon/documents-COAL.json";
import financials from "../../../../tests/fixtures/research/canon/financials-COAL-standalone.json";
import journal from "../../../../tests/fixtures/research/canon/journal-COAL.json";
import pack from "../../../../tests/fixtures/research/canon/pack-COAL.json";
import prices from "../../../../tests/fixtures/research/canon/prices-COAL-1y.json";
import dossier from "../../../../tests/fixtures/research/canon/ticker-COAL.json";
import structure from "../../../../tests/fixtures/research/canon/structure-COAL.json";
import valuation from "../../../../tests/fixtures/research/canon/valuation-COAL.json";
import { json, mockApi } from "../../test/mockApi";
import { yoyCellText } from "../research/change";
import { formatScaled } from "../research/format";
import { noPageOverflowProblems } from "../research/screenAsserts.testutil";
import { setViewport } from "../research/testing/viewport";
import { CompanyPage } from "./CompanyPage";
import { lastQuarterGroups, tableScale, unitCaption } from "./lastQuarter";
import { sectionTarget } from "./sections";

const base = "/api/research/COAL";

function renderPage(search = "") {
  return render(
    <MemoryRouter initialEntries={[`/perusahaan/COAL${search}`]}>
      <Routes>
        <Route path="/perusahaan/:ticker" element={<CompanyPage />} />
      </Routes>
    </MemoryRouter>,
  );
}

function mockCompany(packBody: unknown = pack) {
  return mockApi({
    [base]: dossier,
    [`${base}/pack`]: packBody,
    [`${base}/financials`]: financials,
    [`${base}/valuation`]: valuation,
    [`${base}/claims`]: claims,
    [`${base}/journal`]: journal,
    [`${base}/documents`]: documents,
    [`${base}/prices`]: prices,
    // Peta kosong: golden mesin tanpa node.
    [`${base}/structure`]: { ...structure, nodes: [], counts: { proposed: 0, accepted: 0, rejected: 0 }, updated_at: null, draft: { ...structure.draft, available: false } },
    [`${base}/seen`]: {},
    "/api/thesis/COAL": () => json({ detail: "Dossier COAL tidak ditemukan" }, 404),
    "/api/thesis/drafts": { drafts: [], vault_drafts: [], total: 0 },
  });
}

beforeEach(() => {
  setViewport(393);
  mockCompany();
});
afterEach(() => setViewport(1280));

describe("company page #/perusahaan/:ticker (ui-slicing.md §1, design.md §3.4)", () => {
  it("one column in the spec's block order with a single h1, headings in order", async () => {
    const { container } = renderPage();
    await screen.findByRole("heading", { level: 2, name: /Kuartal terakhir · Q2 26/ });
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("COAL");
    const titles = screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent);
    expect(titles.slice(0, 5)).toEqual(["Struktur bisnis", "Angka kunci", "Kuartal terakhir · Q2 26", "Membaik / Diawasi", "Klaim"]);
    expect(titles.slice(-4)).toEqual(["Jurnal", "Tesis", "Harga", "Keuangan"]);
    expect(titles).toContain("Data belum ada");
    const page = container.firstElementChild as HTMLElement;
    expect(page).toHaveClass("w-full", "min-w-0");
    // Kolom satu lebar cangkang (D13): yang diperiksa tabel di dalam penggulungnya.
    expect(noPageOverflowProblems(page).filter((p) => !p.startsWith("akar layar"))).toEqual([]);
    expect(screen.getByRole("link", { name: "Kembali ke Perusahaan" })).toHaveAttribute("href", "/perusahaan");
  });

  it("Jurnal and Keuangan start collapsed with the heading as the toggle, Harga starts open", async () => {
    renderPage();
    await screen.findByRole("heading", { level: 2, name: "Jurnal" });
    const jurnal = screen.getByRole("button", { name: "Jurnal" });
    const keuangan = screen.getByRole("button", { name: "Keuangan" });
    const harga = screen.getByRole("button", { name: "Harga" });
    expect(jurnal).toHaveAttribute("aria-expanded", "false");
    expect(keuangan).toHaveAttribute("aria-expanded", "false");
    expect(harga).toHaveAttribute("aria-expanded", "true");
    expect(screen.queryByRole("form", { name: /Catatan/ })).toBeNull();
    fireEvent.click(jurnal);
    expect(jurnal).toHaveAttribute("aria-expanded", "true");
    await screen.findByRole("region", { name: "Entri jurnal" });
    fireEvent.click(harga);
    expect(harga).toHaveAttribute("aria-expanded", "false");
  });

  it("a KPI not yet entered shows Isi, which opens the entry sheet with Tidak diketahui and the document upload", async () => {
    renderPage();
    const isi = await screen.findAllByRole("button", { name: /^Isi Harga jual rata-rata Q2 26$/i });
    fireEvent.click(isi[0]);
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByLabelText("Tidak diketahui")).toBeInTheDocument();
    expect(within(dialog).getByRole("region", { name: "Unggah dokumen" })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Simpan" })).toBeInTheDocument();
  });

  it("?section= opens the collapsed block it points at, including old section names", async () => {
    renderPage("?section=jurnal");
    const jurnal = await screen.findByRole("button", { name: "Jurnal" });
    expect(jurnal).toHaveAttribute("aria-expanded", "true");
  });

  it("Tesis sits between Jurnal and Harga, collapsed, and loads nothing until opened", async () => {
    const fetchMock = mockCompany();
    const thesisCalls = () => fetchMock.mock.calls.filter(([url]) => String(url).includes("/api/thesis/")).length;
    renderPage();
    const tesis = await screen.findByRole("button", { name: "Tesis" });
    expect(tesis).toHaveAttribute("aria-expanded", "false");
    expect(tesis.closest("section")).toHaveAttribute("id", "tesis");
    expect(thesisCalls()).toBe(0);
    fireEvent.click(tesis);
    expect(await screen.findByText("Belum ada tesis untuk COAL")).toBeInTheDocument();
    expect(thesisCalls()).toBe(2);
  });

  it.each(["tesis", "thesis"])("?section=%s opens the Tesis block and scrolls it to the top", async (section) => {
    const scroll = vi.fn();
    const original = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = scroll;
    try {
      renderPage(`?section=${section}`);
      const tesis = await screen.findByRole("button", { name: "Tesis" });
      expect(tesis).toHaveAttribute("aria-expanded", "true");
      await waitFor(() => expect(scroll.mock.contexts).toContain(document.getElementById("tesis")));
      expect(scroll).toHaveBeenCalledWith({ block: "start" });
    } finally {
      Element.prototype.scrollIntoView = original;
    }
  });

  it("a ticker outside the research universe shows the thesis-only state (prd.md O1): header, Tesis open, Tambah ke riset", async () => {
    mockApi({
      "/api/research/ZZZZ": () => json({ detail: "Unknown ticker" }, 404),
      "/api/thesis/ZZZZ": () => json({ detail: "Dossier ZZZZ tidak ditemukan" }, 404),
      "/api/thesis/drafts": { drafts: [{ id: "d1", ticker: "ZZZZ", title: "Draf ZZZZ", thesis_statement: "s", thesis_type: "narrative", is_primary: false, scenario_status: "Menunggu", story_status: "Belum terkonfirmasi", falsifiers: [], triggers_add: [], triggers_exit: [], notes: "", created_at: "2026-10-20", updated_at: "2026-10-20" }], vault_drafts: [], total: 1 },
    });
    render(
      <MemoryRouter initialEntries={["/perusahaan/zzzz?from=portofolio&section=tesis"]}>
        <Routes>
          <Route path="/perusahaan/:ticker" element={<CompanyPage />} />
        </Routes>
      </MemoryRouter>,
    );
    expect(await screen.findByText("ZZZZ belum ada di daftar riset")).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("ZZZZ");
    expect(screen.getByRole("link", { name: "Kembali ke Portofolio" })).toHaveAttribute("href", "/portofolio");
    expect(screen.getByRole("link", { name: "Tambah ke riset" })).toHaveAttribute("href", "/perusahaan/tambah?ticker=ZZZZ&from=portofolio");
    // Tesis terbuka dan bukan tombol lipat; tidak ada blok riset lain.
    expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent)).toEqual(["Tesis"]);
    expect(screen.queryByRole("button", { name: "Tesis" })).toBeNull();
    expect(await screen.findByRole("list", { name: "Draf tesis ZZZZ" })).toHaveTextContent("Draf ZZZZ");
    expect(screen.getByRole("button", { name: "Tulis draf tesis" })).toBeInTheDocument();
  });
});

describe("quarter table labels and gap reasons (E4)", () => {
  type Pack = { latest: { fy: number; q: number }; dimensions: Array<{ metrics: Array<{ id: string; cells: Array<Record<string, unknown>> }> }> };

  it("metric labels wrap instead of clamping, the label column is the widest, and the columns fill the table (H3)", async () => {
    const { container } = renderPage();
    await screen.findByRole("heading", { level: 2, name: /Kuartal terakhir · Q2 26/ });
    const block = container.querySelector("#kuartal") as HTMLElement;
    expect(block.querySelector(".line-clamp-2")).toBeNull();
    const widths = [...block.querySelectorAll("col")].map((c) => Number(/w-\[(\d+)%\]/.exec(c.className)?.[1]));
    expect(widths.reduce((a, b) => a + b, 0)).toBe(100);
    expect(widths[0]).toBeGreaterThan(Math.max(...widths.slice(1)));
    expect(noPageOverflowProblems(container.firstElementChild as HTMLElement).filter((p) => !p.startsWith("akar layar"))).toEqual([]);
  });

  it("the table scrolls inside its own named, focusable box with the metric column pinned (design.md §3 app-wide rules)", async () => {
    const { container } = renderPage();
    await screen.findByRole("heading", { level: 2, name: /Kuartal terakhir · Q2 26/ });
    const table = container.querySelector("#kuartal table") as HTMLElement;
    const box = table.parentElement as HTMLElement;
    // 1e15 YoY values make the table wider than the 357 px column: they scroll in this box instead of running off the screen.
    expect(box).toHaveAttribute("role", "region");
    expect(box).toHaveAttribute("tabindex", "0");
    expect(box).toHaveAccessibleName(/^Kuartal terakhir Q2 26 dibanding Q2 25, per metrik paket, gulir untuk kolom lain$/);
    expect(box).toHaveClass("overflow-x-auto");
    for (const th of table.querySelectorAll('tbody th[scope="row"], thead th:first-child')) expect(th).toHaveClass("sticky", "left-0");
  });

  it("a TTM metric under the quarter columns says TTM in its label, a quarter metric does not (M3)", async () => {
    const { container } = renderPage();
    await screen.findByRole("heading", { level: 2, name: /Kuartal terakhir · Q2 26/ });
    const label = (id: string) => container.querySelector(`tr[data-metric="${id}"] th span span`)?.textContent;
    expect(label("net_margin")).toBe("Margin bersih (TTM)");
    expect(label("mining_cost_ratio")).toBe("Biaya penambangan / pendapatan (TTM)");
    expect(label("revenue")).toBe("Pendapatan");
  });

  it("a money KPI written in USD juta shows in the table's own scale with the same unit as the revenue row (N8)", async () => {
    const edited = structuredClone(pack) as unknown as Pack;
    const metric = edited.dimensions.flatMap((d) => d.metrics).find((m) => m.id === "production_mt")! as unknown as { label: string; unit: string; cells: Array<Record<string, unknown>> };
    metric.label = "Nilai kontrak tersisa (backlog)";
    metric.unit = "USD juta";
    Object.assign(metric.cells.find((c) => c.state === "entered")!, { value: 95.2, unit: "USD juta" });
    mockCompany(edited);
    const { container } = renderPage();
    await screen.findByRole("heading", { level: 2, name: /Kuartal terakhir · Q2 26/ });
    const caption = (id: string) => container.querySelector(`tr[data-metric="${id}"] th > span > span:last-child`)?.textContent;
    const scale = tableScale(lastQuarterGroups(edited as never, financials.taxonomy), financials.currency)!;
    expect(caption("production_mt")).toBe(caption("revenue"));
    expect(caption("production_mt")).toBe(unitCaption("ccy", "USD", scale));
    expect(container.querySelector(`tr[data-metric="production_mt"] td button`)?.textContent).toBe(formatScaled(95.2e6, scale));
  });

  it("a missing input reads with the metric label and Q2 26 in the title and the screen-reader text, never the element name or 2026Q2 (M2)", async () => {
    const edited = structuredClone(pack) as unknown as Pack;
    const metric = edited.dimensions.flatMap((d) => d.metrics).find((m) => m.id === "mining_cost_ratio")!;
    const cell = metric.cells.find((c) => c.fy === edited.latest.fy && c.q === edited.latest.q)!;
    Object.assign(cell, { value: null, gap: { kind: "missing", ident: "NonCurrentStripping", q: { fy: edited.latest.fy, q: edited.latest.q }, view: "instant", text: "NonCurrentStripping tidak ada untuk 2026Q2 (posisi)" } });
    mockCompany(edited);
    const { container } = renderPage();
    await screen.findByRole("heading", { level: 2, name: /Kuartal terakhir · Q2 26/ });
    const gap = container.querySelector('tr[data-metric="mining_cost_ratio"] [data-gap]') as HTMLElement;
    expect(gap.getAttribute("title")).toBe("Biaya penambangan / pendapatan: bahan perhitungannya tidak ada di laporan Q2 26");
    expect(gap.textContent).toContain("bahan perhitungannya tidak ada di laporan Q2 26");
    expect(container.querySelector("#kuartal")!.innerHTML).not.toMatch(/NonCurrentStripping|2026Q2/);
  });

  it("Data belum ada names the stand-in series by its label when it only lives under the driver's proxy (N4)", async () => {
    const edited = structuredClone(pack) as unknown as { drivers: Array<{ id: string; proxy: unknown }>; missing_drivers: Array<{ id: string; label: string; proxy: string | null }> };
    const target = edited.drivers.find((d) => d.id === "hba")!;
    const stand = edited.drivers.find((d) => d.id === "newcastle")!;
    edited.drivers = edited.drivers.filter((d) => d.id !== "newcastle");
    target.proxy = { ...stand, proxy: true, proxy_for: "hba" };
    edited.missing_drivers = [{ id: "hba", label: (target as unknown as { label: string }).label, proxy: "newcastle" }];
    mockCompany(edited);
    renderPage();
    const block = (await screen.findByRole("heading", { level: 2, name: "Data belum ada" })).closest("section") as HTMLElement;
    const standLabel = (stand as unknown as { label: string }).label;
    expect(block).toHaveTextContent(`${standLabel} dipakai sebagai pengganti`);
    expect(block).not.toHaveTextContent("newcastle");
  });
});

describe("ratio YoY and one amount scale in the quarter table (E5)", () => {
  type M = { id: string; unit: string; yoy: { kind: string; value?: number } | null };
  const metrics = (pack as unknown as { dimensions: Array<{ metrics: M[] }> }).dimensions.flatMap((d) => d.metrics);

  it("a ratio row shows the difference in times, not a percent change (N2)", async () => {
    const ratio = metrics.find((m) => m.unit === "x" && m.yoy);
    expect(ratio?.yoy?.kind).toBe("x");
    const { container } = renderPage();
    await screen.findByRole("heading", { level: 2, name: /Kuartal terakhir · Q2 26/ });
    const cells = container.querySelectorAll(`tr[data-metric="${ratio!.id}"] td`);
    const yoy = cells[cells.length - 1] as HTMLElement;
    expect(yoy.textContent).toBe(yoyCellText(ratio!.yoy as never));
    expect(yoy.textContent).not.toContain("%");
  });

  it("a metric whose latest value is an older quarter shows no YoY in the latest column", async () => {
    const stale = structuredClone(pack) as unknown as { dimensions: Array<{ metrics: Array<M & { latest: { label: string } | null }> }> };
    const m = stale.dimensions.flatMap((d) => d.metrics).find((x) => x.yoy && x.latest)!;
    m.latest!.label = "2026Q1";
    mockCompany(stale);
    const { container } = renderPage();
    await screen.findByRole("heading", { level: 2, name: /Kuartal terakhir · Q2 26/ });
    const cells = container.querySelectorAll(`tr[data-metric="${m.id}"] td`);
    const yoy = cells[cells.length - 1] as HTMLElement;
    expect(yoy.querySelector("[data-gap]")).toHaveAttribute("title", expect.stringContaining("belum ada nilai Q2 26"));
  });

  it("every money row of the table uses one scale, chosen from revenue (M4)", async () => {
    const money = new Set(metrics.filter((m) => m.unit === "ccy").map((m) => m.id));
    const { container } = renderPage();
    await screen.findByRole("heading", { level: 2, name: /Kuartal terakhir · Q2 26/ });
    const captions = [...container.querySelectorAll("#kuartal tr[data-metric]")]
      .filter((tr) => money.has(tr.getAttribute("data-metric")!))
      .map((tr) => tr.querySelector("th > span > span:last-child")!.textContent);
    expect(captions.length).toBeGreaterThan(3);
    expect(new Set(captions).size).toBe(1);
  });
});

describe("accessibility after the release-candidate QA", () => {
  it("quarter table group rows are row-group headers, not column headers (M4)", async () => {
    const { container } = renderPage();
    await screen.findByRole("heading", { level: 2, name: /Kuartal terakhir · Q2 26/ });
    const table = container.querySelector("#kuartal table") as HTMLElement;
    const groups = [...table.querySelectorAll("tbody[data-group] > tr:first-child > th")];
    expect(groups.length).toBeGreaterThan(0);
    for (const th of groups) expect(th).toHaveAttribute("scope", "rowgroup");
    expect(table.querySelector('th[scope="colgroup"]')).toBeNull();
    // The real column headers are still the four in thead.
    expect([...table.querySelectorAll("thead th")].map((th) => th.getAttribute("scope"))).toEqual(["col", "col", "col", "col"]);
  });

  it("every Angka kunci button is named from its visible label first, then the value, then where it comes from (M3, WCAG 2.5.3)", async () => {
    renderPage();
    const grid = await screen.findByTestId("angka-kunci");
    const buttons = within(grid).getAllByRole("button");
    expect(buttons.length).toBeGreaterThan(0);
    for (const b of buttons) {
      // The label span ends in a space so the visible words do not fuse with the value's (axe label-content-name-mismatch).
      const label = (b.firstElementChild?.firstElementChild?.textContent ?? "").trim();
      expect(label).not.toBe("");
      expect(b.getAttribute("aria-label")!.startsWith(`${label} `)).toBe(true);
      expect(b.getAttribute("aria-label")).toMatch(/, asal angka /);
      expect(b).toHaveAttribute("aria-expanded");
    }
  });

  it("the header pills wrap and grow instead of clipping at 200% text zoom (M7)", async () => {
    const { container } = renderPage();
    await screen.findByRole("heading", { level: 1 });
    const pills = [...container.querySelectorAll('[data-block="header"] span.uppercase')];
    expect(pills.length).toBeGreaterThan(0);
    for (const pill of pills) {
      expect(pill).toHaveClass("min-h-6", "max-w-full", "whitespace-normal");
      expect(pill).not.toHaveClass("h-6");
      expect(pill).not.toHaveClass("whitespace-nowrap");
    }
  });

  it("the KPI entry sheet is a modal dialog: the page behind is inert, Tab stays inside, and focus returns to the Isi button (H2, M1)", async () => {
    renderPage();
    const isi = (await screen.findAllByRole("button", { name: /^Isi Harga jual rata-rata Q2 26$/i }))[0];
    isi.focus();
    fireEvent.click(isi);
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog.closest("[inert]")).toBeNull();
    expect(isi.closest("[inert]")).not.toBeNull();
    const items = [...dialog.querySelectorAll<HTMLElement>("a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled])")];
    items[items.length - 1].focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(dialog.contains(document.activeElement)).toBe(true);
    fireEvent.click(within(dialog).getByRole("button", { name: "Tutup" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.querySelector("[inert]")).toBeNull();
    expect(isi).toHaveFocus();
  });
});

describe("?section= targets", () => {
  it("maps current and old section names to blocks", () => {
    expect(sectionTarget("harga")).toBe("harga");
    expect(sectionTarget("paket")).toBe("kuartal");
    expect(sectionTarget("claims")).toBe("klaim");
    expect(sectionTarget("financials")).toBe("keuangan");
    expect(sectionTarget("struktur")).toBe("struktur");
    expect(sectionTarget("tesis")).toBe("tesis");
    expect(sectionTarget("thesis")).toBe("tesis");
    expect(sectionTarget("valuasi")).toBeNull();
    expect(sectionTarget(null)).toBeNull();
  });
});
