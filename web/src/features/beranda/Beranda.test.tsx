import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import golden from "../../../../tests/fixtures/macro/golden/observations_isolated_false.json";
import desk from "../../../../tests/fixtures/research/canon/desk.json";
import { json, mockApi } from "../../test/mockApi";
import { MACRO, manyObservations } from "../research/desk/macro.testutil";
import { expectNoVerdictWords } from "../research/screenAsserts.testutil";
import { setViewport } from "../research/testing/viewport";
import type { DeskResponse } from "../research/types";
import DefaultBeranda, { Beranda } from "./Beranda";
import { MACRO_PREVIEW } from "./MacroGrid";

const DESK = desk as unknown as DeskResponse;

function Probe() {
  const { pathname, search } = useLocation();
  return <p data-testid="at">{pathname + search}</p>;
}

function renderBeranda() {
  return render(
    <MemoryRouter initialEntries={["/beranda"]}>
      <Routes>
        <Route path="/beranda" element={<Beranda />} />
        <Route path="*" element={<Probe />} />
      </Routes>
    </MemoryRouter>,
  );
}

const block = (name: string) => screen.getByRole("region", { name });
const calls = (f: ReturnType<typeof mockApi>, prefix: string) => f.mock.calls.filter(([u]) => String(u).startsWith(prefix)).length;

beforeEach(() => setViewport(393));
afterEach(() => setViewport(1280));

describe("Beranda #/beranda (design.md §3.1)", () => {
  it("is the named and default export", () => {
    expect(DefaultBeranda).toBe(Beranda);
  });

  it("blocks in order: header with Muat ulang, Perlu perhatian, Kalender, Makro, footer; no company list, no Tambah, no Lainnya", async () => {
    mockApi({ "/api/research/desk": DESK, "/api/macro/observations": MACRO });
    renderBeranda();
    await screen.findByRole("group", { name: "Kotak masuk" });
    expect(screen.getByRole("heading", { level: 1, name: "Beranda" })).toBeInTheDocument();
    const reload = screen.getByRole("button", { name: "Muat ulang" });
    expect(reload).toHaveClass("h-11", "w-11");
    expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent)).toEqual(["Perlu perhatian", "Kalender", "Makro"]);
    expect(screen.getByRole("contentinfo")).toHaveTextContent("Bukan saran investasi");
    expect(screen.getByRole("contentinfo")).toHaveTextContent(/Umur data · Harga: terbaru 1 hari/);
    expect(screen.queryByText(/Perusahaan saya|Lainnya|Dipegang/)).toBeNull();
    expect(screen.queryByRole("link", { name: /Tambah/ })).toBeNull();
    expect(document.querySelector("[data-company]")).toBeNull();
  });

  it("no block is collapsed: Kalender and Makro open by default", async () => {
    mockApi({ "/api/research/desk": DESK, "/api/macro/observations": MACRO });
    renderBeranda();
    await screen.findByRole("group", { name: "Kotak masuk" });
    expect(screen.queryByRole("button", { name: "Kalender" })).toBeNull();
    expect(within(block("Kalender")).getAllByRole("listitem").length).toBeGreaterThan(0);
    expect(await within(block("Makro")).findByRole("list", { name: /Indikator makro/ })).toBeInTheDocument();
  });
});

