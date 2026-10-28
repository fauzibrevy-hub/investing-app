import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
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
  summary: { current_valuation: 1_100_000, total_cost: 900_000, total_pl: 100_000, total_pl_pct: 11.11, cash_balance: 100_000, updated_at: "2026-10-27T09:30:00" },
  positions: [pos("SOCI", 400_000, 300_000, 33.3, 40, 15), pos("BUMI", 600_000, 700_000, -14.3, 60, 30)],
  available: true,
};

const STORAGE_KEY = "investing.hideAmounts";

function Where() {
  const { pathname, search } = useLocation();
  return <p data-testid="where">{`${pathname}${search}`}</p>;
}

const renderAt = (provider: { initial?: boolean } = { initial: false }) =>
  render(
    <HideAmountsProvider {...provider}>
      <MemoryRouter initialEntries={["/portofolio"]}>
        <Routes>
          <Route path="/portofolio" element={<PortfolioScreen />} />
          <Route path="/perusahaan/:ticker" element={<Where />} />
        </Routes>
      </MemoryRouter>
    </HideAmountsProvider>,
  );

const eye = () => screen.getByRole("button", { name: /nominal$/ });
/** Teks layar yang menampilkan angka rupiah (`Rp` diikuti angka), bukan `Rp•••`. */
const visibleRupiah = () => (document.body.textContent ?? "").match(/Rp\s?[\d.,]+/g) ?? [];
const positionRow = (ticker: string) => screen.getAllByRole("button").find((b) => new RegExp(`^${ticker}\\s*1 lot`).test(b.textContent ?? ""));

beforeEach(() => {
  window.localStorage.clear();
  mockApi({ "/api/finance/stock-portfolio": data });
});
afterEach(() => vi.unstubAllGlobals());

