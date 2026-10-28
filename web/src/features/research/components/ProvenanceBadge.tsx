import { cn } from "../../../lib/cn";
import type { CellFlag } from "../types";

export type ProvenanceKind = "xbrl" | "pdf" | "manual" | "turunan" | "pembanding" | "disajikan" | "usulan" | "basis" | "tanda" | "per_saham";

const LABELS: Record<ProvenanceKind, string> = {
  xbrl: "XBRL",
  pdf: "PDF",
  manual: "manual",
  turunan: "turunan FY",
  pembanding: "dari pembanding",
  disajikan: "disajikan kembali",
  usulan: "usulan",
  basis: "Basis berubah",
  tanda: "Tanda bertentangan",
  per_saham: "Per saham, perkiraan",
};

/** Penanda pendek untuk sel tabel sempit (`compact`); nama lengkap tetap jadi nama aksesibel badge. */
const SHORT: Record<ProvenanceKind, string> = {
  xbrl: "X",
  pdf: "PDF",
  manual: "M",
  turunan: "FY",
  pembanding: "PB",
  disajikan: "DK",
  usulan: "U",
  basis: "BB",
  tanda: "±",
  per_saham: "≈",
};

const HINTS: Record<ProvenanceKind, string> = {
  xbrl: "Dibaca dari laporan XBRL",
  pdf: "Laporan dari PDF asli, bukan XBRL; halaman tiap angka ada di asal angka",
  manual: "Diisi pemilik dengan dokumen sumber",
  turunan: "Diturunkan dari angka FY yang diaudit (FY dikurangi sembilan bulan)",
  pembanding: "Kuartal ini diisi dari kolom pembanding laporan berikutnya",
  disajikan: "Angka disajikan kembali oleh laporan yang lebih baru; angka asli ada di asal angka",
  usulan: "Dibaca dari dokumen unggahan; belum menjadi nilai KPI sampai pemilik menerimanya",
  basis: "Tahun ini dilaporkan ulang dengan basis lain oleh laporan berikutnya; TTM dan YoY yang melintasinya tidak sebanding",
  tanda: "Laporan berikutnya mencantumkan angka yang sama dengan tanda berlawanan; angka asli tetap ditampilkan",
  per_saham: "Angka per saham hasil selisih atau jumlah antarperiode; hanya perkiraan",
};

/** Bendera sel turunan → badge (design.md §3.2). `gap`, `instant`, `reconcile_failed` tidak punya badge. */
export function badgesForFlags(flags: readonly CellFlag[] = []): ProvenanceKind[] {
  const out: ProvenanceKind[] = [];
  if (flags.includes("derived_fy")) out.push("turunan");
  if (flags.includes("from_comparative")) out.push("pembanding");
  if (flags.includes("restated")) out.push("disajikan");
  if (flags.includes("basis_break")) out.push("basis");
  if (flags.includes("sign_conflict")) out.push("tanda");
  if (flags.includes("per_share_approx")) out.push("per_saham");
  return out;
}

/**
 * Legenda badge ringkas, mis. `FY = turunan FY · ≈ = Per saham, perkiraan`: `title` tidak muncul di layar sentuh, jadi
 * penanda pendek dijelaskan dengan teks terlihat. Hanya jenis yang diberikan, urutan tetap; penanda yang sama dengan
 * namanya (`PDF`) dilewati.
 */
export function badgeLegend(kinds: Iterable<ProvenanceKind>): string {
  const shown = new Set(kinds);
  return (Object.keys(SHORT) as ProvenanceKind[])
    .filter((k) => shown.has(k) && SHORT[k] !== LABELS[k])
    .map((k) => `${SHORT[k]} = ${LABELS[k]}`)
    .join(" · ");
}

export interface ProvenanceBadgeProps {
  kind: ProvenanceKind;
  /** Menggantikan teks bawaan, mis. `turunan` di header kolom Q4. */
  text?: string;
  /** Tooltip dengan tanggal dan sumber; default penjelasan jenisnya. */
  title?: string;
  /** Penanda pendek (mis. `FY`) dengan nama aksesibel lengkap, agar badge tidak melebarkan kolom angka. */
  compact?: boolean;
  className?: string;
}

/**
 * Badge 11 px huruf besar, `text-2` di `surface-sunken`, `radius-sm`, tidak pernah berwarna (design.md §7.7).
 * Tidak punya state memuat, kosong, atau galat: hanya ditampilkan bila sumbernya ada.
 */
export function ProvenanceBadge({ kind, text, title, compact = false, className }: ProvenanceBadgeProps) {
  const full = text ?? LABELS[kind];
  if (compact) {
    return (
      <span
        data-provenance={kind}
        role="img"
        aria-label={full}
        title={`${full}: ${title ?? HINTS[kind]}`}
        className={cn("inline-flex h-4 shrink-0 items-center whitespace-nowrap rounded-sm bg-surface-sunken px-1 text-table-head uppercase text-text-2", className)}
      >
        {SHORT[kind]}
      </span>
    );
  }
  return (
    <span
      data-provenance={kind}
      title={title ?? HINTS[kind]}
      className={cn("inline-flex h-4 shrink-0 items-center whitespace-nowrap rounded-sm bg-surface-sunken px-1 text-table-head uppercase text-text-2", className)}
    >
      {full}
    </span>
  );
}
