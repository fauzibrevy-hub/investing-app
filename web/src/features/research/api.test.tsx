import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { CLIENT_HEADER } from "../../lib/api";
import { json, mockApi } from "../../test/mockApi";
import bank from "../../../../tests/fixtures/research/packs/BANK.json";
import {
  acceptProposal,
  addToUniverse,
  annotateJournal,
  closeClaim,
  createClaim,
  createJournalEntry,
  dismissEvent,
  markSeen,
  putKpi,
  putScenarios,
  rejectProposal,
  uploadDocument,
  useProposals,
  useClaims,
  useCompare,
  useDossier,
  useFactDetail,
  useFinancials,
  useJournal,
  usePack,
} from "./api";
import type { FactTarget } from "./types";

const target: FactTarget = {
  filing_id: "AAAA_2026_Q2",
  element: "idx-cor:ProfitLoss",
  ctx_kind: "duration",
  start: "2026-01-01",
  end: "2026-06-30",
  dims: "",
  ctx: "2026-01-01_2026-06-30",
};

let fetchMock: ReturnType<typeof mockApi>;

function calls(): Array<{ path: string; method: string; headers: Record<string, string>; body: unknown }> {
  return fetchMock.mock.calls.map(([input, init]) => {
    const url = new URL(String(input), "http://localhost");
    const i = (init ?? {}) as RequestInit;
    return {
      path: `${url.pathname}${url.search}`,
      method: i.method ?? "GET",
      headers: (i.headers ?? {}) as Record<string, string>,
      body: i.body ? JSON.parse(String(i.body)) : undefined,
    };
  });
}

beforeEach(() => {
  fetchMock = mockApi({});
});

