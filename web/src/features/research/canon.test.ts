import { describe, expect, it } from "vitest";

/**
 * Aturan canon yang diuji, bukan dipercaya (plan.md §7.4, design.md §7): tanpa kosakata vonis dan tanpa impor
 * dari `screens/` atau panggilan ke endpoint modul lain. Kata terlarang disusun dari potongan agar
 * grep gerbang pada folder ini tetap kosong.
 */
const FORBIDDEN = [["be", "li"], ["ju", "al"], ["kelu", "ar"], ["cor", "et"], ["lay", "ak"], ["mur", "ah"], ["mah", "al"], ["waj", "ar"]].map((p) => p.join(""));

const sources = import.meta.glob("./**/*.{ts,tsx}", { query: "?raw", import: "default", eager: true }) as Record<string, string>;
const production = Object.entries(sources).filter(([path]) => !/\.test\.tsx?$/.test(path) && !path.endsWith("canon.test.ts"));

describe("canon rules over the research feature source", () => {
  it("finds the feature's own files", () => {
    expect(production.length).toBeGreaterThan(10);
  });

  it("uses no verdict vocabulary as a word", () => {
    const pattern = new RegExp(`\\b(${FORBIDDEN.join("|")})\\b`, "i");
    const hits = production.flatMap(([path, text]) => text.split("\n").map((line, i) => ({ path, line: i + 1, text: line })).filter((l) => pattern.test(l.text)));
    expect(hits).toEqual([]);
  });

  it("imports nothing from web/src/screens and calls no other module endpoint", () => {
    // Dibangun dari potongan agar grep independensi pada folder ini tetap kosong.
    const banned = [["the", "sis"], ["vau", "lt"], ["fal", "sif"]].map((p) => p.join(""));
    const pattern = new RegExp(`from\\s+["'][^"']*/screens/|/api/idx|${banned.join("|")}`, "i");
    const hits = production.filter(([, text]) => pattern.test(text)).map(([path]) => path);
    expect(hits).toEqual([]);
  });

  it("puts no currency, percent or multiplication sign into a cell formatter", () => {
    const formatter = production.find(([path]) => path === "./format.ts")?.[1] ?? "";
    // Units appear only in header helpers: `unitHeader` and `chooseScale` build `(RP …)` text; value formatters never do.
    const valueFormatters = formatter
      .slice(formatter.indexOf("export function formatValue"), formatter.indexOf("// ---- skala per tabel ----"))
      .replace(/\/\*[\s\S]*?\*\//g, "");
    expect(valueFormatters).not.toMatch(/["'`]\s*(Rp|%|×)/);
  });
});
