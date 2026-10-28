export type BlockId = "struktur" | "angka" | "kuartal" | "sorotan" | "klaim" | "kosong" | "jurnal" | "harga" | "keuangan" | "footer";

/** Blok yang bisa dilipat dan apakah terbuka pada awalnya (ui-slicing.md §1: Jurnal dan Keuangan lipat). */
export const COLLAPSIBLE: Partial<Record<BlockId, boolean>> = { jurnal: false, harga: true, keuangan: false };

const IDS: readonly BlockId[] = ["struktur", "angka", "kuartal", "sorotan", "klaim", "kosong", "jurnal", "harga", "keuangan", "footer"];

/** Nama `?section=` lama (tab dossier dan bagian kotak masuk) yang menunjuk ke blok baru. */
const ALIASES: Record<string, BlockId> = {
  ringkasan: "struktur",
  overview: "struktur",
  paket: "kuartal",
  pack: "kuartal",
  keuangan: "keuangan",
  financials: "keuangan",
  harga: "harga",
  price: "harga",
  klaim: "klaim",
  claims: "klaim",
  data: "kosong",
  jurnal: "jurnal",
};

/** `?section=` → blok yang digulirkan ke atas; `null` untuk nilai kosong atau tidak dikenal (sumber dan valuasi tidak lagi punya blok). */
export function sectionTarget(param: string | null): BlockId | null {
  if (!param) return null;
  if ((IDS as readonly string[]).includes(param)) return param as BlockId;
  return ALIASES[param] ?? null;
}
