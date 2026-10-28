import { formatDelta, formatPct } from "./format";
import type { YoY } from "./types";

/**
 * Satu aturan perubahan relatif untuk seluruh web (cermin `yoy_json` di backend): `(sekarang − lalu) / |lalu| × 100`
 * selama kedua nilai bertanda sama (atau sekarang nol). Tanda melewati nol tidak diberi persentase (`sign_change`);
 * dasar nol tidak punya perubahan relatif (`null`).
 */
export function relativeChange(now: number, then: number): YoY | null {
  if (!Number.isFinite(now) || !Number.isFinite(then) || then === 0) return null;
  if (now === 0 || Math.sign(now) === Math.sign(then)) return { kind: "pct", value: ((now - then) / Math.abs(then)) * 100 };
  return { kind: "sign_change", from: then, to: now };
}

/** Baris laba: pergantian tanda ditulis rugi ↔ laba, bukan negatif ↔ positif. */
const PROFIT_LINES = new Set(["gross_profit", "operating_profit", "net_income", "ebitda"]);
export const isProfitLine = (id: string): boolean => PROFIT_LINES.has(id);

/** Frasa pergantian tanda: `dari rugi ke laba`, `dari positif ke negatif`. */
export function signChangePhrase(from: number, to: number, profitLine: boolean): string {
  const [neg, pos] = profitLine ? ["rugi", "laba"] : ["negatif", "positif"];
  return from < 0 && to > 0 ? `dari ${neg} ke ${pos}` : `dari ${pos} ke ${neg}`;
}

/** Isi sel YoY yang sempit: `+12,3`, `−359,4`, atau `rugi → laba` untuk pergantian tanda. */
export function yoyCellText(yoy: YoY, profitLine = false): string {
  if (yoy.kind !== "sign_change") return formatDelta(yoy.value);
  const [neg, pos] = profitLine ? ["rugi", "laba"] : ["negatif", "positif"];
  return yoy.from < 0 ? `${neg} → ${pos}` : `${pos} → ${neg}`;
}

/** Kalimat fakta YoY: `Laba bersih turun 7,2% YoY`, `Laba bersih berbalik dari rugi ke laba YoY`. */
export function yoySentence(label: string, yoy: YoY, profitLine = false): string {
  if (yoy.kind === "sign_change") return `${label} berbalik ${signChangePhrase(yoy.from, yoy.to, profitLine)} YoY`;
  const verb = yoy.value > 0 ? "naik" : "turun";
  const size = formatPct(Math.abs(yoy.value));
  if (yoy.kind === "pp") return `${label} ${verb} ${size} poin YoY`;
  return yoy.kind === "x" ? `${label} ${verb} ${size}× YoY` : `${label} ${verb} ${size}% YoY`;
}
