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
