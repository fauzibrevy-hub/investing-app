import { periodLabel } from "../research/format";
import { isKpiCell } from "../research/components/MetricRow";
import type { PackGap, PackMetric, PackResponse } from "../research/types";
import { isBankTaxonomy, isHiddenCore } from "./lastQuarter";

export interface DriverGap {
  id: string;
  label: string;
  text: string;
  /** Sumbernya ada tetapi belum diambil: **Perbarui data** yang mengambilnya. */
  fetchable: boolean;
}

export interface DataGaps {
  /** `Produksi Q2 26` untuk tiap sel KPI yang dinantikan dan belum berisi apa pun. */
  kpi: string[];
  drivers: DriverGap[];
  /** Celah XBRL di kuartal terbaru, dengan alasan dari backend. */
  xbrl: string[];
  /** Kuartal yang seluruh angkanya kosong karena tidak ada laporan (mis. BUMI Q1 2024): bukan galat. */
  quarters: string[];
}

const MISSING_KPI = /^(\S+) (\d{4}Q[1-4])$/;
const MAX_XBRL = 6;
const squash = (s: string) => s.replace(/[_\s]/g, "").toLowerCase();

/**
 * Mengapa satu metrik kosong, dalam kata pemilik: tidak ada nama elemen mentah (`GrossProfit`, `Capex`). Angka yang
 * memang hilang dari laporan: "tidak ada di laporan Q2 26"; bahan rumusnya yang hilang: "bahan perhitungannya tidak ada
 * di laporan Q2 26". Sebab lain (pembagi nol, satuan berbeda) memakai kalimat backend.
 */
export function gapSentence(gap: PackGap, metric: PackMetric, when: string): string {
  if (gap.kind !== "missing") return gap.text;
  const own = !gap.ident || squash(gap.ident) === squash(metric.id);
  return own ? `tidak ada di laporan ${when}` : `bahan perhitungannya tidak ada di laporan ${when}`;
}

/**
 * Blok 6 "Data belum ada": daftar polos dari payload paket, tanpa menebak apa pun. Kosong di semua daftar berarti blok
 * disembunyikan.
 */
export function dataGaps(pack: PackResponse, taxonomy: string | null = null): DataGaps {
  const bank = isBankTaxonomy(taxonomy);
  const allMetrics = pack.dimensions.flatMap((d) => d.metrics);
  const byId = new Map(allMetrics.map((m) => [m.id, m]));
  // Baris inti yang disembunyikan di tabel (bank, atau kosong seluruh riwayat) bukan celah yang perlu diisi.
  const metrics = pack.dimensions.flatMap((d) => d.metrics.filter((m) => !isHiddenCore(m, d.pack === "core", bank)));

  const kpi = pack.coverage.kpi.missing.map((entry) => {
    const m = MISSING_KPI.exec(entry);
    return m ? `${byId.get(m[1])?.label ?? m[1]} ${periodLabel(m[2])}` : entry;
  });

  const drivers: DriverGap[] = [];
  for (const d of pack.drivers) {
    if (d.status === "tersedia") continue;
    const proxy = pack.missing_drivers.find((x) => x.id === d.id)?.proxy;
    // Driver pengganti ada di payload sebagai `proxy` milik driver yang belum bersumber, bukan sebagai driver tersendiri.
    const proxyLabel = proxy ? (pack.drivers.find((x) => x.id === proxy)?.label ?? d.proxy?.label ?? proxy) : null;
    if (d.status === "TBD") drivers.push({ id: d.id, label: d.label, text: `belum ada sumber${proxyLabel ? `, ${proxyLabel} dipakai sebagai pengganti` : ""}`, fetchable: false });
    else drivers.push({ id: d.id, label: d.label, text: "belum diambil", fetchable: true });
  }

  const quarters: string[] = [];
  const xbrl: string[] = [];
  const xbrlMetrics = metrics.filter((m) => m.layer === "xbrl");
  const noReport: string[] = [];
  for (const q of pack.quarters) {
    const cells = xbrlMetrics.map((m) => m.cells.find((c) => c.fy === q.fy && c.q === q.q)).filter((c) => c && !isKpiCell(c));
    if (cells.length === 0) continue;
    const empty = cells.every((c) => c && !isKpiCell(c) && c.value === null);
    const isLatest = q.label === pack.latest.label;
    if (empty && isLatest && !pack.latest.from_filing) quarters.push(`${periodLabel(q.label)}: laporan belum terbit`);
    else if (empty) noReport.push(periodLabel(q.label));
  }
  // Satu baris untuk semua kuartal tanpa laporan, bukan satu baris per kuartal.
  if (noReport.length) quarters.push(`${noReport.join(", ")}: tidak ada laporan untuk ${noReport.length > 1 ? "kuartal-kuartal ini" : "kuartal ini"}, jadi angkanya kosong dan tidak diisi`);
  if (pack.latest.from_filing) {
    const when = periodLabel(pack.latest.label);
    for (const m of xbrlMetrics) {
      const c = m.cells.find((x) => x.fy === pack.latest.fy && x.q === pack.latest.q);
      if (c && !isKpiCell(c) && c.value === null && c.gap) xbrl.push(`${m.label}: ${gapSentence(c.gap, m, when)}`);
    }
  }

  return { kpi, drivers, xbrl: xbrl.slice(0, MAX_XBRL).concat(xbrl.length > MAX_XBRL ? [`dan ${xbrl.length - MAX_XBRL} metrik lain`] : []), quarters };
}

export const hasGaps = (g: DataGaps): boolean => g.kpi.length + g.drivers.length + g.xbrl.length + g.quarters.length > 0;
