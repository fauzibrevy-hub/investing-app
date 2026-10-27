/**
 * Aturan cache service worker (design.md §10), dipisah dari sw.ts agar bisa diuji di vitest.
 * File ini dibundel ke sw.js; jangan import modul DOM/React di sini.
 */

export const API_CACHE_PREFIX = "finance-api-";
/** Naikkan saat bentuk payload /api berubah (v2 = R5: research v5) agar cache lama tak masuk UI baru. */
export const API_CACHE = `${API_CACHE_PREFIX}v2`;
export const SHELL_CACHE_PREFIX = "finance-shell-";
export const API_CACHE_MAX_ENTRIES = 50;
export const API_TIMEOUT_MS = 4_000;
/** Disalin dari lib/api.ts (CACHE_DATE_HEADER); sw.js tidak mengimpor kode UI. */
export const CACHE_DATE_HEADER = "X-Finance-Cache-Date";

export type Strategy =
  | "passthrough" // tidak disentuh SW: tulis, sesi, SSE, lintas origin
  | "api-network-first"
  | "navigation" // HTML: network, fallback app shell dari precache
  | "shell-cache-first";

export function strategyFor(req: { method: string; url: string; mode?: string; headers?: { get(name: string): string | null } }, origin: string): Strategy {
  const url = new URL(req.url);
  if (url.origin !== origin) return "passthrough";
  // POST/PUT/PATCH/DELETE tidak pernah di-cache atau di-queue.
  if (req.method !== "GET") return "passthrough";
  if (url.pathname.startsWith("/api/")) {
    // Sesi tidak di-cache (respons berisi Set-Cookie); stream SSE tidak bisa di-cache.
    if (url.pathname === "/api/session") return "passthrough";
    if (url.pathname.endsWith("/stream") || req.headers?.get("Accept")?.includes("text/event-stream")) return "passthrough";
    return "api-network-first";
  }
  if (req.mode === "navigate") return "navigation";
  return "shell-cache-first";
}

/** Hanya respons sukses yang di-cache (401/5xx tidak menimpa data terakhir yang baik). */
export function isCacheable(res: { ok: boolean; status: number; type?: string }): boolean {
  return res.ok && res.status === 200 && (res.type === undefined || res.type === "basic" || res.type === "default");
}

/** Kunci tertua yang harus dibuang agar cache ≤ max entri (urutan kunci = urutan sisip). */
export function keysToEvict<T>(keys: readonly T[], max = API_CACHE_MAX_ENTRIES): T[] {
  return keys.length > max ? keys.slice(0, keys.length - max) : [];
}

/** Cache API lama (finance-api-* selain yang aktif) yang harus dihapus saat activate. */
export function staleApiCaches(names: readonly string[], current = API_CACHE): string[] {
  return names.filter((n) => n.startsWith(API_CACHE_PREFIX) && n !== current);
}
