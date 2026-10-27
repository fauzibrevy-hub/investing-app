import { describe, expect, it } from "vitest";
import manifestRaw from "../../public/manifest.webmanifest?raw";
import { CACHE_DATE_HEADER as API_HEADER } from "../lib/api";
import { CACHE_DATE_HEADER, isCacheable, keysToEvict, strategyFor, staleApiCaches, API_CACHE } from "./swPolicy";

const O = "https://mac.tailnet.ts.net";
const req = (method: string, path: string, extra: { mode?: string; accept?: string } = {}) => ({
  method,
  url: path.startsWith("http") ? path : `${O}${path}`,
  mode: extra.mode,
  headers: { get: (n: string) => (n === "Accept" ? (extra.accept ?? null) : null) },
});

describe("strategi service worker (design.md §10)", () => {
  it("GET /api/* → network-first", () => {
    expect(strategyFor(req("GET", "/api/finance/summary"), O)).toBe("api-network-first");
    expect(strategyFor(req("GET", "/api/finance/transactions?limit=200"), O)).toBe("api-network-first");
  });

  it("tulis tidak pernah di-cache/di-queue", () => {
    for (const m of ["POST", "PUT", "PATCH", "DELETE"]) {
      expect(strategyFor(req(m, "/api/finance/transaction"), O)).toBe("passthrough");
    }
  });

  it("sesi dan SSE tidak disentuh (token tidak pernah masuk Cache Storage)", () => {
    expect(strategyFor(req("POST", "/api/session"), O)).toBe("passthrough");
    expect(strategyFor(req("GET", "/api/session"), O)).toBe("passthrough");
    expect(strategyFor(req("GET", "/api/macro/stream"), O)).toBe("passthrough");
    expect(strategyFor(req("GET", "/api/x", { accept: "text/event-stream" }), O)).toBe("passthrough");
  });

  it("navigasi dan aset shell", () => {
    expect(strategyFor(req("GET", "/", { mode: "navigate" }), O)).toBe("navigation");
    expect(strategyFor(req("GET", "/assets/index-abc.js"), O)).toBe("shell-cache-first");
    expect(strategyFor(req("GET", "https://fonts.example.com/x.woff2"), O)).toBe("passthrough");
  });

  it("hanya 200 yang di-cache", () => {
    expect(isCacheable({ ok: true, status: 200, type: "basic" })).toBe(true);
    expect(isCacheable({ ok: false, status: 401, type: "basic" })).toBe(false);
    expect(isCacheable({ ok: true, status: 204, type: "basic" })).toBe(false);
    expect(isCacheable({ ok: false, status: 503 })).toBe(false);
  });

  it("cache API dibatasi 50 entri (buang yang tertua)", () => {
    const keys = Array.from({ length: 53 }, (_, i) => i);
    expect(keysToEvict(keys)).toEqual([0, 1, 2]);
    expect(keysToEvict(keys.slice(0, 50))).toEqual([]);
  });

  it("header stempel cache sama dengan yang dibaca klien API", () => {
    expect(CACHE_DATE_HEADER).toBe(API_HEADER);
  });
});

describe("manifest (design.md §10)", () => {
  const manifest = JSON.parse(manifestRaw);
  it("field sesuai tabel", () => {
    expect(manifest).toMatchObject({
      name: "Finance",
      short_name: "Finance",
      id: "/",
      start_url: "/?source=pwa",
      scope: "/",
      display: "standalone",
      background_color: "#FFFFFF",
      theme_color: "#FFFFFF",
      lang: "id",
    });
    expect(manifest.orientation).toBeUndefined();
    expect(manifest.display_override).toBeUndefined();
    expect(manifest.icons.map((i: { src: string; sizes: string; purpose: string }) => `${i.src} ${i.sizes} ${i.purpose}`)).toEqual([
      "/icon-192.png 192x192 any",
      "/icon-512.png 512x512 any",
      "/icon-maskable-512.png 512x512 maskable",
    ]);
  });
});

describe("staleApiCaches", () => {
  it("API_CACHE bukan v1 lagi", () => {
    expect(API_CACHE).not.toBe("investing-api-v1");
  });
  it("membuang semua investing-api-* lama, menyisakan yang aktif dan cache shell", () => {
    const names = ["investing-api-v1", "investing-api-v0", API_CACHE, "investing-shell-abc", "other"];
    expect(staleApiCaches(names)).toEqual(["investing-api-v1", "investing-api-v0"]);
  });
});
