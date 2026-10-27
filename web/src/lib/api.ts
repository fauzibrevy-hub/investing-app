/**
 * Klien API (prd.md §5, L3-BRIEF §2).
 * - Path selalu relatif `/api/...` (satu origin dengan backend; tanpa host/port).
 * - Auth lewat cookie `finance_session` (HttpOnly) → `credentials: "same-origin"`.
 * - Token hanya dikirim sekali ke `POST /api/session`; tidak pernah disimpan di sini.
 * - Header `X-Finance-Client: web` dikirim di setiap request (wajib untuk tulis).
 */

export const CLIENT_HEADER = "X-Finance-Client";
export const CLIENT_VALUE = "web";
/** Diisi service worker saat respons diambil dari cache (design.md §10). Nilai: ISO 8601. */
export const CACHE_DATE_HEADER = "X-Finance-Cache-Date";

export type ApiErrorKind =
  | "unauthorized" // 401
  | "forbidden" // 403
  | "not_found" // 404
  | "validation" // 400 / 422
  | "unavailable" // 503 (mis. sumber data IDX tidak ada)
  | "server" // 5xx lain
  | "http" // status lain
  | "network" // fetch gagal (offline, server mati)
  | "timeout"
  | "aborted";

export class ApiError extends Error {
  readonly kind: ApiErrorKind;
  readonly status: number | null;
  /** Pesan per field dari 422, mis. `{ account: "akun tidak diizinkan" }`. */
  readonly fields: Record<string, string>;
  readonly body: unknown;

  constructor(kind: ApiErrorKind, message: string, init: { status?: number | null; fields?: Record<string, string>; body?: unknown } = {}) {
    super(message);
    this.name = "ApiError";
    this.kind = kind;
    this.status = init.status ?? null;
    this.fields = init.fields ?? {};
    this.body = init.body;
  }
}

export const MESSAGES = {
  offlineWrite: "Tidak ada koneksi, data belum tersimpan",
  network: "Server tidak bisa dihubungi",
  timeout: "Server terlalu lama merespons",
  unauthorized: "Sesi berakhir, buka kunci lagi",
  server: "Gagal memuat",
  validation: "Periksa isian yang ditandai",
} as const;

export interface CacheInfo {
  /** true jika respons berasal dari cache service worker (offline). */
  fromCache: boolean;
  cachedAt: Date | null;
}

type Listener<T> = (value: T) => void;
const unauthorizedListeners = new Set<Listener<ApiError>>();
const cacheListeners = new Set<Listener<CacheInfo & { path: string }>>();

/** Dipanggil setiap ada respons 401 — shell menampilkan layar buka kunci. */
export function onUnauthorized(listener: Listener<ApiError>): () => void {
  unauthorizedListeners.add(listener);
  return () => unauthorizedListeners.delete(listener);
}

/** Dipanggil setiap respons GET berhasil — shell menampilkan banner "Offline · data per …". */
export function onCacheInfo(listener: Listener<CacheInfo & { path: string }>): () => void {
  cacheListeners.add(listener);
  return () => cacheListeners.delete(listener);
}

export interface RequestOptions {
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  /** Diserialisasi sebagai JSON; `FormData` (unggahan berkas) dikirim apa adanya dengan batas multipart dari peramban. */
  body?: unknown;
  query?: Record<string, string | number | boolean | undefined | null>;
  signal?: AbortSignal;
  /** Wajib untuk `POST /api/finance/transaction` (prd.md §6.1). */
  idempotencyKey?: string;
  /** Default 15 s; 0 = tanpa batas. */
  timeoutMs?: number;
}

const DEFAULT_TIMEOUT_MS = 15_000;

function buildUrl(path: string, query?: RequestOptions["query"]): string {
  if (!path.startsWith("/api/")) {
    throw new Error(`Path API harus relatif dan diawali /api/: ${path}`);
  }
  if (!query) return path;
  const params = new URLSearchParams();
  for (const [k, v] of Object.entries(query)) {
    if (v !== undefined && v !== null) params.set(k, String(v));
  }
  const qs = params.toString();
  return qs ? `${path}?${qs}` : path;
}

function isOffline(): boolean {
  return typeof navigator !== "undefined" && navigator.onLine === false;
}

function kindForStatus(status: number): ApiErrorKind {
  if (status === 401) return "unauthorized";
  if (status === 403) return "forbidden";
  if (status === 404) return "not_found";
  if (status === 400 || status === 422) return "validation";
  if (status === 503) return "unavailable";
  if (status >= 500) return "server";
  return "http";
}

/**
 * Ambil pesan + field error dari body. Mendukung bentuk FastAPI MC
 * (`{detail: "..."}` / `{detail: [{loc: [..., "field"], msg}]}`) dan bentuk generik
 * (`{error|message, fields: {field: msg}}`).
 */