describe("Perlu perhatian (inbox)", () => {
  beforeEach(() => {
    mockApi({ "/api/research/desk": DESK, "/api/macro/observations": MACRO });
  });

  it("shows the first five 2-line rows with Lihat semua, and filter chips in one scrolling row", async () => {
    renderBeranda();
    const region = await screen.findByRole("group", { name: "Kotak masuk" });
    const items = within(region).getAllByRole("listitem");
    expect(items).toHaveLength(5);
    expect(items[0].querySelector('[data-line="1"]')).toHaveTextContent(/COAL\s*Level\s*hari ini/);
    // C8: a row may take two lines (line 2 clamps, it does not truncate) and keeps its 60 px height.
    expect(items[0].querySelector('[data-line="2"]')).toHaveClass("line-clamp-2");
    expect(items[0].querySelector('[data-line="2"]')).not.toHaveClass("truncate");
    for (const li of items) expect(li).toHaveClass("min-h-[60px]");
    fireEvent.click(within(block("Perlu perhatian")).getByRole("button", { name: `Lihat semua (${DESK.inbox.length})` }));
    expect(within(region).getAllByRole("listitem")).toHaveLength(DESK.inbox.length);
    const chips = screen.getByRole("group", { name: "Filter kotak masuk" });
    // C8: the chip row keeps the 18 px gutter at rest (`px`) and when snapping (`scroll-px`).
    expect(chips).toHaveClass("hscroll", "px-[var(--gutter-left)]", "scroll-px-[var(--gutter-left)]");
    expect(within(chips).getAllByRole("button").map((b) => b.textContent)).toEqual(DESK.filters.map((f) => `${f.label} ${f.count}`));
    fireEvent.click(within(chips).getByRole("button", { name: /^Klaim/ }));
    expect(within(region).getAllByRole("listitem")).toHaveLength(1);
  });

  it("tapping a row opens its sheet; closing returns focus to the row", async () => {
    renderBeranda();
    const region = await screen.findByRole("group", { name: "Kotak masuk" });
    const row = within(region).getAllByRole("button")[0];
    expect(row).toHaveAttribute("aria-haspopup", "dialog");
    row.focus();
    fireEvent.click(row);
    const dialog = await screen.findByRole("dialog", { name: "COAL · Level" });
    expect(within(dialog).getByText(DESK.inbox[0].text!)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Buka halaman perusahaan" })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Abaikan" })).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(row).toHaveFocus();
  });

  it("Buka halaman perusahaan pushes the company page at the item's section, owned by Beranda (from=beranda)", async () => {
    renderBeranda();
    const region = await screen.findByRole("group", { name: "Kotak masuk" });
    fireEvent.click(within(region).getAllByRole("button")[0]);
    fireEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Buka halaman perusahaan" }));
    expect(await screen.findByTestId("at")).toHaveTextContent(/^\/perusahaan\/COAL\?section=price&from=beranda$/);
  });

  it("an item without a ticker cannot open a company page", async () => {
    const event = { ...DESK.inbox[0], id: "e-all", ticker: null, section: null };
    mockApi({ "/api/research/desk": { ...DESK, inbox: [event] }, "/api/macro/observations": MACRO });
    renderBeranda();
    fireEvent.click(within(await screen.findByRole("group", { name: "Kotak masuk" })).getByRole("button", { name: /Semua emiten/ }));
    expect(within(await screen.findByRole("dialog")).getByRole("button", { name: "Buka halaman perusahaan" })).toHaveAttribute("aria-disabled", "true");
  });

  it("Abaikan dismisses the item on the server, closes the sheet and drops the row", async () => {
    const first = DESK.inbox[0];
    const f = mockApi({
      "/api/research/desk": DESK,
      "/api/macro/observations": MACRO,
      [`/api/research/events/${first.id}/dismiss`]: () => json({ ...first, dismissed_at: "2026-10-28T04:00:00+00:00" }),
    });
    renderBeranda();
    const region = await screen.findByRole("group", { name: "Kotak masuk" });
    fireEvent.click(within(region).getAllByRole("button")[0]);
    await act(async () => {
      fireEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Abaikan" }));
    });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(region.querySelector(`[data-event-id="${first.id}"]`)).toBeNull();
    const post = f.mock.calls.find(([u]) => String(u).includes("/dismiss"))!;
    expect(post[1]).toMatchObject({ method: "POST" });
  });

  it("empty: Tidak ada yang perlu perhatian", async () => {
    mockApi({ "/api/research/desk": { ...DESK, inbox: [] }, "/api/macro/observations": MACRO });
    renderBeranda();
    expect(await within(await screen.findByRole("group", { name: "Kotak masuk" })).findByText(/^Tidak ada yang perlu perhatian/)).toBeInTheDocument();
  });
});

