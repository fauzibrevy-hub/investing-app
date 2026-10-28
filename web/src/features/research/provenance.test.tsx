import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { SURFACES, mockCanonBackend, renderSurface } from "./canonSurfaces.testutil";

/** Elemen yang membawa asal angka: `FactChip` (`aria-haspopup="dialog"`), `SourcedValue` ("Asal angka …"), `KpiCell` ("… ubah KPI") dan tautan nilai turunan ke Paket. */
const PROVENANCE = 'button[aria-haspopup="dialog"], button[aria-label^="Asal angka"], button[aria-label*=", asal angka "], a[aria-label^="Asal angka"], button[aria-label$="ubah KPI"]';

/** Angka telanjang: tanda opsional, lalu digit dengan pemisah ribuan titik dan desimal koma (`1.234,5`); satuan menempel (`%`, `×`, `Rp`) tetap dihitung angka agar tidak lolos. */
const NUMBER = /^(?:Rp\s?)?[+\u2212-]?\d[\d.,]*\s?[%×]?$/;

/**
 * Daftar pengecualian (A5.5 / S5), tersurat dan masing-masing bukan angka keuangan yang bisa ditelusuri ke laporan.
 * Tanggal ("28 Okt 2026"), periode ("Q2 26"), usia ("3 hari") dan hitungan berkata ("2 level") bukan angka telanjang,
 * jadi tidak pernah sampai ke sini; yang tercantum di bawah adalah angka telanjang yang sah tanpa asal angka.
 */
const EXCLUSIONS: Array<{ name: string; why: string; match: (el: Element) => boolean }> = [
  { name: "sr-only", why: "teks pembaca layar, bukan tampilan angka", match: (el) => !!el.closest(".sr-only, .sr-only-table, [hidden], script, style") },
  { name: "header", why: "kepala kolom dan keterangan tabel (tahun, nomor periode)", match: (el) => !!el.closest("th, caption, kbd") },
  { name: "control", why: "kontrol pilihan (rentang 8/12/20 kuartal, zoom dan lipat cabang peta struktur), bukan nilai", match: (el) => !!el.closest('[role="radio"], [role="radiogroup"], [role="tab"], select, option, input, [data-mm-toggle]') || (el.getAttribute("aria-hidden") === "true" && !!el.closest("#struktur")) },
  { name: "chart", why: "label sumbu dan penanda nilai terakhir di dalam grafik; angka yang sama ada di statistik dan tabel sr-only bersumber", match: (el) => !!el.closest("svg, .pointer-events-none") },
  {
    name: "count",
    why: "hitungan cakupan: pecahan di samping meter, hitungan klaim di samping titik status, dan jumlah kuartal kosong",
    match: (el) =>
      el.previousElementSibling?.getAttribute("role") === "meter" ||
      (el.parentElement?.tagName === "LI" && el.previousElementSibling?.getAttribute("aria-hidden") === "true") ||
      /^\s*kuartal kosong/.test(el.nextSibling?.textContent ?? ""),
  },
  {
    name: "rule-parameter",
    why: "parameter aturan dan asumsi model (kartu Aturan, horizon dan pertumbuhan terminal DCF), bukan fakta laporan",
    match: (el) => !!el.closest('[aria-label="Aturan"], section[data-block="aturan"]') || /^(Horizon \(tahun\)|Pertumbuhan terminal)/.test(el.closest("dd")?.previousElementSibling?.textContent ?? ""),
  },
  { name: "provenance-badge", why: "angka dengan lencana asal (manual, turunan FY) di sebelahnya", match: (el) => !!el.parentElement?.querySelector(":scope > [data-provenance]") },
];

function visibleNumberNodes(root: HTMLElement): Text[] {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const out: Text[] = [];
  for (let n = walker.nextNode(); n; n = walker.nextNode()) if (NUMBER.test((n.textContent ?? "").trim())) out.push(n as Text);
  return out;
}

/** Angka telanjang yang tidak berada di dalam target asal angka dan tidak masuk daftar pengecualian. `MetricCard` (`article[data-metric]`) sah bila memuat target. */
export function unsourcedNumbers(root: HTMLElement): string[] {
  return visibleNumberNodes(root)
    .filter((n) => {
      const el = n.parentElement!;
      if (el.closest(PROVENANCE) || EXCLUSIONS.some((e) => e.match(el))) return false;
      const card = el.closest("article[data-metric]");
      return !(card && card.querySelector(PROVENANCE));
    })
    .map((n) => {
      const where = n.parentElement!.closest("[aria-label], [role], section, table, article")?.getAttribute("aria-label") ?? "?";
      return `"${n.textContent!.trim()}" di ${where}`;
    });
}

