import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { scaleTitle } from "../format";
import { FINANCIALS } from "./fixtures";
import { QuarterTable, firstColumnWidth, rowUnitText } from "./QuarterTable";
import { SUBTOTAL_IDS, quarterTableFromFinancials } from "./quarterTableData";

const data = quarterTableFromFinancials(FINANCIALS);

function renderTable(props: Partial<React.ComponentProps<typeof QuarterTable>> = {}) {
  return render(
    <div data-testid="page" className="min-w-0">
      <QuarterTable caption="Laba rugi, kuartal" title={scaleTitle("Laba rugi", data.scale)} unitHeader={data.unitHeader} periods={data.periods} groups={data.groups} {...props} />
    </div>,
  );
}

describe("quarterTableFromFinancials", () => {
  it("chooses one scale for the table and states it in the header", () => {
    expect(data.unitHeader).toBe("(RP MILIAR)");
    expect(scaleTitle("Laba rugi", data.scale)).toBe("Laba rugi · dalam miliar");
  });

  it("keeps periods in payload order (latest right) and shortens their labels", () => {
    expect(data.periods.map((p) => p.label)).toEqual(["Q3 25", "Q4 25", "Q1 26", "Q2 26"]);
    expect(data.periods.map((p) => p.derived)).toEqual([false, true, false, false]);
  });

  it("formats currency cells on the scale and per-share cells apart, with no unit inside a cell", () => {
    const revenue = data.groups[0].rows.find((r) => r.id === "revenue")!;
    expect(revenue.cells.map((c) => c.text)).toEqual(["410", "450", "430", "470"]);
    const eps = data.groups[0].rows.find((r) => r.id === "eps")!;
    expect(eps.unit).toBe("IDR/shares");
    expect(eps.cells[0].text).toBe("4,50");
    for (const cell of data.groups.flatMap((g) => g.rows).flatMap((r) => r.cells)) {
      expect(cell.text ?? "").not.toMatch(/Rp|%|×|IDR/);
    }
  });

  it("marks subtotals and uses U+2212 for negatives", () => {
    const rows = data.groups.flatMap((g) => g.rows);
    expect(rows.filter((r) => r.subtotal).map((r) => r.id)).toEqual(["net_income", "equity"]);
    expect(SUBTOTAL_IDS.has("gross_profit")).toBe(true);
    expect(rows.find((r) => r.id === "net_income")!.cells[0].text).toBe("−12");
  });
});

