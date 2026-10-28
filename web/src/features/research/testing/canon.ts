/**
 * Golden canon dari backend (`tests/fixtures/research/canon/*.json`) untuk tes layar. Berkas yang tidak ada
 * adalah galat keras, bukan data pengganti: tes yang memintanya gagal.
 */
const files = import.meta.glob("../../../../../tests/fixtures/research/canon/*.json", { eager: true, import: "default" }) as Record<string, unknown>;
const byName = new Map(Object.entries(files).map(([path, value]) => [path.split("/").pop()!.replace(/\.json$/, ""), value]));

/** Salinan dalam golden bernama `name` (tanpa `.json`), supaya tes boleh mengubahnya. */
export function golden<T>(name: string): T {
  if (!byName.has(name)) throw new Error(`golden canon "${name}" tidak ada di tests/fixtures/research/canon`);
  return structuredClone(byName.get(name)) as T;
}

/** Apakah ada golden bernama `name` (untuk emiten yang hanya punya sebagian golden). */
export function hasGolden(name: string): boolean {
  return byName.has(name);
}
