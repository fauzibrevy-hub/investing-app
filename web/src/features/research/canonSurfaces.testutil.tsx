import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, expect, vi } from "vitest";
import { json, mockApi } from "../../test/mockApi";
import { MACRO } from "./desk/macro.testutil";
import { ResearchRoutes } from "./routes";
import { DOSSIER_SECTIONS } from "./shortcuts";
import { golden, hasGolden } from "./testing/canon";
import { proposalsResponse } from "./testing/extract";

/**
 * Permukaan Riset yang dirender terhadap golden canon (bukan test sendiri; dipakai oleh `vocabulary`, `provenance`,
 * `independence` dan `display`). Setiap permukaan memakai `ResearchRoutes` pada jalurnya, jadi routing ikut teruji.
 * Backend ditiru per `pathname`; jalur yang tidak punya golden dijawab 404, dan `renderSurface` gagal bila permukaan
 * sampai menampilkan keadaan galat, supaya mock yang kurang tidak lolos diam-diam.
 */
export interface Surface {
  name: string;
  path: string;
  /** Penanda bahwa data permukaan sudah tampil (bukan skeleton). */
  landmark: () => Promise<unknown>;
}

const SECTION_LANDMARK: Record<string, () => Promise<unknown>> = {
  ringkasan: () => screen.findByRole("region", { name: "Apa yang berubah" }),
  paket: () => screen.findAllByRole("table"),
  keuangan: () => screen.findAllByRole("table"),
  valuasi: () => screen.findByRole("table", { name: /Skenario nilai per saham/ }),
  harga: () => screen.findByRole("img", { name: /harga/i }),
  sumber: () => screen.findByRole("heading", { name: /Sumber/ }),
};

export const SURFACES: Surface[] = [
  { name: "Desk", path: "/research", landmark: () => screen.findByRole("region", { name: "Papan cakupan" }) },
  ...DOSSIER_SECTIONS.map((s): Surface => ({ name: `Dossier ${s.label}`, path: `/research/COAL?section=${s.id}`, landmark: SECTION_LANDMARK[s.id] })),
  { name: "Compare", path: "/research/compare?pack=coal_mining", landmark: () => screen.findByRole("region", { name: /Tabel perbandingan/ }) },
  { name: "Calibration", path: "/research/compare/calibration", landmark: () => screen.findByRole("region", { name: /Tabel kalibrasi/ }) },
  { name: "Packs", path: "/research/packs", landmark: () => screen.findByRole("navigation", { name: "Daftar paket" }) },
  // Emiten lain dan paket kosong: label dan satuan yang berbeda dari batubara, dan keadaan tanpa paket (NONE).
  { name: "Dossier REST Keuangan", path: "/research/REST?section=keuangan", landmark: SECTION_LANDMARK.keuangan },
  { name: "Dossier REST Paket", path: "/research/REST?section=paket", landmark: SECTION_LANDMARK.paket },
  { name: "Dossier REST Valuasi", path: "/research/REST?section=valuasi", landmark: () => screen.findAllByRole("region", { name: "Valuasi" }) },
  { name: "Dossier NONE Paket", path: "/research/NONE?section=paket", landmark: () => screen.findByRole("heading", { name: "Tanpa paket" }) },
];

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
    // Kartu konteks makro di Desk: satu-satunya jalur di luar /api/research/ yang dibaca fitur ini (independence.test.ts).
    "/api/macro/observations": MACRO,
    ...tickerRoutes("COAL"),
    ...tickerRoutes("REST"),
    ...tickerRoutes("NONE"),
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

/** Lebar ≥ 1440 px (pane Klaim & Jurnal terpasang dan terlihat) atau sempit (strip dan overlay); jsdom tidak punya layout. */
export function setViewport(wide: boolean) {
  window.matchMedia = ((query: string) =>
    ({
      matches: wide && /min-width:\s*(1024|1440)px/.test(query),
      media: query,
      onchange: null,
      addEventListener() {},
      removeEventListener() {},
      addListener() {},
      removeListener() {},
      dispatchEvent: () => false,
    }) as MediaQueryList) as typeof window.matchMedia;
}

// Viewport lebar hanya berlaku di dalam test yang merender permukaan; layar lain (mis. App penuh) kembali ke stub setup.
afterEach(() => setViewport(false));

/** Merender satu permukaan sampai datanya tampil dan tidak ada yang masih memuat atau galat. */
export async function renderSurface(surface: Surface, { wide = true }: { wide?: boolean } = {}) {
  setViewport(wide);
  const view = render(
    <MemoryRouter initialEntries={[surface.path]}>
      <Routes>
        <Route path="/research/*" element={<ResearchRoutes />} />
      </Routes>
    </MemoryRouter>,
  );
  await surface.landmark();
  await waitFor(() => expect(view.container.querySelector('[aria-busy="true"]'), `${surface.name} masih memuat`).toBeNull());
  expect(screen.queryAllByRole("button", { name: "Coba lagi" }), `${surface.name} menampilkan galat`).toHaveLength(0);
  expect(unmockedPaths(vi.mocked(fetch) as never), `${surface.name} meminta jalur tanpa golden`).toEqual([]);
  return view;
}