describe("Kalender", () => {
  it("next five as 2-line rows; Lihat semua shows the whole 90 days", async () => {
    mockApi({ "/api/research/desk": DESK, "/api/macro/observations": MACRO });
    renderBeranda();
    await screen.findByRole("group", { name: "Kotak masuk" });
    const cal = block("Kalender");
    const items = within(cal).getAllByRole("listitem");
    expect(items).toHaveLength(5);
    expect(items[0].querySelector('[data-line="1"]')).toHaveTextContent("BANK");
    expect(items[0].querySelector('[data-line="2"]')).toHaveTextContent("Batas laporan 2026Q3");
    const more = within(cal).getByRole("button", { name: `Lihat semua (${DESK.calendar.length})` });
    fireEvent.click(more);
    expect(within(cal).getAllByRole("listitem")).toHaveLength(DESK.calendar.length);
    expect(within(cal).queryByRole("button", { name: /Lihat semua/ })).toBeNull();
  });

  it("each row links to the company page, owned by Beranda (from=beranda), as a target of at least 44 px (design.md §2.4 rule 4)", async () => {
    mockApi({ "/api/research/desk": DESK, "/api/macro/observations": MACRO });
    renderBeranda();
    await screen.findByRole("group", { name: "Kotak masuk" });
    const items = within(block("Kalender")).getAllByRole("listitem");
    for (const [i, li] of items.entries()) {
      const link = within(li).getByRole("link");
      expect(link).toHaveAttribute("href", `/perusahaan/${DESK.calendar[i].ticker}?from=beranda`);
      expect(link).toHaveClass("min-h-[60px]", "w-full");
      expect(li).toHaveClass("min-h-[60px]");
      expect(li).not.toHaveClass("h-[60px]");
    }
    expect(within(items[0]).getByRole("link")).toHaveAccessibleName(/BANK/);
    fireEvent.click(within(items[0]).getByRole("link"));
    expect(await screen.findByTestId("at")).toHaveTextContent(`/perusahaan/${DESK.calendar[0].ticker}?from=beranda`);
  });

  it("up to five items: no Lihat semua", async () => {
    mockApi({ "/api/research/desk": { ...DESK, calendar: DESK.calendar.slice(0, 2) }, "/api/macro/observations": MACRO });
    renderBeranda();
    await screen.findByRole("group", { name: "Kotak masuk" });
    expect(within(block("Kalender")).getAllByRole("listitem")).toHaveLength(2);
    expect(within(block("Kalender")).queryByRole("button")).toBeNull();
  });

  it("empty: Tidak ada jadwal 90 hari ke depan", async () => {
    mockApi({ "/api/research/desk": { ...DESK, calendar: [] }, "/api/macro/observations": MACRO });
    renderBeranda();
    await screen.findByRole("group", { name: "Kotak masuk" });
    expect(within(block("Kalender")).getByText(/^Tidak ada jadwal 90 hari ke depan/)).toBeInTheDocument();
  });
});

