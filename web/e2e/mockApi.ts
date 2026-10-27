import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import type { Page, Route } from "@playwright/test";
import { MACRO } from "../../src/features/research/desk/macro.testutil";
import { proposalsResponse } from "../../src/features/research/testing/extract";

/**
 * API tiruan untuk e2e Riset: jawaban dari `tests/fixtures/research/canon/*.json` (golden backend), satu per `pathname`,
 * dipasang lewat `page.route`. Sama pemetaannya dengan `canonSurfaces.testutil.tsx` (vitest); emiten tanpa golden
 * sendiri memakai golden batubara untuk klaim, jurnal, kalender dan harga. Jalur yang tidak punya jawaban dijawab 404
 * dan dicatat, supaya tes gagal bila layar meminta sesuatu yang tidak ditiru.
 */
const CANON = fileURLToPath(new URL("../../../tests/fixtures/research/canon/", import.meta.url));

function golden(name: string): unknown {
  const file = `${CANON}${name}.json`;
  if (!existsSync(file)) throw new Error(`golden canon "${name}" tidak ada di tests/fixtures/research/canon`);
  return JSON.parse(readFileSync(file, "utf8"));
}
const has = (name: string) => existsSync(`${CANON}${name}.json`);

type Handler = unknown | ((url: URL) => unknown);

function tickerRoutes(ticker: string): Record<string, Handler> {
  const own = (name: string, fallback: string) => golden(has(name) ? name : fallback);
  return {
    [`/api/research/${ticker}`]: golden(`ticker-${ticker}`),
    [`/api/research/${ticker}/pack`]: own(`pack-${ticker}`, "pack-COAL"),
    [`/api/research/${ticker}/claims`]: golden("claims-COAL"),
    [`/api/research/${ticker}/journal`]: golden("journal-COAL"),
    [`/api/research/${ticker}/documents`]: own(`documents-${ticker}`, "documents-COAL"),
    // R8: golden canon tidak memuat usulan; jawaban sintetis dari testing/extract.ts.
    [`/api/research/${ticker}/proposals`]: proposalsResponse(),
    [`/api/research/${ticker}/seen`]: {},
    [`/api/research/${ticker}/calendar`]: golden("calendar-COAL"),
    [`/api/research/${ticker}/valuation`]: own(`valuation-${ticker}`, "valuation-COAL"),
    [`/api/research/${ticker}/prices`]: golden("prices-COAL-1y"),
    [`/api/research/${ticker}/financials`]: (u: URL) => {
      const view = u.searchParams.get("view") ?? "standalone";
      return own(`financials-${ticker}-${view}`, `financials-COAL-${view}`);
    },
  };
}

/** Wizard tambah: golden `/lookup` tidak ada (endpoint baru R6.3), jadi jawabannya sintetis dan sesuai `LookupResponse`. */
function lookup(u: URL) {
  const q = (u.searchParams.get("q") ?? "").trim().toUpperCase();
  const item = (ticker: string, name: string, inUniverse = false) => ({
    ticker,
    name,
    listed: true,
    industry_code: "B055",
    pack: { id: "coal_mining", label: "Pertambangan batubara" },
    in_universe: inUniverse,
    role: inUniverse ? "manual" : null,
  });
  const exact = /^[A-Z]{4}$/.test(q) ? item(q, "Contoh Tambang Tbk") : null;
  return {
    as_of: "2026-10-28T03:00:00+00:00",
    query: q,
    list_available: true,
    exact,
    items: [item("COAL", "Batubara Contoh Tbk", true), item("CTOH", "Contoh Tambang Tbk"), item("TMBG", "Tambang Gamma Tbk")].filter((i) => !q || i.ticker.includes(q) || (i.name ?? "").toUpperCase().includes(q)),
  };
}

export const API_ROUTES: Record<string, Handler> = {
  "/api/session": {},
  "/api/research/desk": golden("desk"),
  "/api/research/universe": golden("universe"),
  "/api/research/packs": golden("packs"),
  "/api/research/rules": golden("rules"),
  "/api/research/calibration": golden("calibration"),
  "/api/research/compare": golden("compare-coal_mining"),
  "/api/research/filings": golden("filings-COAL"),
  "/api/research/lookup": lookup,
  // Kartu konteks makro di Desk.
  "/api/macro/observations": MACRO,
  ...tickerRoutes("COAL"),
  ...tickerRoutes("REST"),
  ...tickerRoutes("NONE"),
};

/** Jalur dengan id di tengah: evaluasi dan bukti klaim, detail fakta. */
function dynamic(pathname: string): Handler | undefined {
  if (/^\/api\/research\/claims\/[^/]+\/evals$/.test(pathname)) return golden("claim-evals-c1");
  const evidence = /^\/api\/research\/claims\/([^/]+)\/evidence$/.exec(pathname);
  if (evidence) return { claim_id: evidence[1], ticker: "COAL", items: [] };
  if (/^\/api\/research\/facts\//.test(pathname)) return golden("facts-COAL");
  return undefined;
}

/** Pasang tiruan pada `page`; mengembalikan daftar `pathname` yang diminta layar tetapi tidak ditiru. */
export async function mockResearchApi(page: Page): Promise<string[]> {
  const unmocked: string[] = [];
  await page.route("**/api/**", async (route: Route) => {
    const url = new URL(route.request().url());
    const handler = API_ROUTES[url.pathname] ?? dynamic(url.pathname);
    if (handler === undefined) {
      unmocked.push(url.pathname);
      return route.fulfill({ status: 404, contentType: "application/json", body: JSON.stringify({ detail: "not mocked" }) });
    }
    // Tulis (POST/PUT/DELETE) tidak dipakai oleh pemeriksaan ini; jawab 200 kosong.
    const body = typeof handler === "function" ? (handler as (u: URL) => unknown)(url) : handler;
    return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(body) });
  });
  return unmocked;
}