describe("Portofolio: tata letak", () => {
  it("judul layar, tanpa kontrol segmen Investasi, dan footer", async () => {
    renderAt();
    expect(await screen.findByRole("heading", { level: 1, name: "Portofolio" })).toBeInTheDocument();
    expect(screen.queryByRole("tablist")).toBeNull();
    expect(screen.queryByText("Investasi")).toBeNull();
    expect(await screen.findAllByText("Harga per 2026-10-27 09:30")).toHaveLength(2); // di bawah judul dan di footer
    expect(screen.getByText("Bukan saran investasi")).toBeInTheDocument();
  });

  it("hero: nilai portofolio (termasuk kas) tanpa Rp, satu baris P/L bertanda yang satu-satunya Rp di blok", async () => {
    renderAt();
    const hero = await screen.findByRole("region", { name: "Nilai portofolio" });
    expect(await within(hero).findByText("1.100.000")).toBeInTheDocument();
    expect(within(hero).getByText("Nilai portofolio (Rp), termasuk kas")).toBeInTheDocument();
    const line = within(hero).getByText("+Rp100 rb · +11,1%");
    expect(line.closest("p")).toHaveClass("text-status-safe");
    expect(hero.textContent?.match(/Rp/g)).toHaveLength(2); // satuan di label, nominal di baris P/L
  });

  it("hero rugi: baris P/L bertanda minus berwarna status-over", async () => {
    mockApi({ "/api/finance/stock-portfolio": { ...data, summary: { ...data.summary, total_pl: -250_000, total_pl_pct: -4.1 } } });
    renderAt();
    const hero = await screen.findByRole("region", { name: "Nilai portofolio" });
    expect((await within(hero).findByText("−Rp250 rb · −4,1%")).closest("p")).toHaveClass("text-status-over");
  });

  it("Ringkasan: tiga kartu dua baris (label, nilai) dengan satuan di judul; Nilai pasar = Σ valuation", async () => {
    renderAt();
    const group = await screen.findByRole("group", { name: "Ringkasan portofolio" });
    await screen.findByText("1.100.000");
    expect(screen.getByRole("heading", { name: "Ringkasan (Rp)" })).toBeInTheDocument();
    const cards = Array.from(group.children) as HTMLElement[];
    expect(cards).toHaveLength(3);
    expect(cards.map((c) => Array.from(c.querySelectorAll("span")).map((s) => s.textContent))).toEqual([
      ["Modal", "900 rb"],
      ["Nilai pasar", "1 jt"],
      ["Kas", "100 rb"],
    ]);
    expect(group.textContent).not.toContain("Rp");
  });

  it("Komposisi: donut dan legenda urut nilai pasar, satu warna berbeda per emiten", async () => {
    mockApi({
      "/api/finance/stock-portfolio": {
        ...data,
        positions: ["AAAA", "BBBB", "CCCC", "DDDD", "EEEE", "FFFF"].map((t, i) => pos(t, 600_000 - i * 80_000, 300_000, 5, 20, 20)),
      },
    });
    renderAt();
    const legend = await screen.findByRole("list", { name: "Legenda komposisi" });
    const items = within(legend).getAllByRole("listitem");
    expect(items.map((li) => li.textContent?.slice(0, 4))).toEqual(["AAAA", "BBBB", "CCCC", "DDDD", "EEEE", "FFFF"]);
    const swatches = items.map((li) => (li.querySelector("[data-swatch]") as HTMLElement).dataset.swatch);
    expect(new Set(swatches).size).toBe(6);
    expect(screen.getByText("6 emiten")).toBeInTheDocument();
  });

  it("donut hanya visual (inert + aria-hidden, tanpa tombol busur); legenda memegang aksi pilih ≥ 44 px dan menyorot irisan", async () => {
    renderAt();
    const legend = await screen.findByRole("list", { name: "Legenda komposisi" });
    const arcs = Array.from(document.querySelectorAll("[data-segment]"));
    expect(arcs).toHaveLength(2);
    for (const arc of arcs) expect(arc.closest("[inert][aria-hidden='true']")).not.toBeNull();
    expect(screen.queryByRole("button", { name: /^BUMI, / })).toBeNull();

    const rows = within(legend).getAllByRole("button");
    expect(rows.map((r) => r.textContent?.slice(0, 4))).toEqual(["BUMI", "SOCI"]);
    for (const r of rows) {
      expect(r.className).toContain("min-h-11");
      expect(r).toHaveAttribute("aria-pressed", "false");
    }
    fireEvent.click(rows[1]);
    expect(rows[1]).toHaveAttribute("aria-pressed", "true");
    expect(rows[0]).toHaveAttribute("aria-pressed", "false");
    expect(document.querySelector("[data-center-line='0']")).toHaveTextContent("SOCI");
    expect((document.querySelector("[data-arc='0']") as SVGElement).getAttribute("opacity")).toBe("0.35");
    fireEvent.click(rows[1]);
    expect(rows[1]).toHaveAttribute("aria-pressed", "false");
    expect(document.querySelector("[data-center-line='0']")).toHaveTextContent("Nilai pasar");
  });

  it("Posisi: baris urut nilai, subjudul lot · avg, P/L % bertanda; tombol dengan pitch 57 px", async () => {
    renderAt();
    await screen.findByText("1.100.000");
    const rows = screen.getAllByRole("button").filter((b) => /lot · avg/.test(b.textContent ?? ""));
    expect(rows.map((r) => r.textContent?.slice(0, 4))).toEqual(["BUMI", "SOCI"]);
    expect(rows[0]).toHaveTextContent("1 lot · avg 4.150");
    expect(screen.getByRole("heading", { name: "Posisi (Rp)" })).toBeInTheDocument(); // satuan di judul blok
    expect(within(rows[0]).getByText("600.000")).toBeInTheDocument(); // nilai pasar tanpa Rp
    expect(rows.map((r) => r.textContent).join("")).not.toContain("Rp"); // tidak ada Rp berulang di baris
    expect(rows[0].className).toContain("min-h-[57px]");
    expect(within(rows[0]).getByText("−14,3%")).toHaveClass("text-status-over");
  });

  it("Posisi: subjudul lot · avg boleh turun baris tetapi tidak pernah dipotong (tanpa truncate), swatch tanpa margin lebar", async () => {
    renderAt();
    await screen.findByText("1.100.000");
    const row = positionRow("BUMI") as HTMLElement;
    const subtitle = within(row).getByText("1 lot · avg 4.150");
    expect(subtitle.className).not.toMatch(/truncate|text-ellipsis|whitespace-nowrap/);
    expect(row.querySelector("[data-swatch]")?.className).not.toMatch(/\bmx-/);
  });

  it("hanya membaca portofolio Sheets: satu permintaan, tanpa tautan rute lama", async () => {
    const fetchMock = mockApi({ "/api/finance/stock-portfolio": data });
    renderAt();
    await screen.findByText("1.100.000");
    fireEvent.click(positionRow("BUMI") as HTMLElement);
    await screen.findByRole("dialog", { name: "BUMI" });
    expect(fetchMock.mock.calls.map(([input]) => String(input))).toEqual(["/api/finance/stock-portfolio"]);
    expect(Array.from(document.querySelectorAll("a")).map((a) => a.getAttribute("href"))).toEqual(["/perusahaan/BUMI?from=portofolio"]);
  });

  it("Alokasi vs target: drift per emiten", async () => {
    renderAt();
    expect(await screen.findByText("+30 pp")).toBeInTheDocument(); // BUMI 60 vs 30
    expect(screen.getByText("+25 pp")).toBeInTheDocument(); // SOCI 40 vs 15
    expect(screen.getByText("60% / target 30%")).toBeInTheDocument(); // bobot dan target di keterangan yang bisa turun baris
  });

  it("tab PORTOFOLIO kosong → empty state, gagal → Coba lagi", async () => {
    mockApi({ "/api/finance/stock-portfolio": { summary: {}, positions: [], available: false } });
    const first = renderAt();
    expect((await screen.findAllByText("Belum ada posisi di tab PORTOFOLIO")).length).toBeGreaterThan(0);
    first.unmount();
    mockApi({ "/api/finance/stock-portfolio": () => json({ detail: "x" }, 502) });
    renderAt();
    expect((await screen.findAllByRole("button", { name: "Coba lagi" })).length).toBeGreaterThan(0);
  });
});

