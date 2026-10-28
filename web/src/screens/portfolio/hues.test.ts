import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { CAT_OTHER, catForRank, DISTINCT_HUE_ORDER, DISTINCT_HUE_REMAP, DISTINCT_HUE_SOURCE, type CatIndex } from "../../lib/categories";

// vitest.config `css: false` mengosongkan impor `.css` (juga `?raw`), jadi tokens dibaca dari berkasnya.
const tokens = readFileSync("src/styles/tokens.css", "utf8") /* `npm test` jalan dari web/ */;

/** Hex `--cat-N` dari tokens.css (design.md §1.3). */
const HEX = Object.fromEntries(Array.from(tokens.matchAll(/--cat-(\d):\s*(#[0-9a-fA-F]{6})/g)).map((m) => [Number(m[1]), m[2]])) as Record<number, string>;

function hueOf(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
  const max = Math.max(r, g, b);
  const d = max - Math.min(r, g, b);
  if (d === 0) return 0;
  return max === r ? ((g - b) / d + (g < b ? 6 : 0)) * 60 : max === g ? ((b - r) / d + 2) * 60 : ((r - g) / d + 4) * 60;
}
const gap = (a: number, b: number) => 180 - Math.abs((Math.abs(a - b) % 360) - 180);
const hueByCat = (cat: number) => hueOf(HEX[cat]);
const minGap = (cats: readonly number[]) => {
  let min = 360;
  for (let i = 0; i < cats.length; i++) for (let j = i + 1; j < cats.length; j++) min = Math.min(min, gap(hueByCat(cats[i]), hueByCat(cats[j])));
  return min;
};
const subsets = <T,>(items: T[], k: number): T[][] => (k === 0 ? [[]] : items.flatMap((x, i) => subsets(items.slice(i + 1), k - 1).map((rest) => [x, ...rest])));

/** Token peringkat 0–7 di Portofolio: tujuh peringkat berurutan dari `DISTINCT_HUE_ORDER`, peringkat ke-8 ("Lainnya") = cat-8. */
const colourOfRank = (rank: number): CatIndex => (rank < DISTINCT_HUE_ORDER.length ? DISTINCT_HUE_ORDER[rank] : CAT_OTHER);
const RANKS = Array.from({ length: 8 }, (_, i) => i);

describe("Portofolio: hue donut per ticker (design.md §3.2 blok 4)", () => {
  it("tokens.css memuat cat-1…8 dan urutan hue adalah permutasi cat-1…7", () => {
    expect(Object.keys(HEX)).toHaveLength(8);
    expect([...DISTINCT_HUE_ORDER].sort()).toEqual([1, 2, 3, 4, 5, 6, 7]);
  });

  it("8 peringkat memakai 8 token berbeda; peringkat terbesar tetap cat-1 dan 'Lainnya' tetap abu cat-8", () => {
    const cats = RANKS.map(colourOfRank);
    expect(new Set(cats).size).toBe(8);
    expect(cats[0]).toBe(1);
    expect(cats[7]).toBe(CAT_OTHER);
  });

  it("cat-5 (biru tua, hue ≈ cat-3) dipakai paling akhir: tidak ada dua biru sebelum semua hue lain terpakai", () => {
    expect(DISTINCT_HUE_ORDER[DISTINCT_HUE_ORDER.length - 1]).toBe(5);
    for (let n = 2; n <= 6; n++) {
      const blues = DISTINCT_HUE_ORDER.slice(0, n).filter((c) => hueByCat(c) > 190 && hueByCat(c) < 250);
      expect(blues.length, `${n} peringkat`).toBeLessThanOrEqual(1);
    }
  });

  it("untuk 2–6 peringkat, jarak hue terkecil = yang terbaik yang mungkin dari token; urutan bawaan cat-1…5 hampir sama (biru dua kali)", () => {
    const chromaticWithoutNavy = [1, 2, 3, 4, 6, 7];
    for (let n = 2; n <= 6; n++) {
      const best = Math.max(...subsets(chromaticWithoutNavy, n).map(minGap));
      expect(minGap(DISTINCT_HUE_ORDER.slice(0, n)), `${n} peringkat`).toBeCloseTo(best, 6);
    }
    // Sampai 4 emiten: tidak ada dua hue dalam 30°. 5 dan 6 emiten: palet 8 token tidak punya 5 hue yang saling ≥ 30°
    // (terbaik 25° dan 17,9°), jadi itu batas yang dipin; melonggarkannya perlu token baru (keputusan pemilik).
    expect(minGap(DISTINCT_HUE_ORDER.slice(0, 4))).toBeGreaterThanOrEqual(30);
    expect(minGap(DISTINCT_HUE_ORDER.slice(0, 5))).toBeGreaterThanOrEqual(25);
    expect(minGap(DISTINCT_HUE_ORDER.slice(0, 6))).toBeGreaterThanOrEqual(17);
    expect(minGap([1, 2, 3, 4, 5])).toBeLessThan(2);
  });

  it("pemetaan CSS lokal: peringkat n memakai token DISTINCT_HUE_ORDER[n−1] lewat --hue-* (tanpa merujuk dirinya sendiri)", () => {
    const remap = DISTINCT_HUE_REMAP as Record<string, string>;
    const source = DISTINCT_HUE_SOURCE as Record<string, string>;
    for (let rank = 0; rank < 7; rank++) {
      expect(remap[`--cat-${rank + 1}`]).toBe(`var(--hue-${DISTINCT_HUE_ORDER[rank]})`);
      expect(remap[`--cat-${rank + 1}-tint`]).toBe(`var(--hue-${DISTINCT_HUE_ORDER[rank]}-tint)`);
    }
    expect(remap["--cat-8"]).toBeUndefined();
    for (const key of Object.keys(remap)) expect(source[`--hue-${key.slice("--cat-".length)}`]).toBe(`var(--cat-${key.slice("--cat-".length)})`);
    // Pemetaan hanya untuk layar ini; peringkat global (donut lain) tidak berubah.
    expect(RANKS.map(catForRank)).toEqual([1, 2, 3, 4, 5, 6, 7, 8]);
  });
});
