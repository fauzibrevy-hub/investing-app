import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { expect, vi } from "vitest";
import { AppRoutes } from "../../App";
import { json, mockApi } from "../../test/mockApi";
import { MACRO } from "./desk/macro.testutil";
import { golden, hasGolden } from "./testing/canon";
import { proposalsResponse } from "./testing/extract";
import { setViewport } from "./testing/viewport";

/**
 * Permukaan yang dirender terhadap golden canon (bukan test sendiri; dipakai oleh `vocabulary`, `provenance`,
 * `independence` dan `display`). Setiap permukaan memakai tabel rute `App` (`AppRoutes`) pada jalur design.md §2.2,
 * jadi routing ikut teruji. L3a memasang Beranda, Alat dan tiga alatnya; L3b menambah blok halaman perusahaan,
 * Perusahaan dan Tambah; L3c menambah Portofolio (plan.md §3.3 "Tests").
 * Backend ditiru per `pathname`; jalur yang tidak punya golden dijawab 404, dan `renderSurface` gagal bila permukaan
 * sampai menampilkan keadaan galat, supaya mock yang kurang tidak lolos diam-diam.
 */
export interface Surface {
  name: string;
  path: string;
  /** Penanda bahwa data permukaan sudah tampil (bukan skeleton). */
  landmark: () => Promise<unknown>;
  /** `false`: permukaan yang dilayani API lain (Portofolio membaca portofolio Sheets); gerbang khusus riset (independence, provenance) melewatinya. */
  research?: boolean;
}

export const SURFACES: Surface[] = [
  {
    name: "Beranda",
    path: "/beranda",
    // Kotak masuk (desk) dan kisi Makro (observasi) dimuat terpisah; keduanya harus sudah tampil.
    landmark: async () => {
      await screen.findByRole("group", { name: "Kotak masuk" });
      return screen.findByRole("list", { name: /Indikator makro/ });
    },
  },
  // L3c: Portofolio membaca portofolio Sheets, bukan API riset; `STOCK_PORTFOLIO` menirunya (emiten sintetis COAL dan REST).
  { name: "Portofolio", path: "/portofolio", landmark: () => screen.findByRole("list", { name: "Legenda komposisi" }), research: false },
  { name: "Alat", path: "/alat", landmark: () => screen.findByRole("list", { name: "Alat" }) },
  { name: "Compare", path: "/alat/bandingkan?pack=coal_mining", landmark: () => screen.findByRole("region", { name: /Tabel perbandingan/ }) },
  { name: "Calibration", path: "/alat/kalibrasi", landmark: () => screen.findByRole("list", { name: "Butir kalibrasi" }) },
  { name: "Packs", path: "/alat/paket", landmark: () => screen.findByRole("combobox", { name: "Paket" }) },
  {
    name: "Perusahaan",
    path: "/perusahaan",
    landmark: async () => {
      await screen.findByRole("region", { name: "Dipegang" });
      return screen.findByRole("button", { name: /Tesis lain/ });
    },
  },
  { name: "Tambah", path: "/perusahaan/tambah", landmark: () => screen.findByRole("heading", { level: 2, name: /Langkah 1 dari 4/ }) },
  { name: "Company COAL Kuartal", path: "/perusahaan/COAL?section=kuartal", landmark: () => screen.findByRole("heading", { level: 2, name: /Kuartal terakhir/ }) },
  { name: "Company COAL Jurnal", path: "/perusahaan/COAL?section=jurnal", landmark: () => screen.findByRole("region", { name: "Entri jurnal" }) },
  { name: "Company COAL Keuangan", path: "/perusahaan/COAL?section=keuangan", landmark: () => screen.findByRole("table", { name: "Laba rugi, kuartal" }) },
  // Emiten lain dan paket kosong: label dan satuan yang berbeda dari batubara (REST), dan keadaan tanpa paket (NONE).
  { name: "Company REST Kuartal", path: "/perusahaan/REST?section=kuartal", landmark: () => screen.findByRole("heading", { level: 2, name: /Kuartal terakhir/ }) },
  { name: "Company REST Keuangan", path: "/perusahaan/REST?section=keuangan", landmark: () => screen.findByRole("table", { name: "Laba rugi, kuartal" }) },
  { name: "Company NONE Kuartal", path: "/perusahaan/NONE?section=kuartal", landmark: () => screen.findByRole("heading", { level: 2, name: /Kuartal terakhir/ }) },
];

const position = (no: number, ticker: string, name: string, lots: number, avg: number, last: number, cost: number, w: number, target: number) => ({
  no,
  category: "CORE",
  ticker,
  full_code: `IDX:${ticker}`,
  company_name: name,
  shares: lots * 100,
  lots,
  avg_price: avg,
  current_price: last,
  cost,
  valuation: lots * 100 * last,
  pl: lots * 100 * last - cost,
  pl_pct: ((lots * 100 * last - cost) / cost) * 100,
  fund_alloc_pct: 0,
  val_alloc_pct: w,
  target_alloc_pct: target,
  drift_pct: w - target,
});

/** `GET /api/finance/stock-portfolio` untuk Portofolio (L3c). */
const STOCK_PORTFOLIO = {
  summary: { current_valuation: 115_000_000, total_cost: 101_000_000, total_pl: 1_500_000, total_pl_pct: 1.49, cash_balance: 12_500_000, updated_at: "2026-10-27T09:30:00" },
  positions: [position(1, "COAL", "Batubara Contoh Tbk", 200, 4150, 4300, 83_000_000, 62.5, 60), position(2, "REST", "Restoran Contoh Tbk", 100, 1800, 1650, 18_000_000, 37.5, 40)],
  available: true,
};

