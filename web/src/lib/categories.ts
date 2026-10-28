import type { CSSProperties } from "react";

/** Warna kategori (design.md §1.3). Ditugaskan berdasarkan peringkat nominal, bukan nama. */
export type CatIndex = 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8;

/** `cat-8` = "Lainnya". */
export const CAT_OTHER: CatIndex = 8;
export const MAX_RANKED = 7;

export const catColor = (i: CatIndex) => `var(--cat-${i})`;
export const catTint = (i: CatIndex) => `var(--cat-${i}-tint)`;

/** Peringkat 0-based (terbesar = 0) → token `cat-1…7`; di luar itu `cat-8`. */
export function catForRank(rank: number): CatIndex {
  return rank >= 0 && rank < MAX_RANKED ? ((rank + 1) as CatIndex) : CAT_OTHER;
}

/**
 * Urutan token untuk daftar yang harus membedakan hue per baris (Portofolio, design.md §3.2 blok 4: "satu hue yang jelas
 * berbeda per ticker, tidak pernah sederet biru"). Urutan bawaan memberi peringkat 3 dan 5 dua biru (cat-3 ≈ 215°,
 * cat-5 ≈ 216°). Di sini tiap awalan urutan memuat jarak hue terkecil terbaik yang mungkin dari delapan token:
 * sampai 4 emiten ≥ 30°, 5 emiten 25°, 6 emiten 17,9°; palet ini tidak punya lima hue yang saling ≥ 30°. cat-5 jatuh
 * terakhir (peringkat 7). Peringkat ke-8 ("Lainnya") tetap cat-8.
 */
export const DISTINCT_HUE_ORDER: readonly CatIndex[] = [1, 7, 2, 3, 4, 6, 5];

const SLOTS: readonly CatIndex[] = [1, 2, 3, 4, 5, 6, 7];

/**
 * Pemetaan hanya untuk satu subpohon (donut, legenda, bar), tanpa mengubah `catForRank` global yang dipakai donut lain:
 * `DonutChart` mewarnai busur dari `var(--cat-n)` menurut peringkat, jadi subpohon itu menimpa `--cat-n` menjadi token
 * `DISTINCT_HUE_ORDER[n−1]`. Dua lapis, karena `--cat-n: var(--cat-m)` pada elemen yang sama merujuk dirinya sendiri:
 * pembungkus luar (`DISTINCT_HUE_SOURCE`) menyalin token asli ke `--hue-n`, pembungkus dalam (`DISTINCT_HUE_REMAP`) memakainya.
 */
export const DISTINCT_HUE_SOURCE: CSSProperties = Object.fromEntries(SLOTS.flatMap((i) => [[`--hue-${i}`, `var(--cat-${i})`], [`--hue-${i}-tint`, `var(--cat-${i}-tint)`]]));
export const DISTINCT_HUE_REMAP: CSSProperties = Object.fromEntries(
  DISTINCT_HUE_ORDER.flatMap((from, rank) => [[`--cat-${rank + 1}`, `var(--hue-${from})`], [`--cat-${rank + 1}-tint`, `var(--hue-${from}-tint)`]]),
);
