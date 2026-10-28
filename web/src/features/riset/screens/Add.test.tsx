import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import packs from "../../../../../tests/fixtures/research/canon/packs.json";
import { json, mockApi } from "../../../test/mockApi";
import DefaultAdd, { Add } from "./Add";

function Probe() {
  const { pathname, search } = useLocation();
  return <p data-testid="at">{pathname + search}</p>;
}

function renderAdd(entry = "/perusahaan/tambah") {
  return render(
    <MemoryRouter initialEntries={[entry]}>
      <Routes>
        <Route path="/perusahaan/tambah" element={<Add />} />
        <Route path="*" element={<Probe />} />
      </Routes>
    </MemoryRouter>,
  );
}

const lookupItem = (ticker: string, extra: Record<string, unknown> = {}) => ({
  ticker,
  name: `${ticker} Tbk`,
  industry_code: "B1",
  in_universe: false,
  role: null,
  pack: { id: "coal_mining", label: "Batu bara", source: "industry" },
  ...extra,
});
const lookup = (ticker: string) => ({ q: ticker, list_available: true, exact: lookupItem(ticker), items: [lookupItem(ticker)] });
const lookupCalls = (f: ReturnType<typeof mockApi>) => f.mock.calls.filter(([u]) => new URL(String(u), "http://localhost").pathname === "/api/research/lookup");

afterEach(() => vi.unstubAllGlobals());

describe("Tambah emiten #/perusahaan/tambah (design.md §3.4)", () => {
  it("is the named and default export", () => {
    expect(DefaultAdd).toBe(Add);
  });

  it("without ?ticker= the code field is empty and no lookup runs", async () => {
    const f = mockApi({ "/api/research/packs": packs });
    renderAdd();
    expect(screen.getByLabelText(/Kode emiten/)).toHaveValue("");
    await new Promise((r) => setTimeout(r, 400));
    expect(lookupCalls(f)).toHaveLength(0);
  });

  it("?ticker=<T> prefills the code field (upper-cased) and runs the lookup once", async () => {
    const f = mockApi({ "/api/research/packs": packs, "/api/research/lookup": lookup("COAL") });
    renderAdd("/perusahaan/tambah?ticker=coal");
    expect(screen.getByLabelText(/Kode emiten/)).toHaveValue("COAL");
    expect(await screen.findByText("COAL Tbk")).toBeInTheDocument();
    await new Promise((r) => setTimeout(r, 400));
    expect(lookupCalls(f)).toHaveLength(1);
    expect(new URL(String(lookupCalls(f)[0][0]), "http://localhost").searchParams.get("q")).toBe("COAL");
  });

  it("a malformed ?ticker= (not four letters) is ignored: empty field, no lookup", async () => {
    const f = mockApi({ "/api/research/packs": packs });
    renderAdd("/perusahaan/tambah?ticker=toolong9");
    await new Promise((r) => setTimeout(r, 400));
    expect(lookupCalls(f)).toHaveLength(0);
    expect(screen.getByLabelText(/Kode emiten/)).toHaveValue("");
  });

  it("Mulai adds the ticker and opens its company page at the Kuartal terakhir block", async () => {
    const posts: unknown[] = [];
    mockApi({
      "/api/research/packs": packs,
      "/api/research/lookup": lookup("COAL"),
      "/api/research/universe": (_u: URL, init: RequestInit) => {
        posts.push(JSON.parse(String(init.body)));
        return json({ ticker: "COAL" }, 201);
      },
    });
    renderAdd("/perusahaan/tambah?ticker=COAL");
    await screen.findByText("COAL Tbk");
    for (let i = 0; i < 3; i++) fireEvent.click(await screen.findByRole("button", { name: "Lanjut" }));
    fireEvent.click(await screen.findByRole("button", { name: "Mulai" }));
    await waitFor(() => expect(screen.getByTestId("at")).toHaveTextContent("/perusahaan/COAL?section=kuartal"));
    expect(posts[0]).toMatchObject({ ticker: "COAL", role: "manual" });
  });

  it("has the pushed-screen back control to Perusahaan and no old-route links", async () => {
    mockApi({ "/api/research/packs": packs, "/api/research/lookup": lookup("BANK") });
    const { container } = renderAdd();
    expect(screen.getByRole("heading", { level: 1, name: "Tambah emiten" })).toBeInTheDocument();
    expect(container.querySelectorAll('a[href^="#/riset"], a[href^="#/research"]')).toHaveLength(0);
  });
});
