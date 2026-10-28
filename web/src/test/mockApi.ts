import { vi } from "vitest";
import { resetResourceCache } from "../lib/useResource";

type Handler = unknown | ((url: URL, init: RequestInit) => Response | Promise<Response>);

export const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

/**
 * Mock `fetch` per path (test saja; layar di aplikasi selalu memuat dari backend nyata).
 * Kunci = pathname, atau pathname + "?" + query persis. Path tanpa handler → 404.
 */
export function mockApi(routes: Record<string, Handler>) {
  resetResourceCache();
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init: RequestInit = {}) => {
    const url = new URL(String(input), "http://localhost");
    const handler = routes[`${url.pathname}${url.search}`] ?? routes[url.pathname];
    if (handler === undefined) return json({ detail: "not mocked" }, 404);
    if (typeof handler === "function") return (handler as (u: URL, i: RequestInit) => Response)(url, init);
    return json(handler);
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}
