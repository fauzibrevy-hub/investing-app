import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import type { Page, Route } from "@playwright/test";

/**
 * API tiruan untuk e2e (D15: `vite preview` + `page.route`, tanpa backend): jawaban dari
 * `tests/fixtures/research/canon/*.json` (golden Riset), `tests/fixtures/thesis_golden/*.json` (golden tesis) dan
 * `e2e/fixtures/*.json` (sintetis: makro, usulan ekstraksi, portofolio saham), satu per `pathname`, dipasang lewat
 * `page.route`. Emiten tanpa golden sendiri memakai golden batubara untuk klaim, jurnal, kalender dan harga. Jalur
 * yang tidak punya jawaban dijawab 404 dan dicatat, supaya tes gagal bila layar meminta sesuatu yang tidak ditiru.
 * Tidak mengimpor apa pun dari `src/`: berkas ini hanya membaca JSON.
 */
const CANON = fileURLToPath(new URL("../../tests/fixtures/research/canon/", import.meta.url));
const THESIS = fileURLToPath(new URL("../../tests/fixtures/thesis_golden/", import.meta.url));
const LOCAL = fileURLToPath(new URL("./fixtures/", import.meta.url));

function readJson(dir: string, name: string): any {
  const file = `${dir}${name}.json`;
  if (!existsSync(file)) throw new Error(`fixture "${name}" tidak ada di ${dir}`);
  return JSON.parse(readFileSync(file, "utf8"));
}
const thesis = (name: string) => readJson(THESIS, name);
const local = (name: string) => readJson(LOCAL, name);

const golden = (name: string): unknown => readJson(CANON, name);
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
    // R8: golden canon tidak memuat usulan; jawaban sintetis dari e2e/fixtures/proposals.json.
    [`/api/research/${ticker}/proposals`]: local("proposals"),
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
  "/api/health": { status: "ok" },
  "/api/research/desk": golden("desk"),
  "/api/research/universe": golden("universe"),
  "/api/research/packs": golden("packs"),
  "/api/research/rules": golden("rules"),
  "/api/research/calibration": golden("calibration"),
  "/api/research/compare": golden("compare-coal_mining"),
  "/api/research/filings": golden("filings-COAL"),
  "/api/research/lookup": lookup,
  // Kartu konteks makro di Desk.
  "/api/macro/observations": local("macro-observations"),
  // Tesis (L3b): golden `tests/fixtures/thesis_golden/`; `/drafts` memakai bentuk `{ drafts, vault_drafts, total }`.
  "/api/thesis/summary": thesis("summary"),
  "/api/thesis/AAAA": thesis("detail_AAAA"),
  "/api/thesis/BBBB": thesis("detail_BBBB"),
  "/api/thesis/CCCC": thesis("detail_CCCC"),
  "/api/thesis/macro": thesis("macro"),
  "/api/thesis/drafts/vault": thesis("drafts_vault"),
  "/api/thesis/drafts": { drafts: [], vault_drafts: thesis("drafts_vault").vault_drafts, total: thesis("drafts_vault").vault_drafts.length },
  // Portofolio (L3c): posisi COAL dan REST, supaya halaman perusahaan posisi punya data canon.
  "/api/finance/stock-portfolio": local("stock-portfolio"),
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

/**
 * Pasang tiruan pada `page`; mengembalikan daftar `pathname` yang diminta layar tetapi tidak ditiru.
 * `health` menggantikan jawaban `/api/health` (mis. `{ status: "ok", sandbox: true }`, D7).
 */
export async function mockApi(page: Page, { health }: { health?: unknown } = {}): Promise<string[]> {
  const unmocked: string[] = [];
  await page.route("**/api/**", async (route: Route) => {
    const url = new URL(route.request().url());
    const handler = url.pathname === "/api/health" && health !== undefined ? health : (API_ROUTES[url.pathname] ?? dynamic(url.pathname));
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

/** Timpa satu jalur API di atas tiruan (rute yang dipasang belakangan dijalankan lebih dulu): status dan badan sendiri. */
export async function overrideApi(page: Page, pathname: string, answer: { status?: number; body?: unknown }) {
  await page.route(`**${pathname}`, async (route: Route) => {
    if (new URL(route.request().url()).pathname !== pathname) return route.fallback();
    return route.fulfill({ status: answer.status ?? 200, contentType: "application/json", body: JSON.stringify(answer.body ?? {}) });
  });
}