describe("Portofolio: tombol mata (sembunyikan nominal)", () => {
  it("tombol di header memakai aria-pressed dan nama yang mengikuti keadaan", async () => {
    renderAt();
    await screen.findByText("1.100.000");
    expect(eye()).toHaveAccessibleName("Sembunyikan nominal");
    expect(eye()).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(eye());
    expect(eye()).toHaveAccessibleName("Tampilkan nominal");
    expect(eye()).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(eye());
    expect(eye()).toHaveAccessibleName("Sembunyikan nominal");
    expect(eye()).toHaveAttribute("aria-pressed", "false");
  });

  it("menyembunyikan setiap nominal: Rp•••, tanpa satu pun angka rupiah; persen tetap tampil", async () => {
    renderAt();
    await screen.findByText("1.100.000");
    expect(visibleRupiah().length).toBeGreaterThan(0);
    fireEvent.click(eye());
    expect(visibleRupiah()).toEqual([]);
    expect(document.body.textContent).not.toContain("1.100.000");
    const hero = screen.getByRole("region", { name: "Nilai portofolio" });
    expect(hero).toHaveTextContent("•••");
    expect(within(hero).getByText("Rp••• · +11,1%")).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Ringkasan portofolio" }).textContent).toBe("Modal•••Nilai pasar•••Kas•••");
    const row = positionRow("BUMI");
    expect(within(row as HTMLElement).getByText("•••", { selector: "span.num" })).toBeInTheDocument(); // nilai pasar posisi, tanpa Rp
    expect(row?.textContent).not.toContain("Rp");
    expect(row).toHaveTextContent("1 lot · avg •••");
    expect(row).toHaveTextContent("−14,3%");
  });

  it("pilihan tersimpan di localStorage dan dipakai saat layar dibuka lagi", async () => {
    const first = renderAt();
    await screen.findByText("1.100.000");
    fireEvent.click(eye());
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe("1");
    first.unmount();

    renderAt({});
    await screen.findByRole("region", { name: "Nilai portofolio" });
    expect(eye()).toHaveAttribute("aria-pressed", "true");
    await screen.findByText("Bukan saran investasi");
    expect(visibleRupiah()).toEqual([]);
    fireEvent.click(eye());
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe("0");
  });
});