describe("reads", () => {
  it("loads the pack payload and reports ready", async () => {
    fetchMock = mockApi({ "/api/research/BANK/pack": bank });
    const { result } = renderHook(() => usePack("bank"));
    expect(result.current.status).toBe("loading");
    await waitFor(() => expect(result.current.status).toBe("ready"));
    expect(result.current.data?.pack.id).toBe("bank_sharia");
    expect(calls()[0].path).toBe("/api/research/BANK/pack");
  });

  it("builds query strings and drops empty parameters", async () => {
    renderHook(() => useFinancials("aaaa", "ttm", 8));
    renderHook(() => useJournal("aaaa"));
    renderHook(() => useCompare({ ticker: "aaaa", tickers: ["bbbb", "cccc"] }));
    await waitFor(() => expect(calls()).toHaveLength(3));
    const paths = calls().map((c) => c.path);
    expect(paths).toContain("/api/research/AAAA/financials?view=ttm&periods=8");
    expect(paths).toContain("/api/research/AAAA/journal?limit=20&offset=0");
    expect(paths).toContain("/api/research/compare?ticker=AAAA&tickers=BBBB%2CCCCC");
  });

  it("asks for the fact with its context and dimensions, and does nothing without a target", async () => {
    const { rerender } = renderHook(({ t }: { t: FactTarget | null }) => useFactDetail(t), { initialProps: { t: null as FactTarget | null } });
    expect(fetchMock).not.toHaveBeenCalled();
    rerender({ t: { ...target, dims: "axis=member" } });
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    expect(calls()[0].path).toBe(
      "/api/research/facts/AAAA_2026_Q2/idx-cor%3AProfitLoss?ctx=2026-01-01_2026-06-30&dims=axis%3Dmember",
    );
  });

  it("does not request without a ticker (waiting for the route)", () => {
    renderHook(() => useDossier(null));
    renderHook(() => useCompare(null));
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("a failed read is a real error state with a working retry, never fixture data", async () => {
    let attempts = 0;
    fetchMock = mockApi({
      "/api/research/AAAA/claims": () => {
        attempts += 1;
        return attempts === 1 ? json({ detail: "store down" }, 503) : json({ ticker: "AAAA", items: [], counts: {}, metrics: [], windows: [], comparators: [] });
      },
    });
    const { result } = renderHook(() => useClaims("AAAA"));
    await waitFor(() => expect(result.current.status).toBe("error"));
    expect(result.current.data).toBeUndefined();
    expect(result.current.error?.kind).toBe("unavailable");
    act(() => result.current.reload());
    await waitFor(() => expect(result.current.status).toBe("ready"));
    expect(attempts).toBe(2);
  });
});

describe("writes", () => {
  it("carry the client header and a fresh Idempotency-Key per call", async () => {
    fetchMock = mockApi({ "/api/research/AAAA/journal": () => json({ id: "j-1" }, 201) });
    await createJournalEntry("aaaa", { kind: "catatan", text: "tes" });
    await createJournalEntry("aaaa", { kind: "catatan", text: "tes" });
    const writes = calls().filter((c) => c.method === "POST");
    expect(writes).toHaveLength(2);
    for (const w of writes) {
      expect(w.headers[CLIENT_HEADER]).toBe("web");
      expect(w.headers["Idempotency-Key"]).toMatch(/\S{8,}/);
    }
    expect(writes[0].headers["Idempotency-Key"]).not.toBe(writes[1].headers["Idempotency-Key"]);
  });

  it("use the canon verbs and paths", async () => {
    fetchMock = mockApi({
      "/api/research/universe": () => json({ ticker: "BBBB" }, 201),
      "/api/research/AAAA/kpi/production_mt/2026/2": () => json({ state: "entered" }),
      "/api/research/AAAA/scenarios": () => json({ ticker: "AAAA" }),
      "/api/research/AAAA/claims": () => json({ id: "c-1" }, 201),
      "/api/research/claims/c-1": () => json({ id: "c-1" }),
      "/api/research/journal/j-1/annotations": () => json({ id: "a-1" }, 201),
      "/api/research/events/e-1/dismiss": () => json({ id: "e-1" }),
    });
    await addToUniverse({ ticker: "bbbb", role: "peer", peer_of: "AAAA" });
    await putKpi("aaaa", "production_mt", 2026, 2, { value: 12, document: "paparan", document_date: "2026-08-01" });
    await putScenarios("aaaa", { base: { growth: 5, margin: 10, exit_multiple: 8, years: 5 } });
    await createClaim("aaaa", { statement: "x", metric: "core.net_margin", comparator: ">=", threshold: 10, window: "latest", due: "2027-01-01" });
    await closeClaim("c-1");
    await annotateJournal("j-1", "tambahan");
    await dismissEvent("e-1");
    expect(calls().map((c) => `${c.method} ${c.path}`)).toEqual([
      "POST /api/research/universe",
      "PUT /api/research/AAAA/kpi/production_mt/2026/2",
      "PUT /api/research/AAAA/scenarios",
      "POST /api/research/AAAA/claims",
      "PATCH /api/research/claims/c-1",
      "POST /api/research/journal/j-1/annotations",
      "POST /api/research/events/e-1/dismiss",
    ]);
    expect(calls()[0].body).toMatchObject({ ticker: "BBBB", role: "peer", peer_of: "AAAA" });
    expect(calls()[4].body).toEqual({ closed: true });
  });

  it("reload every Riset read afterwards", async () => {
    let loads = 0;
    fetchMock = mockApi({
      "/api/research/AAAA/claims": () => {
        loads += 1;
        return json({ ticker: "AAAA", items: [], counts: {}, metrics: [], windows: [], comparators: [] });
      },
      "/api/research/AAAA/journal": () => json({ id: "j-1" }, 201),
    });
    const { result } = renderHook(() => useClaims("AAAA"));
    await waitFor(() => expect(result.current.status).toBe("ready"));
    await act(async () => void (await createJournalEntry("AAAA", { kind: "catatan", text: "tes" })));
    await waitFor(() => expect(loads).toBe(2));
  });

  it("markSeen leaves the dossier alone so 'Apa yang berubah' is not emptied", async () => {
    let dossierLoads = 0;
    fetchMock = mockApi({
      "/api/research/AAAA": () => {
        dossierLoads += 1;
        return json({ ticker: "AAAA" });
      },
      "/api/research/AAAA/seen": () => new Response(null, { status: 204 }),
    });
    const { result } = renderHook(() => useDossier("AAAA"));
    await waitFor(() => expect(result.current.status).toBe("ready"));
    await act(async () => void (await markSeen("AAAA")));
    await new Promise((r) => setTimeout(r, 20));
    expect(dossierLoads).toBe(1);
  });

  it("surface a failed write to the caller instead of swallowing it", async () => {
    fetchMock = mockApi({ "/api/research/AAAA/journal": () => json({ detail: "bad" }, 422) });
    await expect(createJournalEntry("AAAA", { kind: "catatan", text: "" })).rejects.toMatchObject({ kind: "validation", status: 422 });
  });
});

describe("R8 upload and proposals", () => {
  it("uploadDocument sends the FormData as is (no JSON Content-Type), the client header and the caller's Idempotency-Key, with a long timeout", async () => {
    fetchMock = mockApi({ "/api/research/AAAA/documents/upload": () => json({ created: true }, 201) });
    const form = new FormData();
    form.set("file", new File(["%PDF-1.4"], "paparan.pdf"));
    form.set("fy", "2026");
    await uploadDocument("aaaa", form, { idempotencyKey: "kunci-1" });
    const [input, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(input).toBe("/api/research/AAAA/documents/upload");
    expect(init.method).toBe("POST");
    expect(init.body).toBe(form);
    const headers = init.headers as Record<string, string>;
    expect(headers["Content-Type"]).toBeUndefined();
    expect(headers["Idempotency-Key"]).toBe("kunci-1");
    expect(headers[CLIENT_HEADER]).toBe("web");
  });

  it("acceptProposal sends no body, or {replace: true}; rejectProposal sends no body; all reload the Riset reads", async () => {
    let loads = 0;
    fetchMock = mockApi({
      "/api/research/AAAA/proposals": () => {
        loads += 1;
        return json({ ticker: "AAAA", document_id: null, items: [], counts: { pending: 0, accepted: 0, rejected: 0, superseded: 0 }, documents: [] });
      },
      "/api/research/proposals/p-1/accept": () => json({ replaced: false }),
      "/api/research/proposals/p-2/reject": () => json({ proposal: {} }),
    });
    const { result } = renderHook(() => useProposals("aaaa"));
    await waitFor(() => expect(result.current.status).toBe("ready"));
    await act(async () => void (await acceptProposal("p-1")));
    await act(async () => void (await acceptProposal("p-1", true)));
    await act(async () => void (await rejectProposal("p-2")));
    const writes = fetchMock.mock.calls.filter(([, init]) => (init as RequestInit | undefined)?.method === "POST");
    expect(writes.map(([url, init]) => [url, (init as RequestInit).body])).toEqual([
      ["/api/research/proposals/p-1/accept", undefined],
      ["/api/research/proposals/p-1/accept", JSON.stringify({ replace: true })],
      ["/api/research/proposals/p-2/reject", undefined],
    ]);
    await waitFor(() => expect(loads).toBeGreaterThan(1));
  });
});
