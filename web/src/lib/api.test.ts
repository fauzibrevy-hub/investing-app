import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  api,
  ApiError,
  CACHE_DATE_HEADER,
  checkSession,
  createSession,
  MESSAGES,
  onCacheInfo,
  onUnauthorized,
  parseErrorBody,
  request,
} from "./api";

function jsonResponse(body: unknown, init: ResponseInit = {}) {
  return new Response(JSON.stringify(body), {
    status: 200,
    ...init,
    headers: { "Content-Type": "application/json", ...(init.headers ?? {}) },
  });
}

/** Tangkap error request sebagai ApiError (test gagal jika promise tidak reject). */
async function fail(p: Promise<unknown>): Promise<ApiError> {
  try {
    await p;
  } catch (e) {
    return e as ApiError;
  }
  throw new Error("promise tidak reject");
}

let fetchMock: ReturnType<typeof vi.fn>;

beforeEach(() => {
  fetchMock = vi.fn();
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

function lastCall() {
  const [url, init] = fetchMock.mock.calls.at(-1)!;
  return { url: url as string, init: init as RequestInit & { headers: Record<string, string> } };
}

describe("request", () => {
  it("memakai path relatif, cookie same-origin, dan header X-Investing-Client", async () => {
    fetchMock.mockResolvedValue(jsonResponse({ ok: 1 }));
    const data = await api.get<{ ok: number }>("/api/finance/summary");
    expect(data).toEqual({ ok: 1 });
    const { url, init } = lastCall();
    expect(url).toBe("/api/finance/summary");
    expect(init.credentials).toBe("same-origin");
    expect(init.headers["X-Investing-Client"]).toBe("web");
    expect(init.headers.Authorization).toBeUndefined();
  });

  it("menolak path yang bukan /api/ atau absolut", async () => {
    await expect(request("https://example.com/api/x")).rejects.toThrow(/relatif/);
    await expect(request("/finance/summary")).rejects.toThrow(/relatif/);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("menyusun query string dan melewati nilai kosong", async () => {
    fetchMock.mockResolvedValue(jsonResponse([]));
    await api.get("/api/idx/metrics", { query: { ticker: "BBCA", metric: "revenue", q: undefined, x: null } });
    expect(lastCall().url).toBe("/api/idx/metrics?ticker=BBCA&metric=revenue");
  });

  it("POST FormData (unggahan): dikirim apa adanya tanpa Content-Type JSON, tetap membawa Idempotency-Key dan header klien", async () => {
    fetchMock.mockResolvedValue(jsonResponse({ created: true }, { status: 201 }));
    const form = new FormData();
    form.set("fy", "2026");
    await api.post("/api/research/COAL/documents/upload", form, { idempotencyKey: "k-1" });
    const { init } = lastCall();
    expect(init.body).toBe(form);
    expect(init.headers["Content-Type"]).toBeUndefined();
    expect(init.headers["Idempotency-Key"]).toBe("k-1");
    expect(init.headers["X-Investing-Client"]).toBe("web");
  });

  it("POST: body JSON, Content-Type, Idempotency-Key, header klien", async () => {
    fetchMock.mockResolvedValue(jsonResponse({ id: "t1" }, { status: 201 }));
    const res = await api.post("/api/finance/transaction", { amount: 1234 }, { idempotencyKey: "k-1" });
    expect(res).toEqual({ id: "t1" });
    const { init } = lastCall();
    expect(init.method).toBe("POST");
    expect(init.body).toBe('{"amount":1234}');
    expect(init.headers["Content-Type"]).toBe("application/json");
    expect(init.headers["Idempotency-Key"]).toBe("k-1");
    expect(init.headers["X-Investing-Client"]).toBe("web");
  });

  it("204 → undefined", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 204 }));
    await expect(api.delete("/api/thesis/drafts/abc")).resolves.toBeUndefined();
  });

  it("401 → ApiError unauthorized + memanggil listener", async () => {
    fetchMock.mockResolvedValue(jsonResponse({ detail: "Not authenticated" }, { status: 401 }));
    const listener = vi.fn();
    const off = onUnauthorized(listener);
    const err = await fail(api.get("/api/finance/summary"));
    off();
    expect(err).toBeInstanceOf(ApiError);
    expect(err.kind).toBe("unauthorized");
    expect(err.status).toBe(401);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("422 FastAPI → kind validation + fields per field", async () => {
    fetchMock.mockResolvedValue(
      jsonResponse(
        {
          detail: [
            { loc: ["body", "account"], msg: "akun tidak diizinkan" },
            { loc: ["body", "amount"], msg: "harus > 0" },
          ],
        },
        { status: 422 },
      ),
    );
    const err = await fail(api.post("/api/finance/transaction", {}));
    expect(err.kind).toBe("validation");
    expect(err.fields).toEqual({ account: "akun tidak diizinkan", amount: "harus > 0" });
    expect(err.message).toBe(MESSAGES.validation);
  });

  it("503 → unavailable dengan pesan dari body", async () => {
    fetchMock.mockResolvedValue(jsonResponse({ error: "database IDX tidak ditemukan" }, { status: 503 }));
    const err = await fail(api.get("/api/idx/summary"));
    expect(err.kind).toBe("unavailable");
    expect(err.message).toBe("database IDX tidak ditemukan");
  });

  it("500 tanpa body → server + pesan default", async () => {
    fetchMock.mockResolvedValue(new Response("", { status: 500 }));
    const err = await fail(api.get("/api/finance/summary"));
    expect(err.kind).toBe("server");
    expect(err.message).toBe(MESSAGES.server);
  });

  it("fetch gagal → network; untuk tulis pesannya 'data belum tersimpan'", async () => {
    fetchMock.mockRejectedValue(new TypeError("Failed to fetch"));
    const getErr = await fail(api.get("/api/finance/summary"));
    expect(getErr.kind).toBe("network");
    expect(getErr.message).toBe(MESSAGES.network);
    const postErr = await fail(api.post("/api/finance/transaction", {}));
    expect(postErr.message).toBe(MESSAGES.offlineWrite);
  });

  it("tulis saat navigator.onLine=false ditolak tanpa memanggil fetch (tidak diantrikan)", async () => {
    vi.spyOn(navigator, "onLine", "get").mockReturnValue(false);
    const err = await fail(api.post("/api/finance/transaction", { amount: 1 }));
    expect(err.kind).toBe("network");
    expect(err.message).toBe("Tidak ada koneksi, data belum tersimpan");
    expect(fetchMock).not.toHaveBeenCalled();
    vi.restoreAllMocks();
  });

  it("timeout → kind timeout", async () => {
    vi.useFakeTimers();
    fetchMock.mockImplementation(
      (_url: string, init: RequestInit) =>
        new Promise((_resolve, reject) => {
          init.signal!.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
        }),
    );
    const p = fail(request("/api/finance/summary", { timeoutMs: 1000 }));
    await vi.advanceTimersByTimeAsync(1000);
    const err = await p;
    expect(err.kind).toBe("timeout");
  });

  it("abort dari pemanggil → kind aborted", async () => {
    fetchMock.mockImplementation(
      (_url: string, init: RequestInit) =>
        new Promise((_resolve, reject) => {
          init.signal!.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
        }),
    );
    const ctrl = new AbortController();
    const p = fail(request("/api/finance/summary", { signal: ctrl.signal }));
    ctrl.abort();
    expect((await p).kind).toBe("aborted");
  });

  it("melaporkan respons dari cache service worker", async () => {
    const listener = vi.fn();
    const off = onCacheInfo(listener);
    fetchMock.mockResolvedValueOnce(jsonResponse({}, { headers: { [CACHE_DATE_HEADER]: "2026-10-20T07:30:00.000Z" } }));
    await api.get("/api/finance/summary");
    fetchMock.mockResolvedValueOnce(jsonResponse({}));
    await api.get("/api/finance/budget");
    off();
    expect(listener.mock.calls[0][0]).toMatchObject({ path: "/api/finance/summary", fromCache: true });
    expect(listener.mock.calls[0][0].cachedAt.toISOString()).toBe("2026-10-20T07:30:00.000Z");
    expect(listener.mock.calls[1][0]).toMatchObject({ path: "/api/finance/budget", fromCache: false, cachedAt: null });
  });
});

describe("sesi", () => {
  it("createSession mengirim token di body sekali dan tidak menyimpannya", async () => {
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    fetchMock.mockResolvedValue(new Response(null, { status: 204 }));
    await createSession("rahasia-yang-panjang-sekali-0123456789");
    const { url, init } = lastCall();
    expect(url).toBe("/api/session");
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body as string)).toEqual({ token: "rahasia-yang-panjang-sekali-0123456789" });
    expect(init.headers.Authorization).toBeUndefined();
    expect(setItem).not.toHaveBeenCalled();
    expect(window.localStorage.length).toBe(0);
    expect(window.sessionStorage.length).toBe(0);
    setItem.mockRestore();
  });

  it("checkSession: 2xx valid, 401 invalid, error lain dilempar", async () => {
    fetchMock.mockResolvedValueOnce(new Response(null, { status: 204 }));
    await expect(checkSession()).resolves.toBe("valid");
    fetchMock.mockResolvedValueOnce(jsonResponse({}, { status: 401 }));
    await expect(checkSession()).resolves.toBe("invalid");
    fetchMock.mockRejectedValueOnce(new TypeError("Failed to fetch"));
    await expect(checkSession()).rejects.toMatchObject({ kind: "network" });
  });
});

describe("parseErrorBody", () => {
  it("bentuk generik {message, fields}", () => {
    expect(parseErrorBody({ message: "x", fields: { amount: "harus > 0" } })).toEqual({ message: "x", fields: { amount: "harus > 0" } });
  });
  it("string dan null", () => {
    expect(parseErrorBody("teks")).toEqual({ message: "teks", fields: {} });
    expect(parseErrorBody(null)).toEqual({ message: null, fields: {} });
  });
});