describe("QuarterTable", () => {
  it("default: a real table with scoped headers, periods as columns and the unit in the first header", () => {
    renderTable();
    const table = screen.getByRole("table", { name: "Laba rugi, kuartal" });
    const heads = within(table).getAllByRole("columnheader");
    expect(heads.map((h) => h.textContent)).toEqual(["(RP MILIAR)", "Q3 25", "Q4 25turunan", "Q1 26", "Q2 26"]);
    for (const h of heads) expect(h).toHaveAttribute("scope", "col");
    for (const h of heads) expect(h).toHaveClass("text-table-head", "uppercase");
    for (const row of within(table).getAllByRole("rowheader")) expect(["row", "rowgroup"]).toContain(row.getAttribute("scope"));
    expect(screen.getByRole("heading", { name: "Laba rugi · dalam miliar" })).toBeInTheDocument();
  });

  it("the card carries no padding of its own: the table is full-bleed and cells pad themselves", () => {
    renderTable();
    const card = screen.getByTestId("page").querySelector("section")!;
    expect(card.className).not.toMatch(/\bp-4\b|\bp-0\b/);
    expect(card).toHaveClass("border", "rounded-md");
  });

  it("exposes one landmark per table, not three", () => {
    renderTable();
    expect(screen.getAllByRole("region")).toHaveLength(1);
  });

  it("no cell contains a currency, percent or multiplication sign", () => {
    renderTable();
    for (const td of screen.getAllByRole("cell")) {
      const visible = Array.from(td.querySelectorAll("[aria-hidden=true]")).map((n) => n.textContent).join("");
      expect(visible).not.toMatch(/Rp|%|×|IDR|USD/);
    }
  });

  it("the first column is sticky: every row header and the corner header", () => {
    renderTable();
    const table = screen.getByRole("table");
    const corner = within(table).getAllByRole("columnheader")[0];
    expect(corner).toHaveClass("sticky", "left-0");
    const rowHeaders = within(table).getAllByRole("rowheader");
    expect(rowHeaders.length).toBe(3 + 1 + 2); // three laba rugi rows, one neraca row, two group rows
    for (const th of rowHeaders) expect(th).toHaveClass("sticky", "left-0", "bg-bg");
    const labels = within(table).getAllByRole("rowheader").filter((th) => th.getAttribute("scope") === "row");
    expect(labels.map((th) => th.textContent)).toEqual(["Pendapatan", "Laba bersih", "Laba per saham(IDR per saham)", "Total ekuitas"]);
  });

  it("the scroll container, not the page, overflows horizontally", () => {
    renderTable();
    const page = screen.getByTestId("page");
    const scroll = page.querySelector("[data-scroll-container]") as HTMLElement;
    expect(scroll).toHaveClass("overflow-x-auto", "max-w-full");
    expect(scroll).toHaveAttribute("role", "region");
    expect(scroll).toHaveAttribute("tabindex", "0"); // keyboard-scrollable
    // Focus scrolling and scroll-snap must account for the 200 px sticky column.
    expect(scroll.style.scrollPaddingLeft).toBe("200px");
    expect(scroll).toHaveAccessibleName("Laba rugi, kuartal, gulir untuk kolom lain");
    // The table is wider than its container by construction.
    expect(scroll.querySelector("table")).toHaveClass("min-w-max");
    // No ancestor between the scroll container and the page scrolls or clips horizontally.
    for (let el = scroll.parentElement; el && el !== page; el = el.parentElement) {
      expect(el.className).not.toMatch(/overflow-(x-|y-)?(auto|scroll|hidden|clip)/);
    }
    expect(page.className).not.toMatch(/overflow/);
    // The card shrinks inside flex/grid parents instead of widening the page.
    expect(scroll.closest("section")).toHaveClass("min-w-0", "max-w-full");
  });

  it("cells are FactChips with the §3.2 badges; a Q4 header carries 'turunan'", () => {
    renderTable();
    const table = screen.getByRole("table");
    expect(within(table).getAllByRole("button", { name: /^Asal angka Pendapatan/ })).toHaveLength(4);
    const q4Header = within(table).getByRole("columnheader", { name: /Q4 25/ });
    expect(within(q4Header).getByText("turunan")).toHaveAttribute("data-provenance", "turunan");
    const labaRow = within(table).getByRole("row", { name: /Pendapatan/ });
    // Badge sel ringkas (tidak melebarkan kolom angka), nama lengkapnya tetap nama aksesibel.
    expect(Array.from(labaRow.querySelectorAll("[data-provenance]")).map((b) => b.getAttribute("aria-label"))).toEqual(["turunan FY", "disajikan kembali"]);
    expect(Array.from(labaRow.querySelectorAll("[data-provenance]")).map((b) => b.textContent)).toEqual(["FY", "DK"]);
    const eps = within(table).getByRole("row", { name: /Laba per saham/ });
    expect(within(eps).getAllByRole("img").map((b) => b.getAttribute("aria-label"))).toEqual(["turunan FY", "dari pembanding"]);
  });

  it("row labels wrap and the unit sits on its own caption line; numeric cells stay on one line", () => {
    renderTable();
    const table = screen.getByRole("table");
    for (const th of within(table).getAllByRole("rowheader")) expect(th.className).not.toMatch(/whitespace-nowrap|truncate|line-clamp|text-ellipsis/);
    expect(within(table).getAllByRole("columnheader")[0]).not.toHaveClass("whitespace-nowrap");
    const eps = within(table).getAllByRole("rowheader").find((th) => th.textContent?.startsWith("Laba per saham"))!;
    // Satu baris sendiri di ponsel, sebaris dengan label di layar lebar; satuan tidak pernah dalam bahasa Inggris.
    expect(within(eps).getByText("(IDR per saham)")).toHaveClass("block");
    expect(within(eps).getByText("(IDR per saham)").className).not.toMatch(/lg:/);
    expect(within(table).queryByText(/\/shares/)).toBeNull();
    for (const cell of screen.getAllByRole("cell").filter((c) => c.hasAttribute("data-cell"))) expect(cell).toHaveClass("whitespace-nowrap");
  });

  it("a period read from the original PDF carries a PDF badge in its header, the others none", () => {
    // Periods are Q3 25, Q4 25, Q1 26, Q2 26: index 2 is Q1 26. A filing-less period (null) has no badge either.
    const pdfData = quarterTableFromFinancials({
      ...FINANCIALS,
      periods: FINANCIALS.periods.map((p, i) => ({ ...p, source_kind: i === 2 ? "pdf" : i === 0 ? null : "xbrl" })),
    });
    expect(pdfData.periods.map((p) => p.pdf)).toEqual([false, false, true, false]);
    renderTable({ periods: pdfData.periods });
    const table = screen.getByRole("table");
    const q1 = within(table).getByRole("columnheader", { name: /Q1 26/ });
    expect(within(q1).getByText("PDF")).toHaveAttribute("data-provenance", "pdf");
    expect(table.querySelectorAll('thead [data-provenance="pdf"]')).toHaveLength(1);
    expect(within(within(table).getByRole("columnheader", { name: /Q2 26/ })).queryByText("PDF")).toBeNull();
  });

  it("numbers are right-aligned with tabular-nums", () => {
    renderTable();
    const row = screen.getByRole("row", { name: /Pendapatan/ });
    for (const td of within(row).getAllByRole("cell")) expect(td).toHaveClass("text-right", "num");
  });

  it("subtotal rows are bold", () => {
    renderTable();
    expect(screen.getByRole("row", { name: /Laba bersih/ })).toHaveClass("font-bold");
    expect(screen.getByRole("row", { name: /Pendapatan/ })).not.toHaveClass("font-bold");
  });

  it("a missing value is a gap with its explanation, not a bare dash", () => {
    renderTable();
    const row = screen.getByRole("row", { name: /Laba bersih/ });
    expect(within(row).getByText("Laba bersih Q2 26: Tidak ada di laporan")).toBeInTheDocument();
  });

  it("opens the fact for a cell through onOpenFact", () => {
    const onOpenFact = vi.fn();
    renderTable({ onOpenFact });
    fireEvent.click(screen.getByRole("button", { name: /^Asal angka Pendapatan Q1 26: / }));
    expect(onOpenFact).toHaveBeenCalledTimes(1);
    expect(onOpenFact.mock.calls[0][0]).toMatchObject({ filing_id: "AAAA_2026_Q1", element: "idx-cor:Revenue" });
  });

  it("loading: a busy skeleton grid, no data table", () => {
    const { container } = renderTable({ status: "loading" });
    expect(screen.getByRole("status")).toHaveAttribute("aria-busy", "true");
    expect(container.querySelectorAll("[data-skeleton-row]").length).toBeGreaterThan(0);
    expect(container.querySelectorAll("[data-skeleton]").length).toBeGreaterThan(5);
    expect(screen.queryByRole("table")).toBeNull();
  });

  it("empty: 'Belum ada laporan'", () => {
    renderTable({ status: "empty" });
    expect(screen.getByText("Belum ada laporan")).toBeInTheDocument();
    expect(screen.queryByRole("table")).toBeNull();
  });

  it("empty also when ready without periods or rows", () => {
    renderTable({ periods: [], groups: [] });
    expect(screen.getByText("Belum ada laporan")).toBeInTheDocument();
  });

  it("error: a Card error with a retry", () => {
    const onRetry = vi.fn();
    renderTable({ status: "error", onRetry, errorMessage: "Gagal memuat laporan" });
    expect(screen.getByRole("alert")).toHaveTextContent("Gagal memuat laporan");
    fireEvent.click(screen.getByRole("button", { name: "Coba lagi" }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });
});

describe("QuarterTable fact buttons and dialogs", () => {
  it("announces a dialog by default, but not for a target that opensDialog rejects", () => {
    const [q1] = data.periods;
    const marker = data.groups[0].rows[0].cells[0].fact!;
    renderTable({ opensDialog: (t) => t !== marker });
    expect(screen.getByRole("button", { name: `Asal angka Pendapatan ${q1.label}: 410` })).not.toHaveAttribute("aria-haspopup");
    expect(screen.getByRole("button", { name: /^Asal angka Pendapatan Q2 26: / })).toHaveAttribute("aria-haspopup", "dialog");
  });

  it("without opensDialog every fact button announces its dialog", () => {
    renderTable();
    for (const b of screen.getAllByRole("button", { name: /^Asal angka / })) expect(b).toHaveAttribute("aria-haspopup", "dialog");
  });
});

describe("rowUnitText", () => {
  it("per-share and share-count units read in Indonesian", () => {
    expect(rowUnitText("USD/shares")).toBe("USD per saham");
    expect(rowUnitText("IDR/shares")).toBe("IDR per saham");
    expect(rowUnitText("Shares")).toBe("lembar");
    expect(rowUnitText("pure")).toBe("pure");
  });
});

describe("QuarterTable keyboard", () => {
  const tabStops = () => screen.getAllByRole("button").filter((b) => b.getAttribute("tabindex") === "0");

  it("is a single Tab stop for the whole table", () => {
    renderTable();
    expect(tabStops()).toHaveLength(1);
    expect(tabStops()[0]).toHaveAccessibleName(/^Asal angka Pendapatan Q3 25: /);
  });

  it("arrow keys move between cells and skip gaps", () => {
    renderTable();
    tabStops()[0].focus();
    const table = screen.getByRole("table");
    fireEvent.keyDown(table, { key: "ArrowRight" });
    expect(screen.getByRole("button", { name: /^Asal angka Pendapatan Q4 25: / })).toHaveFocus();
    expect(tabStops()).toHaveLength(1);
    expect(tabStops()[0]).toHaveAccessibleName(/^Asal angka Pendapatan Q4 25: /);
    fireEvent.keyDown(table, { key: "ArrowDown" });
    expect(screen.getByRole("button", { name: /^Asal angka Laba bersih Q4 25: / })).toHaveFocus();
    fireEvent.keyDown(table, { key: "End" });
    // Laba bersih has a gap in its last column: End lands on the last cell that has a control.
    expect(screen.getByRole("button", { name: /^Asal angka Laba bersih Q1 26: / })).toHaveFocus();
    fireEvent.keyDown(table, { key: "Home" });
    expect(screen.getByRole("button", { name: /^Asal angka Laba bersih Q3 25: / })).toHaveFocus();
    fireEvent.keyDown(table, { key: "ArrowLeft" });
    expect(screen.getByRole("button", { name: /^Asal angka Laba bersih Q3 25: / })).toHaveFocus();
  });

  it("clicking a cell makes it the Tab stop", () => {
    renderTable();
    const cell = screen.getByRole("button", { name: /^Asal angka Total ekuitas Q2 26: / });
    fireEvent.focus(cell);
    expect(tabStops()).toEqual([cell]);
  });

  it("a gap action is a control too", () => {
    const onClick = vi.fn();
    const groups = [{ id: "g", label: "Grup", rows: [{ id: "r", label: "Produksi", cells: [{ value: null, fact: null, gapAction: { label: "Isi", onClick } }] }] }];
    renderTable({ periods: [{ key: "a", label: "Q1 26" }], groups });
    const action = screen.getByRole("button", { name: "Isi Produksi Q1 26" });
    expect(action).toHaveAttribute("tabindex", "0");
    expect(action).toHaveClass("min-h-11");
    fireEvent.click(action);
    expect(onClick).toHaveBeenCalledTimes(1);
  });
});

describe("QuarterTable cells that cannot be shown as figures", () => {
  const periods = [{ key: "a", label: "Q1 26" }, { key: "b", label: "Q2 26" }];

  it("a value without a fact target is a gap with its reason, never a bare number", () => {
    const groups = [{ id: "g", label: "Grup", rows: [{ id: "r", label: "Pendapatan", cells: [{ value: 99, text: "99", fact: null }, { value: 5, text: "5", fact: FINANCIALS.groups[0].rows[0].cells[0].fact }] }] }];
    renderTable({ periods, groups });
    expect(screen.queryByText("99")).toBeNull();
    expect(screen.getByText("Pendapatan Q1 26: Asal angka tidak tersedia")).toBeInTheDocument();
  });

  it("a row with fewer cells than periods shows the missing ones as gaps", () => {
    const groups = [{ id: "g", label: "Grup", rows: [{ id: "r", label: "Pendapatan", cells: [{ value: 5, text: "5", fact: FINANCIALS.groups[0].rows[0].cells[0].fact }] }] }];
    renderTable({ periods, groups });
    expect(screen.getByText("Pendapatan Q2 26: Tidak ada di laporan")).toBeInTheDocument();
  });

  it("a real zero is a figure, not a gap", () => {
    const groups = [{ id: "g", label: "Grup", rows: [{ id: "r", label: "Kas", cells: [{ value: 0, text: "0", fact: FINANCIALS.groups[0].rows[0].cells[0].fact }] }] }];
    renderTable({ periods: [periods[0]], groups });
    expect(screen.getByRole("button", { name: /^Asal angka Kas Q1 26: / })).toBeInTheDocument();
  });
});

describe("quarterTableFromFinancials units and rounding", () => {
  const withRow = (unit: string | null, value: number) => ({
    ...FINANCIALS,
    groups: [
      ...FINANCIALS.groups,
      { id: "x", label: "Lain", rows: [{ id: "extra", label: "Tambahan", element: "idx-cor:X", unit, cells: [{ value, flags: [], fact: FINANCIALS.groups[0].rows[0].cells[0].fact, inputs: 1 }] }] },
    ],
  });

  it("a share count or another currency is outside the scale and does not move it", () => {
    for (const unit of ["Shares", "USD"]) {
      const out = quarterTableFromFinancials(withRow(unit, 3_000_000_000_000));
      expect(out.unitHeader).toBe("(RP MILIAR)");
      const row = out.groups.find((g) => g.id === "x")!.rows[0];
      expect(row.unit).toBe(unit);
      expect(row.cells[0].text).toBe("3.000.000.000.000");
    }
  });

  it("the doubled per-share unit from the backend is normalised", () => {
    const out = quarterTableFromFinancials(withRow("IDR/shares/shares", 4.5));
    expect(out.groups.find((g) => g.id === "x")!.rows[0].unit).toBe("IDR/shares");
  });

  it("a small non-zero figure is never printed as a bare 0", () => {
    const out = quarterTableFromFinancials(withRow("IDR", 400_000_000));
    const text = out.groups.find((g) => g.id === "x")!.rows[0].cells[0].text!;
    expect(text).toBe("0,4");
    expect(quarterTableFromFinancials(withRow("IDR", 0)).groups.find((g) => g.id === "x")!.rows[0].cells[0].text).toBe("0");
  });

  it("annual labels read FY and a null currency falls back to rupiah", () => {
    const out = quarterTableFromFinancials({ ...FINANCIALS, currency: null, periods: FINANCIALS.periods.map((p) => ({ ...p, label: String(p.fy) })) });
    expect(out.periods[0].label).toBe("FY 25");
    expect(out.unitHeader).toBe("(RP MILIAR)");
  });
});

describe("QuarterTable sticky column and scroll start", () => {
  // jsdom has no layout: stub the scroll window (500 px wide, table wider) and the measured widths.
  const WINDOW = 500;
  const COLUMNS = [96, 171, 96, 96];
  const original = {
    clientWidth: Object.getOwnPropertyDescriptor(Element.prototype, "clientWidth"),
    scrollWidth: Object.getOwnPropertyDescriptor(Element.prototype, "scrollWidth"),
    rect: Object.getOwnPropertyDescriptor(Element.prototype, "getBoundingClientRect"),
    range: Object.getOwnPropertyDescriptor(Range.prototype, "getBoundingClientRect"),
  };
  function fakeLayout(scrollWidth: number, label = 120) {
    const scroller = (el: Element) => el.hasAttribute("data-scroll-container");
    Object.defineProperty(Element.prototype, "clientWidth", { configurable: true, get() { return scroller(this) ? WINDOW : 0; } });
    Object.defineProperty(Element.prototype, "scrollWidth", { configurable: true, get() { return scroller(this) ? scrollWidth : 0; } });
    Object.defineProperty(Element.prototype, "getBoundingClientRect", {
      configurable: true,
      value(this: Element) {
        const periods = this.closest("tr") ? Array.from(this.closest("tr")!.children).filter((c) => !c.hasAttribute("data-first")) : [];
        return { width: this.tagName === "TH" ? COLUMNS[periods.indexOf(this)] ?? 0 : 0 } as DOMRect;
      },
    });
    Object.defineProperty(Range.prototype, "getBoundingClientRect", { configurable: true, value: () => ({ width: label }) as DOMRect });
  }
  afterEach(() => {
    const restore = (proto: object, key: string, d: PropertyDescriptor | undefined) => {
      if (d) Object.defineProperty(proto, key, d);
      else delete (proto as Record<string, unknown>)[key];
    };
    restore(Element.prototype, "clientWidth", original.clientWidth);
    restore(Element.prototype, "scrollWidth", original.scrollWidth);
    restore(Element.prototype, "getBoundingClientRect", original.rect);
    restore(Range.prototype, "getBoundingClientRect", original.range);
  });

  it("the sticky first column is opaque and keeps its right edge while scrolled", () => {
    renderTable();
    const first = within(screen.getByRole("table")).getAllByRole("columnheader")[0];
    expect(first).toHaveClass("sticky", "left-0", "bg-bg");
    expect(first.className).toMatch(/shadow-\[inset_-1px_0_0/);
    for (const row of screen.getAllByRole("rowheader")) expect(row).toHaveClass("sticky", "bg-bg");
  });

  it("snaps the scroll window to period columns that start at the sticky edge", () => {
    renderTable();
    const scroll = screen.getByTestId("page").querySelector("[data-scroll-container]") as HTMLElement;
    expect(scroll).toHaveClass("snap-x", "snap-mandatory");
    const heads = within(screen.getByRole("table")).getAllByRole("columnheader");
    for (const h of heads.slice(1)) expect(h).toHaveClass("snap-start");
    for (const cell of screen.getAllByRole("cell").filter((c) => c.hasAttribute("data-cell"))) expect(cell).toHaveClass("snap-start");
  });

  it("gives the window's remainder to the first column so the window holds whole period columns", () => {
    fakeLayout(900);
    renderTable();
    // Jendela 500 - kolom pertama 200 = 300 px; dari kanan muat 96 + 96 = 192 (171 tidak lagi muat): kolom pertama 500 - 192.
    const first = within(screen.getByRole("table")).getAllByRole("columnheader")[0];
    expect(first.style.width).toBe("308px");
    expect(first.style.minWidth).toBe("308px");
    for (const row of screen.getAllByRole("rowheader")) expect(row.style.width).toBe("308px");
    const scroll = screen.getByTestId("page").querySelector("[data-scroll-container]") as HTMLElement;
    expect(scroll.style.scrollPaddingLeft).toBe("308px");
  });

  it("a long label wraps inside a capped first column instead of taking the window", () => {
    fakeLayout(900, 330);
    renderTable();
    // Label 330 px dibatasi 45 % dari 500 = 225; sisa 275 memuat 96 + 96; kolom pertama 500 - 192.
    expect(within(screen.getByRole("table")).getAllByRole("columnheader")[0].style.width).toBe("308px");
  });

  it("refits when the table, not only the window, changes size (cached view switch, web font swap)", () => {
    const observed: Element[] = [];
    const callbacks: Array<() => void> = [];
    vi.stubGlobal("ResizeObserver", class { constructor(cb: () => void) { callbacks.push(cb); } observe(el: Element) { observed.push(el); } disconnect() {} });
    try {
      fakeLayout(900);
      renderTable();
      const scroll = screen.getByTestId("page").querySelector("[data-scroll-container]") as HTMLElement;
      expect(observed).toContain(scroll);
      expect(observed).toContain(scroll.querySelector("table"));
      // Kolom terbaru melebar tanpa jendela berubah: pengamat tabel memicu ukur ulang.
      COLUMNS[3] = 140;
      act(() => callbacks.forEach((cb) => cb()));
      expect(within(screen.getByRole("table")).getAllByRole("columnheader")[0].style.width).toBe("264px");
    } finally {
      COLUMNS[3] = 96;
      vi.unstubAllGlobals();
    }
  });

  it("the snap padding is the sticky column's real width, also for a label wider than the window share", () => {
    fakeLayout(900, 460);
    renderTable();
    const first = within(screen.getByRole("table")).getAllByRole("columnheader")[0];
    expect(first.style.width).toBe("308px");
    expect((screen.getByTestId("page").querySelector("[data-scroll-container]") as HTMLElement).style.scrollPaddingLeft).toBe("308px");
  });

  it("a cell that takes focus under the sticky column is scrolled to its edge", () => {
    fakeLayout(900);
    const offsets = { left: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetLeft"), width: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth") };
    Object.defineProperty(HTMLElement.prototype, "offsetLeft", { configurable: true, get() { return this.hasAttribute("data-cell") ? 100 : 0; } });
    Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get() { return this.hasAttribute("data-first") ? 308 : this.hasAttribute("data-cell") ? 96 : 0; } });
    try {
      renderTable();
      const scroll = screen.getByTestId("page").querySelector("[data-scroll-container]") as HTMLElement;
      scroll.scrollLeft = 300; // celah 100 - 300 = -200: sel di bawah kolom sticky (308 px)
      (scroll.querySelector('[data-cell="0:0"] button') as HTMLElement).focus();
      expect(scroll.scrollLeft).toBe(100 - 308);
    } finally {
      for (const [key, d] of [["offsetLeft", offsets.left], ["offsetWidth", offsets.width]] as const) {
        if (d) Object.defineProperty(HTMLElement.prototype, key, d);
        else delete (HTMLElement.prototype as unknown as Record<string, unknown>)[key];
      }
    }
  });

  it("adds nothing when the table fits without scrolling", () => {
    // Kolom 96 + 171 + 96 + 96 = 459; kolom pertama alami 200: 659 > 500. Jendela lebar 700 memuat semuanya.
    fakeLayout(WINDOW);
    Object.defineProperty(Element.prototype, "clientWidth", { configurable: true, get() { return this.hasAttribute("data-scroll-container") ? 700 : 0; } });
    renderTable();
    expect(within(screen.getByRole("table")).getAllByRole("columnheader")[0].style.width).toBe("");
  });
});

