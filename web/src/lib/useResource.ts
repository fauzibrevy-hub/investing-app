import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { api, ApiError, type RequestOptions } from "./api";

/**
 * Pemuat data layar P3. Satu entri cache per URL (dibagi antar layar, mis. summary dipakai Beranda
 * dan Laporan) + bus invalidasi: setelah tulis, `invalidate("/api/finance/")` memuat ulang semua
 * pemakai prefix itu tanpa reload manual (prd.md A1.8). Cache ini hanya di memori; cache offline
 * tetap urusan service worker.
 */

export type ResourceStatus = "loading" | "ready" | "error";

export interface Resource<T> {
  status: ResourceStatus;
  data: T | undefined;
  error: ApiError | null;
  /** true saat memuat ulang dengan data lama masih tampil. */
  refreshing: boolean;
  reload: () => void;
}

type Query = RequestOptions["query"];

interface Entry {
  data?: unknown;
  error?: ApiError;
  at?: number;
  inflight?: Promise<void>;
  /** `version` saat data terakhir berhasil dimuat. */
  loadedVersion?: number;
  /** Naik setiap invalidasi; hook memuat ulang saat berubah. */
  version: number;
}

const entries = new Map<string, Entry>();
const listeners = new Set<() => void>();
let lastLoadedAt: Date | null = null;
let generation = 0;
const FRESH_MS = 30_000;

export function resourceKey(path: string, query?: Query): string {
  if (!query) return path;
  const params = Object.entries(query)
    .filter(([, v]) => v !== undefined && v !== null && v !== "")
    .map(([k, v]) => `${k}=${String(v)}`)
    .sort();
  return params.length ? `${path}?${params.join("&")}` : path;
}

function entry(key: string): Entry {
  let e = entries.get(key);
  if (!e) {
    e = { version: 0 };
    entries.set(key, e);
  }
  return e;
}

function emit() {
  generation++;
  listeners.forEach((l) => l());
}

/** Tandai semua URL berawalan `prefix` basi; hook yang memakainya memuat ulang. */
export function invalidate(prefix = "/api/"): void {
  for (const [key, e] of entries) {
    if (key.startsWith(prefix)) e.version++;
  }
  emit();
}

/** Waktu respons GET terakhir yang berhasil (stempel di sidebar). */
export function getLastLoadedAt(): Date | null {
  return lastLoadedAt;
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Versi global; berubah setiap ada muat/invalidasi. */
export function useResourceGeneration(): number {
  return useSyncExternalStore(subscribe, () => generation, () => 0);
}

/** Hanya untuk test: kosongkan cache. */
export function resetResourceCache(): void {
  entries.clear();
  lastLoadedAt = null;
  emit();
}

function load(key: string, path: string, query: Query): Promise<void> {
  const e = entry(key);
  if (e.inflight) return e.inflight;
  const version = e.version;
  e.inflight = api
    .get(path, { query })
    .then(
      (data) => {
        e.data = data;
        e.error = undefined;
        e.at = Date.now();
        e.loadedVersion = version;
        lastLoadedAt = new Date();
      },
      (err: unknown) => {
        e.error = err instanceof ApiError ? err : new ApiError("network", String(err));
      },
    )
    .finally(() => {
      e.inflight = undefined;
      // Invalidasi saat request berjalan → muat lagi.
      if (e.version !== version) void load(key, path, query);
      emit();
    });
  return e.inflight;
}

/**
 * `path = null` → tidak memuat (mis. menunggu ticker dipilih).
 * Data lama tetap ditampilkan saat memuat ulang (`refreshing`), sehingga tidak ada kedip skeleton.
 */
export function useResource<T>(path: string | null, query?: Query): Resource<T> {
  const key = path ? resourceKey(path, query) : "";
  useResourceGeneration();
  const e = path ? entry(key) : undefined;
  const loadedVersion = useRef<{ key: string; version: number } | null>(null);
  const queryRef = useRef(query);
  queryRef.current = query;

  useEffect(() => {
    if (!path || !e) return;
    const seen = loadedVersion.current;
    if (seen && seen.key === key && seen.version === e.version && (e.data !== undefined || e.error)) return;
    // Data segar dari pemakai lain (≤ 30 s, versi sama): tidak perlu request baru.
    const fresh = e.loadedVersion === e.version && e.at !== undefined && Date.now() - e.at < FRESH_MS;
    loadedVersion.current = { key, version: e.version };
    if (fresh || e.inflight) return;
    void load(key, path, queryRef.current);
  });

  const reload = useCallback(() => {
    if (!path) return;
    entry(key).version++;
    emit();
  }, [key, path]);

  if (!path || !e) return { status: "loading", data: undefined, error: null, refreshing: false, reload };
  const busy = e.inflight !== undefined;
  if (e.data !== undefined) {
    return { status: "ready", data: e.data as T, error: e.error ?? null, refreshing: busy, reload };
  }
  if (e.error && !busy) return { status: "error", data: undefined, error: e.error, refreshing: false, reload };
  return { status: "loading", data: undefined, error: null, refreshing: false, reload };
}

/** Gabungkan status beberapa resource: error jika salah satu error, loading jika salah satu loading. */
export function combineStatus(...rs: Array<Pick<Resource<unknown>, "status">>): ResourceStatus {
  if (rs.some((r) => r.status === "error")) return "error";
  if (rs.some((r) => r.status === "loading")) return "loading";
  return "ready";
}

/** State lokal yang mengikuti nilai awal saat kunci berubah. */
export function useKeyedState<T>(key: string, initial: T): [T, (v: T) => void] {
  const [state, setState] = useState<{ key: string; value: T }>({ key, value: initial });
  const value = state.key === key ? state.value : initial;
  const set = useCallback((v: T) => setState({ key, value: v }), [key]);
  return [value, set];
}
