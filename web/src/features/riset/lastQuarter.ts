import { chooseScale, currencyWord, type Scale } from "../research/format";
import type { PackCell, PackMetric, PackResponse } from "../research/types";

const SCALE_ABBREVIATION: Record<string, string> = { ribu: "rb", juta: "jt", miliar: "M", triliun: "T" };

/**
 * Satuan untuk baris kedua di bawah label metrik: `USD rb`, `Rp T`, `%`, `×`, `Mt`. Mata uang yang belum diketahui
 * ditulis "mata uang laporan", tidak pernah ditebak `Rp`.
 */
export function unitCaption(unit: string, currency: string | null, scale: Scale | null): string {
  if (unit === "ccy") {
    const word = currency ? currencyWord(currency) : "mata uang laporan";
    const abbreviation = scale?.word ? SCALE_ABBREVIATION[scale.word] : "";
    const code = word === "RP" ? "Rp" : word;
    return abbreviation ? `${code} ${abbreviation}` : code;
  }
  if (unit === "x" || unit === "×") return "×";
  return unit;
}

/** Metrik TTM di bawah kolom kuartal membaca empat kuartal terakhir, jadi labelnya mengatakannya. Dipakai halaman emiten dan Bandingkan. */
export const quarterTableLabel = (metric: { label: string; period?: string | null }): string => (metric.period === "ttm" ? `${metric.label} (TTM)` : metric.label);

const MONEY_MULTIPLIER: Record<string, number> = { ribu: 1e3, juta: 1e6, miliar: 1e9, triliun: 1e12 };

/**
 * KPI uang yang satuannya tertulis di paket (`USD juta`, `IDR miliar`): mata uang dan pengali ke satuan penuh, supaya
 * nilainya bisa ditampilkan pada skala tabel yang sama dengan baris uang lain. `null` untuk satuan lain.
 */
export function kpiMoneyUnit(unit: string): { currency: string; multiplier: number } | null {
  const m = /^(IDR|USD)\s+(ribu|juta|miliar|triliun)$/i.exec(unit.trim());
  return m ? { currency: m[1].toUpperCase(), multiplier: MONEY_MULTIPLIER[m[2].toLowerCase()] } : null;
}

export interface QuarterRow {
  metric: PackMetric;
  /** Kuartal standalone terbaru (`pack.latest`); `null` bila paket tidak punya sel untuk kuartal itu. */
  latest: PackCell | null;
  /** Kuartal yang sama setahun sebelumnya. */
  prior: PackCell | null;
}

export interface QuarterGroup {
  /** `<paket>.<dimensi>`, unik antar grup. */
  id: string;
  /** Judul grup: nama dimensi paket (Volume, Harga, Biaya, Laba rugi …). */
  label: string;
  /** Dimensi inti (laba rugi dan neraca), bukan dimensi paket industri. */
  core: boolean;
  rows: QuarterRow[];
}

/**
 * Satu skala jumlah untuk seluruh tabel emiten, dipilih dari besar pendapatan (dua kuartal yang tampil), supaya `USD jt`
 * dan `USD rb` tidak bercampur antar baris. `null` bila pendapatan tidak ada; tiap baris lalu memilih skalanya sendiri.
 */
export function tableScale(groups: readonly QuarterGroup[], currency: string | null): Scale | null {
  const revenue = groups.flatMap((g) => g.rows).find((r) => r.metric.id === "revenue" && r.metric.unit === "ccy");
  if (!revenue) return null;
  const values = [revenue.latest, revenue.prior].map((c) => (c && !("state" in c) ? c.value : null));
  return values.some((v) => v !== null) ? chooseScale(values, currency ?? "IDR") : null;
}

/** Taksonomi laporan bank (`financial_sharia`, `financial_...`): yang bukan bank memakai `general` atau `infrastructure`. */
export const isBankTaxonomy = (taxonomy: string | null | undefined): boolean => Boolean(taxonomy?.startsWith("financial"));

/**
 * Baris inti yang tidak dibawa laporan bank: laba kotor, belanja modal, arus kas bebas, EBITDA, dan utang berbunga.
 * Bank membiayai diri dengan dana pihak ketiga, jadi "utang bersih" tidak bermakna dan kata "bunga" tidak dipakai.
 */
const NON_BANK_CORE = new Set(["gross_profit", "capex", "free_cash_flow", "ebitda", "total_debt", "net_debt"]);

/** Paket industri untuk laporan bank; perbandingan tidak punya taksonomi per emiten, jadi mengenali bank dari paketnya. */
export const isBankPack = (packId: string | null | undefined): boolean => packId === "bank_sharia";

/** Baris inti yang tidak dibawa bank, dipakai halaman emiten dan Bandingkan dengan satu daftar. */
export const isNonBankCore = (metricId: string, core: boolean, bank: boolean): boolean => core && bank && NON_BANK_CORE.has(metricId);

/** Metrik inti xbrl yang kosong di seluruh riwayat payload: tidak ada yang bisa ditampilkan, jadi barisnya tidak dibuat. */
const emptyForever = (metric: PackMetric): boolean => metric.layer === "xbrl" && metric.cells.every((c) => !("state" in c) && c.value === null);

/** Metrik inti yang tidak ditampilkan dan tidak pula dihitung sebagai celah: bukan bagian emiten ini. */
export function isHiddenCore(metric: PackMetric, core: boolean, bank: boolean): boolean {
  return isNonBankCore(metric.id, core, bank) || (core && emptyForever(metric));
}

/**
 * Blok 3 "Kuartal terakhir": satu baris per metrik, dikelompokkan menurut dimensi paket dalam urutan payload;
 * kuartal terbaru berhadapan dengan kuartal yang sama tahun lalu. Baris inti yang tidak bisa dibawa taksonomi laporan
 * (bank) atau yang kosong di seluruh riwayat disembunyikan; dimensi tanpa baris tidak dibuat. Dimensi bernama sama
 * (Neraca inti dan Neraca paket batu bara) menjadi satu grup: judulnya tampil sekali, barisnya berurutan menurut payload.
 */
export function lastQuarterGroups(pack: PackResponse, taxonomy: string | null = null): QuarterGroup[] {
  const { fy, q } = pack.latest;
  const bank = isBankTaxonomy(taxonomy);
  const groups = pack.dimensions
    .map((d) => {
      const core = d.pack === "core";
      return {
        id: `${d.pack}.${d.id}`,
        label: d.label,
        core,
        rows: d.metrics
          .filter((metric) => !isHiddenCore(metric, core, bank))
          .map((metric) => ({
            metric,
            latest: metric.cells.find((c) => c.fy === fy && c.q === q) ?? null,
            prior: metric.cells.find((c) => c.fy === fy - 1 && c.q === q) ?? null,
          })),
      };
    })
    .filter((g) => g.rows.length > 0);
  const merged: QuarterGroup[] = [];
  for (const g of groups) {
    const same = merged.find((m) => m.label === g.label);
    if (same) same.rows.push(...g.rows);
    else merged.push(g);
  }
  return merged;
}