describe("Portofolio: Sheet posisi", () => {
  const openBumi = async () => {
    renderAt();
    await screen.findByText("1.100.000");
    const row = positionRow("BUMI");
    if (!row) throw new Error("baris BUMI tidak ada");
    row.focus(); // jsdom tidak memfokuskan tombol saat klik; peramban asli melakukannya
    fireEvent.click(row);
    return { row, dialog: await screen.findByRole("dialog", { name: "BUMI" }) };
  };
  const valueOf = (dialog: HTMLElement, label: string) => within(dialog).getByText(label).parentElement?.lastElementChild?.textContent;

  it("baris membuka dialog berjudul ticker dengan nama emiten dan kategori", async () => {
    const { dialog } = await openBumi();
    expect(within(dialog).getByText("BUMI Tbk")).toBeInTheDocument();
    expect(within(dialog).getByText("CORE")).toBeInTheDocument();
  });

  it("badan Sheet yang bisa tergulir punya wilayah fokus bernama (tabIndex 0), dan fokus bisa masuk ke sana", async () => {
    const { dialog } = await openBumi();
    const region = within(dialog).getByRole("group", { name: "Rincian posisi BUMI" });
    expect(region).toHaveAttribute("tabindex", "0");
    region.focus();
    expect(region).toHaveFocus();
  });

  it("memuat setiap angka posisi sebagai label–nilai", async () => {
    const { dialog } = await openBumi();
    expect(valueOf(dialog, "Lot")).toBe("1");
    expect(valueOf(dialog, "Lembar")).toBe("100");
    expect(valueOf(dialog, "Harga rata-rata")).toBe("Rp4.150");
    expect(valueOf(dialog, "Harga close")).toBe("Rp4.300");
    expect(valueOf(dialog, "Modal")).toBe("Rp700.000");
    expect(valueOf(dialog, "Nilai pasar")).toBe("Rp600.000");
    expect(valueOf(dialog, "P/L")).toBe("−Rp100.000");
    expect(valueOf(dialog, "P/L %")).toBe("−14,3%");
    expect(valueOf(dialog, "Bobot")).toBe("60%");
    expect(valueOf(dialog, "Target")).toBe("30%");
    expect(valueOf(dialog, "Drift")).toBe("+30 pp");
  });

  it("tautan Dossier tesis, Riset dan Matriks XBRL dihapus; satu-satunya tautan Buka halaman perusahaan", async () => {
    const { dialog } = await openBumi();
    expect(within(dialog).getAllByRole("link").map((a) => a.textContent)).toEqual(["Buka halaman perusahaan"]);
  });

  it("Buka halaman perusahaan: tautan lebar penuh ≥ 44 px ke /perusahaan/<T>?from=portofolio", async () => {
    const { dialog } = await openBumi();
    const link = within(dialog).getByRole("link", { name: "Buka halaman perusahaan" });
    expect(link).toHaveAttribute("href", "/perusahaan/BUMI?from=portofolio");
    expect(link.className).toContain("min-h-11");
    expect(link.className).toContain("w-full");
    fireEvent.click(link);
    expect(await screen.findByTestId("where")).toHaveTextContent("/perusahaan/BUMI?from=portofolio");
  });

  it("nominal di Sheet ikut tombol mata: Rp••• dan tidak ada angka rupiah", async () => {
    renderAt({ initial: true });
    await screen.findByRole("region", { name: "Nilai portofolio" });
    await screen.findByText("Bukan saran investasi");
    fireEvent.click(positionRow("BUMI") as HTMLElement);
    const dialog = await screen.findByRole("dialog", { name: "BUMI" });
    expect(valueOf(dialog, "Modal")).toBe("Rp•••");
    expect(valueOf(dialog, "P/L")).toBe("Rp•••");
    expect(valueOf(dialog, "P/L %")).toBe("−14,3%");
    expect(visibleRupiah()).toEqual([]);
  });

  it("Esc menutup Sheet dan fokus kembali ke baris yang membukanya", async () => {
    const { row, dialog } = await openBumi();
    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(row).toHaveFocus();
  });

  it("tombol Tutup menutup Sheet", async () => {
    const { dialog } = await openBumi();
    fireEvent.click(within(dialog).getByRole("button", { name: "Tutup" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});