function dom(html: string) {
  const host = document.createElement("div");
  host.innerHTML = html;
  return host;
}

beforeEach(() => {
  mockCanonBackend();
});
afterEach(cleanup);

describe("every shown number opens its provenance (A5.5, S5)", () => {
  it("the checker flags a bare number and accepts one inside a provenance target", () => {
    expect(unsourcedNumbers(dom(`<div><span>154</span><span>1.234,5</span><span>\u221212,3</span></div>`))).toHaveLength(3);
    const chip = dom(`<span><button aria-haspopup="dialog"><span aria-hidden="true">154</span></button><button aria-label="Asal angka X: 5">5</button><a aria-label="Asal angka Y: 6">6</a></span>`);
    expect(unsourcedNumbers(chip)).toEqual([]);
    const card = dom(`<article data-metric="m"><span>9</span><button aria-haspopup="dialog">1</button></article>`);
    expect(unsourcedNumbers(card)).toEqual([]);
    expect(unsourcedNumbers(dom(`<article data-metric="m"><span>9</span></article>`))).toHaveLength(1);
  });

  it("a number with an attached unit is still a bare number", () => {
    expect(unsourcedNumbers(dom(`<div><span>12,3%</span><span>5×</span><span>Rp1.000</span></div>`))).toHaveLength(3);
  });

  it("dates, periods and ages are not bare numbers, so they never need a provenance target", () => {
    expect(unsourcedNumbers(dom(`<div><span>Q2 26</span><span>12 hari</span><span>28 Okt 2026</span><span>2026.10-1</span><span>2 level</span></div>`))).toEqual([]);
  });

  it.each([
    ["sr-only", `<span class="sr-only">3</span>`],
    ["header", `<table><caption>2026</caption><thead><tr><th>26</th></tr></thead></table>`],
    ["control", `<div role="radiogroup"><button role="radio">12</button></div><section id="struktur"><span aria-hidden="true">100%</span><button data-mm-toggle="n1"><span aria-hidden="true">+1</span></button></section>`],
    ["chart", `<svg><text>1.000</text></svg><div class="pointer-events-none"><span>1.290</span></div>`],
    ["count: meter fraction", `<span><span role="meter"></span><span>27</span></span>`],
    ["count: claim", `<ul><li><span aria-hidden="true"></span><span>2</span> berjalan</li></ul>`],
    ["count: empty quarters", `<a><span>8</span> kuartal kosong · Isi</a>`],
    ["rule-parameter", `<section aria-label="Aturan"><dl><dt>x</dt><dd>3</dd></dl></section><dl><dt>Horizon (tahun)</dt><dd>10</dd></dl>`],
    ["provenance-badge", `<span><span>500.000.000</span><span data-provenance="manual">manual</span></span>`],
  ])("exclusion %s is honoured", (_name, html) => {
    expect(unsourcedNumbers(dom(html))).toEqual([]);
  });

  it("the same bare numbers outside those contexts are still flagged", () => {
    expect(unsourcedNumbers(dom(`<table><tbody><tr><td><span>27</span></td></tr></tbody></table>`))).toHaveLength(1);
    expect(unsourcedNumbers(dom(`<dl><dt>Imbal hasil</dt><dd>10</dd></dl>`))).toHaveLength(1);
  });

  it.each(SURFACES)("$name: no number outside a FactChip, KpiCell, SourcedValue or sourced MetricCard", async (surface) => {
    const { container } = await renderSurface(surface);
    const offenders = unsourcedNumbers(container);
    expect(offenders, `angka tanpa asal di ${surface.name}: ${offenders.join("; ")}`).toEqual([]);
    // Bukan lolos karena kosong: permukaan data punya target asal angka (Beranda, Alat, Kalibrasi dan Paket menampilkan hitungan, data makro, harga dan teks, bukan angka laporan).
    if (!/Beranda|Perusahaan|Tambah|Alat|Calibration|Packs/.test(surface.name)) {
      expect(container.querySelectorAll(PROVENANCE).length, `${surface.name} tanpa target asal angka`).toBeGreaterThan(0);
    }
  });
});
