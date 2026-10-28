/** Angka non-uang (lot, lembar, nilai indikator): locale id-ID, maks 2 desimal. Uang tetap lewat `formatRupiah`. */
const fmt = new Intl.NumberFormat("id-ID", { maximumFractionDigits: 2 });

export function formatNumber(value: number | null | undefined): string {
  return value === null || value === undefined || !Number.isFinite(value) ? "—" : fmt.format(value);
}
