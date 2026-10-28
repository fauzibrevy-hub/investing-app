import { chooseScale, formatExact, formatScaled, formatValue, periodLabel, type Scale } from "../format";
import type { CellFlag, FactTarget, FinancialsResponse } from "../types";
import type { GapAction } from "./FactChip";

export interface QuarterTableCell {
  value: number | null;
  /** Teks angka terformat pada skala tabel; tanpa itu `formatValue(value)`. */
  text?: string;
  flags?: readonly CellFlag[];
  fact: FactTarget | null;
  /** Konteks yang bisa diisi: gap menjadi tombol (`Isi`). */
  gapAction?: GapAction;
}

export interface QuarterTableRow {
  id: string;
  label: string;
  /** Satuan di header baris, hanya untuk baris di luar skala tabel (per saham, rasio). */
  unit?: string;
  /** Baris subtotal ditebalkan. */
  subtotal?: boolean;
  /** Satu per periode, urutan sama dengan `periods`. */
  cells: QuarterTableCell[];
}

export interface QuarterTableGroup {
  id: string;
  label: string;
  rows: QuarterTableRow[];
}

export interface QuarterTablePeriod {
  key: string;
  /** `Q2 26`. */
  label: string;
  /** Kolom Q4 turunan dari FY: header berbadge `turunan`. */
  derived?: boolean;
  /** Periode dari laporan asli PDF (tanpa XBRL di IDX): header berbadge `PDF`. */
  pdf?: boolean;
}

/** Baris subtotal tabel keuangan inti (design.md §6 `QuarterTable`). */
export const SUBTOTAL_IDS: ReadonlySet<string> = new Set(["gross_profit", "operating_profit", "net_income", "free_cash_flow", "assets", "liabilities", "equity"]);

export interface QuarterTableData {
  periods: QuarterTablePeriod[];
  groups: QuarterTableGroup[];
  scale: Scale;
  /** Akhiran header kolom pertama, mis. `(RP MILIAR)`. */
  unitHeader: string;
}

/**
 * Backend menambahkan `/shares` pada elemen "PerShare" yang satuannya sudah `IDR/shares` (`IDR/shares/shares`);
 * satuan dinormalkan agar header baris tidak berlipat.
 */
export const normalizeUnit = (unit: string | null): string | null => (unit === null ? null : unit.replace(/(\/shares)+$/i, "/shares"));

/**
 * Baris bermata uang hanya bila satuannya mata uang pelaporan tabel (atau tidak dinyatakan). Selain itu (per saham,
 * `pure`, jumlah saham, mata uang lain) tidak ikut skala dan tidak ikut median; satuannya tampil di header baris.
 */
const isCurrencyUnit = (unit: string | null, currency: string) => unit === null || unit.toUpperCase() === currency.toUpperCase();

/**
 * `GET /{ticker}/financials` → data `QuarterTable`. Skala dipilih dari median seluruh sel bermata uang agar
 * median 3 sampai 6 digit; sel tidak pernah memuat `Rp`, `%`, atau `×`.
 */
export function quarterTableFromFinancials(fin: FinancialsResponse, subtotalIds: ReadonlySet<string> = SUBTOTAL_IDS): QuarterTableData {
  const currency = fin.currency ?? "IDR";
  const scaled = fin.groups.flatMap((g) => g.rows).filter((r) => isCurrencyUnit(normalizeUnit(r.unit), currency));
  const scale = chooseScale(
    scaled.flatMap((r) => r.cells.map((c) => c.value)),
    currency,
  );
  return {
    scale,
    unitHeader: scale.header,
    periods: fin.periods.map((p) => ({ key: p.label, label: periodLabel(p.label), derived: p.derived_fy, pdf: p.source_kind === "pdf" })),
    groups: fin.groups.map((g) => ({
      id: g.id,
      label: g.label,
      rows: g.rows.map((r) => {
        const unit = normalizeUnit(r.unit);
        const currencyRow = isCurrencyUnit(unit, currency);
        return {
          id: r.id,
          label: r.label,
          unit: currencyRow ? undefined : (unit ?? undefined),
          subtotal: subtotalIds.has(r.id),
          cells: r.cells.map((c) => ({
            value: c.value,
            flags: c.flags,
            fact: c.fact,
            text: c.value === null ? undefined : currencyRow ? formatScaled(c.value, scale) : unit && /\/shares$|^pure$/i.test(unit) ? formatValue(c.value, 2) : formatExact(c.value),
          })),
        };
      }),
    })),
  };
}
