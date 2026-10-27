/// <reference lib="webworker" />
/**
 * Service worker Investasi (design.md §4). Dibundel oleh plugin `investingServiceWorker` di vite.config.ts
 * menjadi /sw.js; `__PRECACHE__` dan `__SW_VERSION__` diisi dari hasil build.
 *
 * Token INVESTING_APP_TOKEN tidak pernah lewat sini: POST /api/session tidak diintersep (passthrough),
 * dan yang di-cache hanya body respons GET /api/*.
 */
import { API_CACHE, API_TIMEOUT_MS, CACHE_DATE_HEADER, isCacheable, keysToEvict, SHELL_CACHE_PREFIX, staleApiCaches, strategyFor } from "./pwa/swPolicy";

declare const self: ServiceWorkerGlobalScope;
declare const __PRECACHE__: string[];
declare const __SW_VERSION__: string;

const SHELL_CACHE = `${SHELL_CACHE_PREFIX}${__SW_VERSION__}`;
const APP_SHELL_URL = "/index.html";

self.addEventListener("install", (event) => {
  // Tanpa skipWaiting: versi baru aktif setelah semua tab ditutup, atau saat pengguna mengetuk "Muat ulang".
  event.waitUntil(caches.open(SHELL_CACHE).then((cache) => cache.addAll(__PRECACHE__)));
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const names = await caches.keys();
      await Promise.all(names.filter((n) => n.startsWith(SHELL_CACHE_PREFIX) && n !== SHELL_CACHE).map((n) => caches.delete(n)));
      await Promise.all(staleApiCaches(names).map((n) => caches.delete(n)));
      await self.clients.claim();
    })(),
  );
});

self.addEventListener("message", (event) => {
  if (event.data && event.data.type === "SKIP_WAITING") void self.skipWaiting();
});

self.addEventListener("fetch", (event) => {
  const req = event.request;
  switch (strategyFor(req, self.location.origin)) {
    case "api-network-first":
      event.respondWith(apiNetworkFirst(req));
      return;
    case "navigation":
      event.respondWith(navigation(req));
      return;
    case "shell-cache-first":
      event.respondWith(shellCacheFirst(req));
      return;
    default:
      return; // passthrough: browser menangani langsung
  }
});

async function apiNetworkFirst(req: Request): Promise<Response> {
  const cache = await caches.open(API_CACHE);
  const network = fetch(req).then(async (res) => {
    if (isCacheable(res)) await store(cache, req, res.clone());
    return res;
  });
  // Jika cache dipakai setelah timeout, kegagalan jaringan belakangan tidak boleh jadi unhandled rejection.
  network.catch(() => {});

  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<"timeout">((resolve) => {
    timer = setTimeout(() => resolve("timeout"), API_TIMEOUT_MS);
  });

  try {
    const winner = await Promise.race([network, timeout]);
    if (winner !== "timeout") return winner;
    // Lewat 4 s: pakai cache terakhir jika ada, kalau tidak tunggu jaringan.
    const cached = await cache.match(req);
    return cached ?? (await network);
  } catch {
    const cached = await cache.match(req);
    return cached ?? Response.error();
  } finally {
    clearTimeout(timer);
  }
}

/** Simpan salinan dengan stempel waktu agar UI bisa menampilkan "Offline · data per <waktu>". */
async function store(cache: Cache, req: Request, res: Response): Promise<void> {
  const headers = new Headers(res.headers);
  headers.set(CACHE_DATE_HEADER, new Date().toISOString());
  headers.delete("Set-Cookie");
  const body = await res.blob();
  // Hapus dulu agar entri pindah ke urutan terbaru (LRU sederhana).
  await cache.delete(req);
  await cache.put(req, new Response(body, { status: res.status, statusText: res.statusText, headers }));
  const keys = await cache.keys();
  await Promise.all(keysToEvict(keys).map((k) => cache.delete(k)));
}

async function navigation(req: Request): Promise<Response> {
  try {
    return await fetch(req);
  } catch {
    const shell = await caches.match(APP_SHELL_URL, { cacheName: SHELL_CACHE });
    return shell ?? Response.error();
  }
}

async function shellCacheFirst(req: Request): Promise<Response> {
  const cached = await caches.match(req, { cacheName: SHELL_CACHE });
  return cached ?? fetch(req);
}