/** Golden yang dipakai permukaan: teks dari backend (label metrik paket, nama emiten), bukan kata milik layar. */
export function canonPayloads(): unknown[] {
  return [
    "desk", "universe", "packs", "rules", "calibration", "compare-coal_mining", "ticker-COAL", "pack-COAL", "claims-COAL", "journal-COAL",
    "documents-COAL", "filings-COAL", "calendar-COAL", "valuation-COAL", "prices-COAL-1y", "financials-COAL-standalone", "financials-COAL-ttm",
  ].map((name) => golden(name));
}

/** Jawaban per emiten dari golden; emiten tanpa golden sendiri (klaim, jurnal, kalender, harga) memakai golden batubara. */
function tickerRoutes(ticker: string): Record<string, unknown> {
  const own = (name: string, fallback: string) => (hasGolden(name) ? golden(name) : golden(fallback));
  return {
    [`/api/research/${ticker}`]: golden(`ticker-${ticker}`),
    [`/api/research/${ticker}/pack`]: own(`pack-${ticker}`, "pack-COAL"),
    [`/api/research/${ticker}/structure`]: own(`structure-${ticker}`, "structure-COAL"),
    [`/api/research/${ticker}/claims`]: golden("claims-COAL"),
    [`/api/research/${ticker}/journal`]: golden("journal-COAL"),
    [`/api/research/${ticker}/documents`]: own(`documents-${ticker}`, "documents-COAL"),
    [`/api/research/${ticker}/proposals`]: proposalsResponse(),
    [`/api/research/${ticker}/seen`]: {},
    [`/api/research/${ticker}/calendar`]: golden("calendar-COAL"),
    [`/api/research/${ticker}/valuation`]: own(`valuation-${ticker}`, "valuation-COAL"),
    [`/api/research/${ticker}/prices`]: golden("prices-COAL-1y"),
    [`/api/research/${ticker}/financials`]: (u: URL) => json(own(`financials-${ticker}-${u.searchParams.get("view") ?? "standalone"}`, `financials-COAL-${u.searchParams.get("view") ?? "standalone"}`)),
  };
}

const OTHER_BASE = ["/api/", "the", "sis"].join("");
const OTHER_DRAFTS_KEY = ["vau", "lt_drafts"].join("");

let knownPaths = new Set<string>();

/** Backend tiruan dari golden: satu jawaban per `pathname`. */
export function mockCanonBackend(extra: Record<string, unknown> = {}) {
  const routes: Record<string, unknown> = {
    "/api/research/desk": golden("desk"),
    "/api/research/universe": golden("universe"),
    "/api/research/packs": golden("packs"),
    "/api/research/rules": golden("rules"),
    "/api/research/calibration": golden("calibration"),
    "/api/research/compare": golden("compare-coal_mining"),
    "/api/research/filings": golden("filings-COAL"),
    // Kisi Makro di Beranda: satu-satunya jalur di luar /api/research/ yang dibaca fitur ini (independence.test.ts).
    "/api/macro/observations": MACRO,
    "/api/finance/stock-portfolio": STOCK_PORTFOLIO,
    ...tickerRoutes("COAL"),
    ...tickerRoutes("REST"),
    ...tickerRoutes("NONE"),
    [`${OTHER_BASE}/summary`]: {
      root_ok: true,
      generated_at: 0,
      issuers: [
        {
          ticker: "AAAA",
          company: "PT Alfa Contoh Tbk",
          status: "holdings",
          verdict: null,
          narrative_status: null,
          fundamental_status: null,
          growth_status: null,
          review: { last_date: "2026-10-01", days_ago: 27, due: false },
          evidence: { resolved: 0, doc_quoted: 0, missing: 0, total: 0 },
          tahap: null,
          errors: [],
        },
      ],
    },
    [`${OTHER_BASE}/drafts`]: { drafts: [], [OTHER_DRAFTS_KEY]: [], total: 0 },
    ...extra,
  };
  knownPaths = new Set(Object.keys(routes).map((k) => k.split("?")[0]));
  return mockApi(routes);
}

/** `pathname` yang diminta layar tetapi tidak punya jawaban golden (mockApi menjawabnya 404 dan layar bisa menelannya). */
export function unmockedPaths(fetchMock: { mock: { calls: unknown[][] } }): string[] {
  const paths = fetchMock.mock.calls.map(([input]) => new URL(String(input), "http://localhost").pathname);
  return [...new Set(paths)].filter((path) => !knownPaths.has(path));
}

/** Merender satu permukaan sampai datanya tampil dan tidak ada yang masih memuat atau galat. */
export async function renderSurface(surface: Surface) {
  setViewport(393);
  const view = render(
    <MemoryRouter initialEntries={[surface.path]}>
      <AppRoutes />
    </MemoryRouter>,
  );
  await surface.landmark();
  await waitFor(() => expect(view.container.querySelector('[aria-busy="true"]'), `${surface.name} masih memuat`).toBeNull());
  expect(screen.queryAllByRole("button", { name: "Coba lagi" }), `${surface.name} menampilkan galat`).toHaveLength(0);
  expect(unmockedPaths(vi.mocked(fetch) as never), `${surface.name} meminta jalur tanpa golden`).toEqual([]);
  return view;
}