describe("QuarterTable header badges stay on one line", () => {
  it("the derived badge sits beside the period label, both unwrapped", () => {
    renderTable();
    const head = within(screen.getByRole("table")).getAllByRole("columnheader").find((h) => h.textContent === "Q4 25turunan")!;
    const wrap = head.firstElementChild!;
    expect(wrap).toHaveClass("inline-flex", "whitespace-nowrap");
    expect(wrap).not.toHaveClass("flex-col");
    expect(head).toHaveClass("whitespace-nowrap");
    expect(within(head as HTMLElement).getByText("turunan")).toHaveClass("whitespace-nowrap", "uppercase");
  });
});

describe("firstColumnWidth (sticky column width rule)", () => {
  // iPhone 15: jendela gulir 355 px di dalam kartu. Kolom periode selebar isinya, terlama ke terbaru.
  const PHONE = 355;

  it("null when the natural first column and every period fit", () => {
    expect(firstColumnWidth(700, 150, [96, 96, 96, 96])).toBeNull();
    expect(firstColumnWidth(0, 150, [96])).toBeNull();
  });

  it("phone: a long label is capped near 45 % of the window so two period columns are visible", () => {
    const w = firstColumnWidth(PHONE, 259, Array(12).fill(96))!;
    expect(w).toBe(PHONE - 2 * 96);
    expect(w).toBeLessThanOrEqual(Math.ceil(PHONE * 0.46));
    expect(w).toBeGreaterThanOrEqual(120);
  });

  it("phone: wide number columns squeeze the first column down to 120 px, never below, to keep two whole periods", () => {
    // 449.094.722 dalam ribu: kolom sekitar 112 px. 45 % (159) menyisakan 196: satu kolom; dengan 120 dua muat.
    expect(firstColumnWidth(PHONE, 259, Array(12).fill(112))).toBe(PHONE - 224);
    // Dua kolom tidak muat bahkan dengan 120: satu kolom utuh, kolom pertama mengambil sisanya.
    expect(firstColumnWidth(PHONE, 259, Array(12).fill(150))).toBe(PHONE - 150);
  });

  it("the visible period columns are never narrower than their content: the window ends on a column edge", () => {
    for (const cols of [Array(12).fill(96), [96, 171, 96, 96], Array(8).fill(130), [120, 104, 98, 133, 101]]) {
      for (const width of [320, 355, 393, 500, 768]) {
        const w = firstColumnWidth(width, 240, cols);
        if (w === null) continue;
        let room = width - w;
        let i = cols.length - 1;
        while (i >= 0 && room >= cols[i] - 0.001) room -= cols[i--];
        // Sisa jendela tepat nol: kolom terakhir yang terlihat utuh sampai tepi kanan.
        expect(room).toBe(0);
        expect(w).toBeGreaterThanOrEqual(120);
      }
    }
  });

  it("the latest column alone wider than the window minus 120: the first column narrows below 120 so it is never clipped", () => {
    // 120 + 300 > 355 memotong angka rata kanan; kolom pertama mengambil sisanya saja.
    expect(firstColumnWidth(PHONE, 259, [96, 300])).toBe(PHONE - 300);
    expect(PHONE - firstColumnWidth(PHONE, 259, [96, 300])!).toBeGreaterThanOrEqual(300);
    // Kolom terbaru lebih lebar dari jendela: kolom pertama 0, tidak pernah negatif.
    expect(firstColumnWidth(PHONE, 259, [96, 400])).toBe(0);
  });

  it("wide windows keep the old rule: natural label width (min 200), remainder to the first column", () => {
    // 1000 px, label 260: 260 + 7 × 100 = 960 ≤ 1000; kolom pertama 1000 - 700.
    expect(firstColumnWidth(1000, 260, Array(12).fill(100))).toBe(300);
    expect(firstColumnWidth(1000, 120, Array(12).fill(100))).toBe(200);
  });
});
