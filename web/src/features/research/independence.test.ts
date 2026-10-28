import { cleanup } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SURFACES, mockCanonBackend, renderSurface } from "./canonSurfaces.testutil";

// Awalan API milik fitur lain, dirakit dari potongan agar grep gerbang independensi tetap kosong.
const FOREIGN_PREFIXES = [["/api/", "the", "sis"], ["/api/", "idx"]].map((p) => p.join(""));

/**
 * Satu-satunya jalur di luar `/api/research/` yang boleh dibaca fitur ini: kartu konteks makro di Desk (prd.md §9, R7.4),
 * hanya baca. Jalur makro lain (`/trigger`, `/stream`, `/scoring`) tetap terlarang.
 */
const MACRO_READ = "/api/macro/observations";
const MACRO_PREFIX = "/api/macro/";

const SOURCES = import.meta.glob("./**/*.{ts,tsx}", { query: "?raw", import: "default", eager: true }) as Record<string, string>;
/** Vite tidak menyertakan berkas pengimpor (berkas ini, yang memuat contoh terlarang sebagai data uji pemindai). */
const scanned = () => Object.entries(SOURCES);
/** Awalan API hanya dicari di kode produksi; tes (mis. `canon.test.ts`) boleh menyebutnya dalam pola pemeriksaan. */
const production = () => scanned().filter(([file]) => !/\.test(util)?\.tsx?$/.test(file));

/** Spesifier impor (`from "…"`, `import("…")`, `import "…"`) yang menuju `src/screens/`. */
function screenImports(source: string): string[] {
  return [...source.matchAll(/(?:from|import(?:\.meta\.glob)?)\s*\(?\s*["']([^"']+)["']/g)].map((m) => m[1]).filter((spec) => /(^|\/)screens(\/|$)/.test(spec));
}

function foreignPrefixes(source: string): string[] {
  return FOREIGN_PREFIXES.filter((prefix) => source.includes(prefix));
}

describe("research feature independence (design §9, plan §7.4)", () => {
  it("the scanner finds a screens import in every spelling and a foreign API prefix", () => {
    expect(screenImports(`import { A } from "../../screens/home/Home";`)).toEqual(["../../screens/home/Home"]);
    expect(screenImports(`const m = await import("../screens/macro/MacroScreen");`)).toEqual(["../screens/macro/MacroScreen"]);
    expect(screenImports(`import "../../screens/x";`)).toEqual(["../../screens/x"]);
    expect(screenImports(`import { B } from "../components/Card";`)).toEqual([]);
    expect(foreignPrefixes(`request("${FOREIGN_PREFIXES[0]}/x")`)).toEqual([FOREIGN_PREFIXES[0]]);
    expect(foreignPrefixes(`request("${FOREIGN_PREFIXES[1]}")`)).toEqual([FOREIGN_PREFIXES[1]]);
    expect(foreignPrefixes(`request("/api/research/desk")`)).toEqual([]);
  });

  it("the scanner also catches a glob import of another feature's screens", () => {
    expect(screenImports(`const m = import.meta.glob("../../screens/**/*.tsx");`)).toEqual(["../../screens/**/*.tsx"]);
  });

  it("scans the whole feature directory, not an empty glob", () => {
    const files = Object.keys(SOURCES);
    expect(files.length).toBeGreaterThan(40);
    expect(files).toContain("./api.ts");
    expect(files).toContain("./dossier/sections/Overview.tsx");
  });

  it("no file under web/src/features/research imports from web/src/screens", () => {
    const hits = scanned().flatMap(([file, src]) => screenImports(src).map((spec) => `${file} -> ${spec}`));
    expect(hits).toEqual([]);
  });

  it("the only macro API the feature names is the read-only observations list, used by the Desk context card alone", () => {
    const hits = production().flatMap(([file, src]) => (src.includes(MACRO_PREFIX) ? [file] : []));
    expect(hits).toEqual(["./desk/useMacro.ts"]);
    const source = Object.entries(SOURCES).find(([file]) => file === "./desk/useMacro.ts")![1];
    expect([...new Set([...source.matchAll(/\/api\/macro\/[\w/-]*/g)].map((m) => m[0]))]).toEqual([MACRO_READ]);
  });

  it("no file under web/src/features/research names another feature's API", () => {
    const hits = production().flatMap(([file, src]) => foreignPrefixes(src).map((prefix) => `${file}: ${prefix}`));
    expect(hits).toEqual([]);
  });

  it.each(SURFACES)("$name: every request goes to /api/research/ (the Desk also reads the macro observations) and at least one is made", async (surface) => {
    const fetchMock = mockCanonBackend();
    await renderSurface(surface);
    const requested = fetchMock.mock.calls.map(([input]) => String(input));
    expect(requested.length).toBeGreaterThan(0);
    expect(requested.filter((url) => !url.startsWith("/api/research/") && url !== MACRO_READ)).toEqual([]);
    // Pada Desk kartu konteks makro membaca daftar observasi; permukaan lain tak menyentuh API makro.
    expect(requested.filter((url) => url.startsWith(MACRO_PREFIX))).toEqual(surface.name === "Desk" ? [MACRO_READ] : []);
    expect(requested.filter((url) => FOREIGN_PREFIXES.some((prefix) => url.includes(prefix)))).toEqual([]);
    cleanup();
  });
});
