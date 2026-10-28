import { isProfitLine, yoySentence } from "../research/change";
import type { PackMetric, PackResponse, YoY } from "../research/types";

/** Paling banyak tiga per daftar (ui-slicing.md §1 blok 4, D1). */
export const MOVERS_PER_LIST = 3;

export interface Mover {
  id: string;
  label: string;
  yoy: YoY;
  /** Kalimat fakta dengan angkanya, mis. `Biaya tunai turun 8,2% YoY`. */
  sentence: string;
}

export interface Movers {
  improving: Mover[];
  watching: Mover[];
  /** Alasan satu daftar kosong; `null` bila daftarnya berisi. */
  improvingEmpty: string | null;
  watchingEmpty: string | null;
}

export function moverSentence(label: string, yoy: YoY, profitLine = false): string {
  return yoySentence(label, yoy, profitLine);
}

/** Arah gerak nilai (sekarang − lalu): positif naik, negatif turun, nol tidak bergerak. Untuk persen dan poin sama dengan tanda `value`. */
function moveSign(yoy: YoY): number {
  return yoy.kind === "sign_change" ? Math.sign(yoy.to - yoy.from) : Math.sign(yoy.value);
}

/** Besar gerak untuk urutan. Pergantian tanda tidak punya persentase; ia gerak kualitatif terbesar, jadi di atas semua persen. */
function magnitude(yoy: YoY): number {
  return yoy.kind === "sign_change" ? Number.POSITIVE_INFINITY : Math.abs(yoy.value);
}

interface Candidate {
  metric: PackMetric;
  yoy: YoY;
  favourable: boolean;
}

/**
 * Metrik dengan YoY yang bergerak dan arah baik/buruk; `neutral`, YoY kosong, dan rasio "tidak bermakna" tidak ikut.
 * Baik/buruk menurut arah `(sekarang − lalu)` dan `direction`, bukan tanda persen saja.
 */
function candidates(metrics: readonly PackMetric[]): Candidate[] {
  const out: Candidate[] = [];
  for (const metric of metrics) {
    const yoy = metric.yoy;
    if (!yoy || metric.yoy_reason || metric.direction === "neutral") continue;
    if (yoy.kind === "sign_change" ? !Number.isFinite(yoy.from) || !Number.isFinite(yoy.to) : !Number.isFinite(yoy.value)) continue;
    const sign = moveSign(yoy);
    if (sign === 0) continue;
    out.push({ metric, yoy, favourable: metric.direction === "higher" ? sign > 0 : sign < 0 });
  }
  return out;
}

const biggest = (a: Candidate, b: Candidate) => {
  const [ma, mb] = [magnitude(a.yoy), magnitude(b.yoy)];
  if (ma === mb && a.yoy.kind === "sign_change" && b.yoy.kind === "sign_change") return Math.abs(b.yoy.to - b.yoy.from) - Math.abs(a.yoy.to - a.yoy.from);
  return mb - ma;
};

/**
 * "Membaik / Diawasi": tiga metrik paket dengan gerak YoY paling besar ke arah baik dan tiga ke arah buruk, menurut
 * `direction` tiap metrik (D1). Dihitung dari payload paket saja; tidak ada skor dan tidak ada vonis.
 */
export function computeMovers(pack: PackResponse): Movers {
  const metrics = pack.dimensions.flatMap((d) => d.metrics);
  const found = candidates(metrics).sort(biggest);
  const toMover = ({ metric, yoy }: Candidate): Mover => ({ id: metric.id, label: metric.label, yoy, sentence: moverSentence(metric.label, yoy, isProfitLine(metric.id)) });
  const improving = found.filter((c) => c.favourable).slice(0, MOVERS_PER_LIST).map(toMover);
  const watching = found.filter((c) => !c.favourable).slice(0, MOVERS_PER_LIST).map(toMover);

  const anyYoy = metrics.some((m) => m.yoy !== null);
  let why: string | null = null;
  if (found.length === 0) {
    if (!anyYoy) why = pack.coverage.kpi.missing.length > 0 ? "KPI belum diisi, jadi belum ada pembanding tahun lalu" : "Belum ada pembanding tahun lalu";
    else why = "Tidak ada metrik berarah yang berubah dibanding tahun lalu";
  }
  return {
    improving,
    watching,
    improvingEmpty: improving.length ? null : (why ?? "Belum ada metrik yang membaik dibanding tahun lalu"),
    watchingEmpty: watching.length ? null : (why ?? "Belum ada metrik yang memburuk dibanding tahun lalu"),
  };
}
