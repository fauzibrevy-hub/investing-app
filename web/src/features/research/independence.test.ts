import { cleanup } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SURFACES, mockCanonBackend, renderSurface } from "./canonSurfaces.testutil";

// Awalan API milik fitur lain, dirakit dari potongan agar grep gerbang independensi tetap kosong.
const FOREIGN_PREFIXES = [["/api/", "the", "sis"], ["/api/", "idx"]].map((p) => p.join(""));

/**
 * Satu-satunya jalur di luar `/api/research/` yang boleh dibaca fitur ini: kisi Makro di Beranda (prd.md §9, R7.4),
 * hanya baca. Jalur makro lain (`/trigger`, `/stream`, `/scoring`) tetap terlarang.
 */
const MACRO_READ = "/api/macro/observations";
const MACRO_PREFIX = "/api/macro/";

const SOURCES = import.meta.glob("./**/*.{ts,tsx}", { query: "?raw", import: "default", eager: true }) as Record<string, string>;
/** Vite tidak menyertakan berkas pengimpor (berkas ini, yang memuat contoh terlarang sebagai data uji pemindai). */
const scanned = () => Object.entries(SOURCES);
/** Awalan API hanya dicari di kode produksi; tes (mis. `canon.test.ts`) boleh menyebutnya dalam pola pemeriksaan. */
const production = () => scanned().filter(([file]) => !/\.test(util)?\.tsx?$/.test(file));

/**
 * Kode layar milik fitur lain (di luar folder ini) boleh membaca API fiturnya sendiri; yang dilarang adalah kode di folder
 * ini. Maka awalan API asing yang boleh diminta sebuah permukaan diturunkan dari lokasi kodenya: modul yang diimpor
 * (langsung atau tidak) oleh komponen rute permukaan itu di `App.tsx`, di luar `features/research/`, dan yang menyebut
 * awalan itu. Tidak ada daftar putih per nama permukaan atau per URL: Beranda yang meminta awalan itu tetap gagal.
 */
const APP_SOURCES = import.meta.glob("../../**/*.{ts,tsx}", { query: "?raw", import: "default", eager: true }) as Record<string, string>;
const APP = new Map(Object.entries(APP_SOURCES).filter(([file]) => !/\.test(util)?\.tsx?$/.test(file)).map(([file, src]) => [new URL(file, "file:///features/research/independence.test.ts").pathname.slice(1), src]));

function resolveModule(from: string, spec: string): string | null {
  const base = new URL(spec, `file:///${from}`).pathname.slice(1);
  return [base, `${base}.ts`, `${base}.tsx`, `${base}/index.ts`, `${base}/index.tsx`].find((candidate) => APP.has(candidate)) ?? null;
}

