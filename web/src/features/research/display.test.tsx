import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { SURFACES, canonPayloads, mockCanonBackend, renderSurface } from "./canonSurfaces.testutil";
import { stringsOf } from "./screenAsserts.testutil";

/** Elemen tanpa teks pembaca layar: yang benar-benar tampil di sel. */
function visibleText(el: Element): string {
  const copy = el.cloneNode(true) as Element;
  for (const hidden of copy.querySelectorAll(".sr-only, .sr-only-table, [hidden]")) hidden.remove();
  return (copy.textContent ?? "").replace(/\s+/g, " ").trim();
}

const NUMERIC_TEXT = /^[+−-]?\d[\d.,]*$/;
const SOURCED = 'button[aria-haspopup="dialog"], button[aria-label^="Asal angka"], a[aria-label^="Asal angka"], button[aria-label$="ubah KPI"]';

/** Sel angka: teks tampilnya angka telanjang, atau memuat target asal angka (`FactChip`, `SourcedValue`, `KpiCell`). */
function isNumericCell(td: Element): boolean {
  return NUMERIC_TEXT.test(visibleText(td)) || (td.querySelector(SOURCED) !== null && /\d/.test(visibleText(td)));
}

const rightAligned = (td: Element) => td.classList.contains("text-right") || [...td.children].some((c) => c.classList.contains("text-right") || c.classList.contains("justify-end") || c.classList.contains("items-end"));
const tabular = (td: Element) => !!td.closest(".num") || td.querySelector(".num") !== null;

const describeCell = (td: Element) => `"${visibleText(td).slice(0, 24)}" (${td.className.slice(0, 50)})`;

/** Sel angka dari tabel yang tampil; tabel `sr-only` (data grafik untuk pembaca layar) tidak punya tata letak untuk diperiksa. */
function shownCells(root: HTMLElement): Element[] {
  return [...root.querySelectorAll("table")].filter((t) => !t.closest(".sr-only, .sr-only-table")).flatMap((t) => [...t.querySelectorAll("td")]).filter(isNumericCell);
}

/** Unit di dalam sel (angka dengan %, × atau Rp): satuan hanya di judul dan kepala kolom. */
const UNIT_IN_CELL = /\d\s*(%|×)|Rp\s?\d/;

/** Semua sel tampil (angka atau teks) dari tabel yang tampil, termasuk sel yang bukan angka telanjang. */
function shownTds(root: HTMLElement): Element[] {
  return [...root.querySelectorAll("table")].filter((t) => !t.closest(".sr-only, .sr-only-table")).flatMap((t) => [...t.querySelectorAll("td")]);
}

/** Lencana: `span` `rounded-sm` di `surface-sunken`, di luar tombol (yang ini aksi, bukan lencana). */
function badges(root: HTMLElement): Element[] {
  return [...root.querySelectorAll("span")].filter((el) => el.classList.contains("rounded-sm") && el.classList.contains("bg-surface-sunken") && !el.closest("button, a"));
}

/**
 * A dash that stands for a gap is not bare when its own cell says why: `aria-hidden` mark inside `[data-gap]` with a
 * `title` and a `sr-only` reason beside it (Bandingkan and Kuartal terakhir empty cells), or `SourcedStat`'s empty
 * `StatRow` (`aria-label="…: belum ada data"`) followed by its visible `gapReason` caption.
 */
function isExplainedGap(el: Element | null): boolean {
  const cell = el?.closest("[data-gap]");
  if (cell) return el?.getAttribute("aria-hidden") === "true" && !!cell.getAttribute("title") && !!cell.querySelector(":scope > .sr-only")?.textContent?.trim();
  return Boolean(el?.getAttribute("aria-label")?.endsWith(": belum ada data") && el.parentElement?.nextElementSibling?.textContent?.trim());
}

const MINUS = "−";

beforeEach(() => {
  mockCanonBackend();
});
afterEach(cleanup);