describe("Makro (MacroGrid)", () => {
  it("a 2-column grid of 2-line cards: name, then value with unit and signed change", async () => {
    const f = mockApi({ "/api/research/desk": DESK, "/api/macro/observations": MACRO });
    renderBeranda();
    const list = await within(block("Makro")).findByRole("list", { name: /Indikator makro/ });
    expect(list).toHaveClass("grid", "grid-cols-2");
    const card = (name: string) => within(list).getByRole("button", { name: new RegExp(name) });
    const oil = card("Minyak mentah");
    expect(oil).toHaveClass("min-h-11", "w-full");
    const [one, two] = [oil.querySelector('[data-line="1"]')!, oil.querySelector('[data-line="2"]')!];
    expect(one).toHaveTextContent("Minyak mentah kontrak terdekat");
    expect(two).toHaveTextContent("80,5");
    expect(two).toHaveTextContent("USD/barel");
    expect(two).toHaveTextContent("+1,85");
    // F6: line 2 never wraps onto a third line; the unit gives way (ellipsis), the number and the change do not.
    expect(two).not.toHaveClass("flex-wrap");
    expect(two.querySelector('[aria-hidden="true"].truncate')).toHaveTextContent("USD/barel");
    expect(oil.querySelector('[data-line="3"]')).toBeNull();
    expect(card("Suku bunga acuan").querySelector('[data-line="2"]')).toHaveTextContent(/^4,75.*%.*−0,25/);
    expect(card("Kurs rupiah")).not.toHaveTextContent("Rp");
    expect(card("Inflasi inti")).toHaveTextContent("belum ada");
    expect(list).not.toHaveTextContent("1970");
    expect(within(block("Makro")).getByText("Data makro per 20 Okt 2026")).toBeInTheDocument();
    expect(calls(f, "/api/macro/")).toBe(1);
    expect(f.mock.calls.every(([u]) => !/\/api\/macro\/(stream|trigger|scoring)/.test(String(u)))).toBe(true);
  });

  it("a card announces name, value, unit in words and the change in words", async () => {
    mockApi({ "/api/research/desk": DESK, "/api/macro/observations": MACRO });
    renderBeranda();
    expect(await screen.findByRole("button", { name: /Minyak mentah/ })).toHaveAccessibleName("Minyak mentah kontrak terdekat 80,5 dolar AS per barel selisih plus 1,85");
    expect(screen.getByRole("button", { name: /Inflasi inti/ })).toHaveAccessibleName(expect.stringMatching(/belum ada\s+selisih tidak tersedia$/));
  });

  it("tapping a card opens a sheet with the trend; closing returns focus", async () => {
    mockApi({ "/api/research/desk": DESK, "/api/macro/observations": MACRO });
    renderBeranda();
    const card = await screen.findByRole("button", { name: /Minyak mentah/ });
    expect(card).toHaveAttribute("aria-haspopup", "dialog");
    card.focus();
    fireEvent.click(card);
    const dialog = await screen.findByRole("dialog", { name: "Minyak mentah kontrak terdekat" });
    expect(within(dialog).getByText("Energi · dolar AS per barel · baru")).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(card).toHaveFocus();
  });

  it("is neutral: no score, regime, lens or up/down colour", async () => {
    mockApi({ "/api/research/desk": DESK, "/api/macro/observations": MACRO });
    renderBeranda();
    await screen.findByRole("list", { name: /Indikator makro/ });
    const makro = block("Makro");
    expect(makro.innerHTML).not.toMatch(/text-status-|bg-status-/);
    expect(makro).not.toHaveTextContent(/skor|regime|rezim|lensa/i);
    expectNoVerdictWords(makro, [DESK]);
  });

  it("the key indicators come first; the rest behind Tampilkan semua indikator (real 15-row payload)", async () => {
    mockApi({ "/api/research/desk": DESK, "/api/macro/observations": { as_of: "2026-10-20T09:21:52Z", observations: golden } });
    renderBeranda();
    const list = await screen.findByRole("list", { name: /Indikator makro/ });
    expect(within(list).getAllByRole("listitem")).toHaveLength(MACRO_PREVIEW);
    expect(within(list).getAllByRole("button")[0].querySelector('[data-line="1"]')).toHaveTextContent("Brent Crude Front-Month");
    const toggle = within(block("Makro")).getByRole("button", { name: "Tampilkan semua indikator (15)" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(toggle);
    expect(within(list).getAllByRole("listitem")).toHaveLength(15);
  });

  it("up to the preview size there is no toggle", async () => {
    mockApi({ "/api/research/desk": DESK, "/api/macro/observations": manyObservations(MACRO_PREVIEW) });
    renderBeranda();
    await screen.findByRole("list", { name: /Indikator makro/ });
    expect(within(block("Makro")).queryByRole("button", { name: /^Tampilkan/ })).toBeNull();
  });

  it("empty: Belum ada data makro with Muat ulang, which loads the observations again", async () => {
    const f = mockApi({ "/api/research/desk": DESK, "/api/macro/observations": { as_of: MACRO.as_of, observations: [] } });
    renderBeranda();
    const makro = await screen.findByRole("region", { name: "Makro" });
    expect(await within(makro).findByText("Belum ada data makro")).toBeInTheDocument();
    expect(within(makro).queryByRole("list")).toBeNull();
    await act(async () => {
      fireEvent.click(within(makro).getByRole("button", { name: "Muat ulang" }));
    });
    expect(calls(f, "/api/macro/observations")).toBe(2);
  });

  it("loading: a busy status in the block; the rest of Beranda stays", async () => {
    mockApi({ "/api/research/desk": DESK, "/api/macro/observations": () => new Promise(() => {}) as unknown as Response });
    renderBeranda();
    await screen.findByRole("group", { name: "Kotak masuk" });
    expect(within(block("Makro")).getByText("Memuat data makro…").closest("[role=status]")).toHaveAttribute("aria-busy", "true");
  });

  it("error: the error state with Coba lagi and no data behind it; the inbox is unaffected", async () => {
    let fail = true;
    const f = mockApi({ "/api/research/desk": DESK, "/api/macro/observations": () => (fail ? json({ detail: "boom" }, 500) : json(MACRO)) });
    renderBeranda();
    const makro = await screen.findByRole("region", { name: "Makro" });
    expect(await within(makro).findByText("Gagal memuat data makro")).toBeInTheDocument();
    expect(within(makro).queryByRole("list")).toBeNull();
    expect(screen.getByRole("group", { name: "Kotak masuk" })).toBeInTheDocument();
    fail = false;
    fireEvent.click(within(makro).getByRole("button", { name: "Coba lagi" }));
    expect(await within(makro).findByRole("list", { name: /Indikator makro/ })).toBeInTheDocument();
    expect(calls(f, "/api/macro/observations")).toBe(2);
  });
});

describe("Muat ulang and the desk's own states", () => {
  it("the header Muat ulang loads the desk and the macro observations again", async () => {
    const f = mockApi({ "/api/research/desk": DESK, "/api/macro/observations": MACRO });
    renderBeranda();
    await screen.findByRole("list", { name: /Indikator makro/ });
    expect([calls(f, "/api/research/desk"), calls(f, "/api/macro/observations")]).toEqual([1, 1]);
    await act(async () => {
      fireEvent.click(screen.getAllByRole("button", { name: "Muat ulang" })[0]);
      await new Promise((r) => setTimeout(r, 20));
    });
    expect([calls(f, "/api/research/desk"), calls(f, "/api/macro/observations")]).toEqual([2, 2]);
  });

  it("desk loading: a busy status; header, Makro and footer still render", async () => {
    mockApi({ "/api/research/desk": () => new Promise(() => {}) as unknown as Response, "/api/macro/observations": MACRO });
    renderBeranda();
    expect(screen.getByText("Memuat Beranda…").closest("[role=status]")).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("heading", { level: 1, name: "Beranda" })).toBeInTheDocument();
    expect(await screen.findByRole("list", { name: /Indikator makro/ })).toBeInTheDocument();
    expect(screen.getByText("Bukan saran investasi")).toBeInTheDocument();
  });

  it("desk error: Gagal memuat Beranda with Coba lagi; Makro still shows", async () => {
    let fail = true;
    mockApi({ "/api/research/desk": () => (fail ? json({ detail: "boom" }, 500) : json(DESK)), "/api/macro/observations": MACRO });
    renderBeranda();
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Gagal memuat Beranda");
    expect(await screen.findByRole("list", { name: /Indikator makro/ })).toBeInTheDocument();
    fail = false;
    fireEvent.click(within(alert).getByRole("button", { name: "Coba lagi" }));
    expect(await screen.findByRole("group", { name: "Kotak masuk" })).toBeInTheDocument();
  });
});