export function parseErrorBody(body: unknown): { message: string | null; fields: Record<string, string> } {
  const fields: Record<string, string> = {};
  if (typeof body === "string") return { message: body.trim() || null, fields };
  if (!body || typeof body !== "object") return { message: null, fields };
  const b = body as Record<string, unknown>;

  let message: string | null = null;
  if (typeof b.detail === "string") message = b.detail;
  else if (typeof b.message === "string") message = b.message;
  else if (typeof b.error === "string") message = b.error;

  if (Array.isArray(b.detail)) {
    for (const item of b.detail) {
      if (!item || typeof item !== "object") continue;
      const { loc, msg } = item as { loc?: unknown; msg?: unknown };
      if (typeof msg !== "string") continue;
      const field = Array.isArray(loc) && loc.length > 0 ? String(loc[loc.length - 1]) : "_";
      fields[field] ??= msg;
    }
  }
  if (b.fields && typeof b.fields === "object") {
    for (const [k, v] of Object.entries(b.fields as Record<string, unknown>)) {
      if (typeof v === "string") fields[k] ??= v;
    }
  }
  return { message, fields };
}

async function readBody(res: Response): Promise<unknown> {
  if (res.status === 204 || res.status === 205) return undefined;
  const text = await res.text();
  if (!text) return undefined;
  const type = res.headers.get("Content-Type") ?? "";
  if (type.includes("json")) {
    try {
      return JSON.parse(text);
    } catch {
      return text;
    }
  }
  return text;
}

export async function request<T = unknown>(path: string, options: RequestOptions = {}): Promise<T> {
  const method = options.method ?? "GET";
  const isWrite = method !== "GET";
  const url = buildUrl(path, options.query);

  const headers: Record<string, string> = {
    Accept: "application/json",
    [CLIENT_HEADER]: CLIENT_VALUE,
  };
  const multipart = typeof FormData !== "undefined" && options.body instanceof FormData;
  if (options.body !== undefined && !multipart) headers["Content-Type"] = "application/json";
  if (options.idempotencyKey) headers["Idempotency-Key"] = options.idempotencyKey;

  if (isWrite && isOffline()) {
    throw new ApiError("network", MESSAGES.offlineWrite);
  }

  const controller = new AbortController();
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  let timedOut = false;
  const timer = timeoutMs > 0 ? setTimeout(() => ((timedOut = true), controller.abort()), timeoutMs) : null;
  const onOuterAbort = () => controller.abort();
  options.signal?.addEventListener("abort", onOuterAbort);

  let res: Response;
  try {
    res = await fetch(url, {
      method,
      headers,
      body: options.body === undefined ? undefined : multipart ? (options.body as FormData) : JSON.stringify(options.body),
      credentials: "same-origin",
      cache: "no-store",
      signal: controller.signal,
    });
  } catch (err) {
    if (timedOut) throw new ApiError("timeout", MESSAGES.timeout);
    if (options.signal?.aborted) throw new ApiError("aborted", "Dibatalkan");
    throw new ApiError("network", isWrite ? MESSAGES.offlineWrite : MESSAGES.network, { body: err });
  } finally {
    if (timer) clearTimeout(timer);
    options.signal?.removeEventListener("abort", onOuterAbort);
  }

  const body = await readBody(res);

  if (!res.ok) {
    const kind = kindForStatus(res.status);
    const parsed = parseErrorBody(body);
    const fallback = kind === "unauthorized" ? MESSAGES.unauthorized : kind === "validation" ? MESSAGES.validation : MESSAGES.server;
    const error = new ApiError(kind, parsed.message ?? fallback, { status: res.status, fields: parsed.fields, body });
    if (kind === "unauthorized") unauthorizedListeners.forEach((l) => l(error));
    throw error;
  }

  if (!isWrite) {
    const cachedHeader = res.headers.get(CACHE_DATE_HEADER);
    const cachedAt = cachedHeader ? new Date(cachedHeader) : null;
    const info = { path, fromCache: cachedAt !== null, cachedAt };
    cacheListeners.forEach((l) => l(info));
  }

  return body as T;
}

export const api = {
  get: <T = unknown>(path: string, options: Omit<RequestOptions, "method" | "body"> = {}) => request<T>(path, { ...options, method: "GET" }),
  post: <T = unknown>(path: string, body?: unknown, options: Omit<RequestOptions, "method" | "body"> = {}) =>
    request<T>(path, { ...options, method: "POST", body }),
  delete: <T = unknown>(path: string, options: Omit<RequestOptions, "method" | "body"> = {}) => request<T>(path, { ...options, method: "DELETE" }),
};

/** Kunci idempotensi baru per percobaan simpan (prd.md §6.1). */
export function newIdempotencyKey(): string {
  return crypto.randomUUID();
}

/**
 * Tukar token dengan cookie sesi. Token hanya hidup di argumen fungsi ini;
 * tidak disimpan di localStorage, sessionStorage, atau variabel modul.
 * ASUMSI bentuk body: `{ "token": "<FINANCE_APP_TOKEN>" }`.
 */
export async function createSession(token: string): Promise<void> {
  await request("/api/session", { method: "POST", body: { token } });
}

export type SessionState = "valid" | "invalid";

/**
 * Cek apakah cookie sesi masih berlaku. ASUMSI kontrak: `GET /api/session` → 2xx jika sesi valid, 401 jika tidak.
 * Error selain 401 (server mati, offline) dilempar apa adanya.
 */
export async function checkSession(): Promise<SessionState> {
  try {
    await request("/api/session", { timeoutMs: 6_000 });
    return "valid";
  } catch (err) {
    if (err instanceof ApiError && err.kind === "unauthorized") return "invalid";
    throw err;
  }
}