describe("canon display rules in research tables (design §7, A5.3)", () => {
  it("the helpers tell a right-aligned tabular number from a bare one", () => {
    const host = document.createElement("div");
    host.innerHTML = `<table><tr><td class="num text-right">1</td><td>2</td><td><span class="num">3</span></td><td class="text-right"><button aria-haspopup="dialog" class="num"><span aria-hidden="true">4</span></button></td><td>teks</td></tr></table>`;
    const [a, b, c, d, e] = [...host.querySelectorAll("td")];
    expect([a, b, c, d, e].map(isNumericCell)).toEqual([true, true, true, true, false]);
    expect([a, b, c, d].map(rightAligned)).toEqual([true, false, false, true]);
    expect([a, b, c, d].map(tabular)).toEqual([true, false, true, true]);
  });

  it.each(SURFACES)("$name: numeric cells are right-aligned with tabular-nums, and carry no Rp, % or ×", async (surface) => {
    const { container } = await renderSurface(surface);
    const cells = shownCells(container);
    const misaligned = cells.filter((td) => !rightAligned(td)).map(describeCell);
    expect(misaligned, `sel angka tidak rata kanan di ${surface.name}: ${misaligned.join("; ")}`).toEqual([]);
    const proportional = cells.filter((td) => !tabular(td)).map(describeCell);
    expect(proportional, `sel angka tanpa tabular-nums di ${surface.name}: ${proportional.join("; ")}`).toEqual([]);
    const withUnit = cells.filter((td) => /Rp|%|×/.test(visibleText(td))).map(describeCell);
    expect(withUnit, `satuan di dalam sel di ${surface.name}: ${withUnit.join("; ")}`).toEqual([]);
    // Sel yang bukan angka telanjang (angka dengan satuan menempel) juga tidak boleh membawa satuan.
    const unitAnywhere = shownTds(container).filter((td) => UNIT_IN_CELL.test(visibleText(td))).map(describeCell);
    expect(unitAnywhere, `satuan menempel di sel di ${surface.name}: ${unitAnywhere.join("; ")}`).toEqual([]);
  });

  it("an explained gap dash passes, a bare dash does not", () => {
    const host = document.createElement("div");
    host.innerHTML = `<span title="Belum diisi" data-gap=""><span aria-hidden="true">—</span><span class="sr-only">Pendapatan COAL: Belum diisi</span></span><div><div><span aria-label="P/E TTM: belum ada data">—</span></div><span>Belum ada laba 12 bulan</span></div><div><div><span aria-label="P/E TTM: belum ada data">—</span></div></div><span data-gap=""><span aria-hidden="true">—</span></span><span><span aria-hidden="true">—</span><span class="sr-only">x</span></span><span>—</span>`;
    const dashes = [...host.querySelectorAll("span")].filter((el) => el.textContent === "—" && el.children.length === 0);
    expect(dashes.map(isExplainedGap)).toEqual([true, true, false, false, false, false]);
  });

  it("the unit pattern flags attached units and passes plain numbers and words", () => {
    for (const bad of ["12,3%", "5 ×", "Rp1.000", "Rp 1.000", "3×"]) expect(UNIT_IN_CELL.test(bad), bad).toBe(true);
    for (const ok of ["1.234,5", "Produksi", "12 hari", "−3,2"]) expect(UNIT_IN_CELL.test(ok), ok).toBe(false);
  });

  // Semua permukaan dirender dalam satu test: diberi tenggat sendiri karena default 5 detik tidak cukup.
  it("the surfaces really contain numeric cells and a negative number, so the checks above are not vacuous", { timeout: 60_000 }, async () => {
    let numeric = 0;
    let negatives = 0;
    for (const surface of SURFACES) {
      const { container } = await renderSurface(surface);
      numeric += shownCells(container).length;
      negatives += (container.textContent ?? "").split(MINUS).length - 1;
      cleanup();
    }
    expect(numeric).toBeGreaterThan(100);
    expect(negatives).toBeGreaterThan(0);
  });

  it.each(SURFACES)("$name: negatives use U+2212, never a hyphen", async (surface) => {
    const { container } = await renderSurface(surface);
    // Kalimat peristiwa yang disusun backend ("Ekuitas negatif pada 2025Q4: -40000000") tampil apa adanya; format angkanya
    // urusan backend (TBD R5-E2 di plan.md), bukan angka yang diformat layar ini.
    const fromBackend = new Set(stringsOf(canonPayloads()).map((t) => t.trim()));
    const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
    const hyphenated: string[] = [];
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      if (n.parentElement?.closest(".sr-only, .sr-only-table, [hidden]")) continue;
      const text = n.textContent ?? "";
      if (fromBackend.has(text.trim())) continue;
      // Tanda hubung di depan angka yang berdiri sendiri; tanggal (`2026-10-28`) dan rentang punya angka di depannya.
      if (/(^|[\s(])-\d/.test(text)) hyphenated.push(`${text.trim()} <${n.parentElement?.className}>`);
    }
    expect(hyphenated, `minus berupa tanda hubung di ${surface.name}`).toEqual([]);
  });

  it.each(SURFACES)("$name: badges are 11 px uppercase in text-2 on surface-sunken", async (surface) => {
    const { container } = await renderSurface(surface);
    const wrong = badges(container)
      .filter((el) => {
        const c = el.classList;
        return !(c.contains("uppercase") && (c.contains("text-table-head") || c.contains("text-[11px]")) && c.contains("text-text-2"));
      })
      .map((el) => `"${el.textContent}" (${el.className})`);
    expect(wrong, `lencana bukan gaya 11 px kapital di ${surface.name}`).toEqual([]);
  });

  it.each(SURFACES)("$name: a missing value is never a bare dash or n/a", async (surface) => {
    const { container } = await renderSurface(surface);
    const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
    const bare: string[] = [];
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      if (n.parentElement?.closest(".sr-only, .sr-only-table, [hidden]")) continue;
      if (/^\s*(—|–|-|n\/a|N\/A)\s*$/.test(n.textContent ?? "") && !isExplainedGap(n.parentElement)) bare.push(`<${n.parentElement?.tagName} class="${n.parentElement?.className}">`);
    }
    expect(bare, `strip kosong atau n/a di ${surface.name}`).toEqual([]);
  });
});