/** Semua modul yang terjangkau lewat impor relatif dari `entry` (termasuk `entry`). */
function closure(entry: string): Set<string> {
  const seen = new Set<string>();
  const todo = [entry];
  while (todo.length) {
    const file = todo.pop()!;
    if (seen.has(file) || !APP.has(file)) continue;
    seen.add(file);
    for (const m of APP.get(file)!.matchAll(/(?:from|import)\s*\(?\s*["'](\.[^"']*)["']/g)) {
      const next = resolveModule(file, m[1]);
      if (next) todo.push(next);
    }
  }
  return seen;
}

/** Modul komponen rute untuk sebuah jalur permukaan, dari tabel rute `App.tsx` (rute pertama yang cocok, `*` dilewati). */
function routedModule(path: string): string | null {
  const app = APP.get("App.tsx")!;
  const pathname = path.split("?")[0];
  for (const m of app.matchAll(/<Route path="([^"*][^"]*)" element=\{<(\w+)/g)) {
    if (!new RegExp(`^${m[1].replace(/:[^/]+/g, "[^/]+")}$`).test(pathname)) continue;
    const imported = [...app.matchAll(/import \{([^}]+)\} from "(\.[^"]+)"/g)].find((i) => i[1].split(",").map((n) => n.trim()).includes(m[2]));
    return imported ? resolveModule("App.tsx", imported[2]) : null;
  }
  return null;
}

/** Awalan API asing yang disebut kode di luar folder ini dalam jangkauan komponen rute permukaan itu. */
function ownForeignPrefixes(path: string): string[] {
  const entry = routedModule(path);
  if (!entry) return [];
  return FOREIGN_PREFIXES.filter((prefix) => [...closure(entry)].some((file) => !file.startsWith("features/research/") && APP.get(file)!.includes(prefix)));
}

/** Permintaan ke awalan asing yang tidak berasal dari kode di luar folder ini. */
function foreignViolations(requested: string[], allowed: string[]): string[] {
  return requested.filter((url) => FOREIGN_PREFIXES.some((prefix) => url.includes(prefix)) && !allowed.some((prefix) => url.startsWith(prefix)));
}

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
    expect(files.length).toBeGreaterThan(50);
    expect(files).toContain("./api.ts");
    expect(files).toContain("./components/FactDrawer.tsx");
  });

  it("no file under web/src/features/research imports from web/src/screens", () => {
    const hits = scanned().flatMap(([file, src]) => screenImports(src).map((spec) => `${file} -> ${spec}`));
    expect(hits).toEqual([]);
  });

  it("the only macro API the feature names is the read-only observations list, read by Beranda's Makro grid through `useMacro` alone", () => {
    const hits = production().flatMap(([file, src]) => (src.includes(MACRO_PREFIX) ? [file] : []));
    expect(hits).toEqual(["./desk/useMacro.ts"]);
    const source = Object.entries(SOURCES).find(([file]) => file === "./desk/useMacro.ts")![1];
    expect([...new Set([...source.matchAll(/\/api\/macro\/[\w/-]*/g)].map((m) => m[0]))]).toEqual([MACRO_READ]);
  });

  it("no file under web/src/features/research names another feature's API", () => {
    const hits = production().flatMap(([file, src]) => foreignPrefixes(src).map((prefix) => `${file}: ${prefix}`));
    expect(hits).toEqual([]);
  });

  it("each surface's foreign API prefixes come from its own non-research modules, found by following the route's imports", () => {
    const allowed = Object.fromEntries(SURFACES.map((surface) => [surface.name, ownForeignPrefixes(surface.path)]));
    for (const surface of SURFACES) expect(routedModule(surface.path), `${surface.name}: komponen rute tidak ditemukan`).not.toBeNull();
    // Hanya Perusahaan (blok Tesis lain) dan halaman perusahaan (blok Tesis) memuat kode fitur lain yang membaca API-nya.
    const own = SURFACES.filter((surface) => allowed[surface.name].length > 0).map((surface) => surface.name);
    expect(own).toContain("Perusahaan");
    for (const surface of SURFACES.filter((s) => ["Beranda", "Alat", "Compare", "Calibration", "Packs", "Tambah"].includes(s.name))) expect(allowed[surface.name], surface.name).toEqual([]);
  });

  it("a foreign API request fails unless the surface's own non-research code names that prefix (no exemption by URL)", () => {
    const url = `${FOREIGN_PREFIXES[0]}/summary`;
    expect(foreignViolations([url, "/api/research/desk"], [])).toEqual([url]);
    expect(foreignViolations([url], [FOREIGN_PREFIXES[0]])).toEqual([]);
    expect(foreignViolations([`${FOREIGN_PREFIXES[1]}/x`], [FOREIGN_PREFIXES[0]])).toEqual([`${FOREIGN_PREFIXES[1]}/x`]);
    expect(ownForeignPrefixes("/beranda")).toEqual([]);
  });

  it.each(SURFACES)("$name: every request goes to /api/research/ (Beranda also reads the macro observations) unless its own non-research code reads another API; at least one request is made", async (surface) => {
    const fetchMock = mockCanonBackend();
    await renderSurface(surface);
    const requested = fetchMock.mock.calls.map(([input]) => String(input));
    expect(requested.length).toBeGreaterThan(0);
    const allowed = ownForeignPrefixes(surface.path);
    expect(requested.filter((url) => !url.startsWith("/api/research/") && url !== MACRO_READ && !allowed.some((prefix) => url.startsWith(prefix)))).toEqual([]);
    // Pada Beranda kisi Makro membaca daftar observasi; permukaan lain tak menyentuh API makro.
    expect(requested.filter((url) => url.startsWith(MACRO_PREFIX))).toEqual(surface.name === "Beranda" ? [MACRO_READ] : []);
    expect(foreignViolations(requested, allowed)).toEqual([]);
    cleanup();
  });
});
