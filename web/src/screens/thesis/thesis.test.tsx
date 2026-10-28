import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { json, mockApi } from "../../test/mockApi";
import { ThesisScreen } from "./ThesisScreen";

const draft = {
  id: "draft_test_1_abc",
  ticker: "TEST",
  title: "ACCEPTANCE",
  thesis_statement: "x",
  thesis_type: "fundamental",
  is_primary: false,
  scenario_status: "Menunggu",
  story_status: "Belum terkonfirmasi",
  falsifiers: ["y"],
  triggers_add: [],
  triggers_exit: [],
  notes: "",
  created_at: "2026-10-12T01:00:00+00:00",
  updated_at: "2026-10-12T01:00:00+00:00",
};
const issuer = {
  ticker: "BUMI",
  company: "Bumi Resources",
  status: "holdings",
  verdict: "Deleveraging + batubara",
  narrative_status: "on track",
  fundamental_status: null,
  growth_status: null,
  falsifiers: { count: 1, nearest: null },
  review: { last_date: "2026-10-01", days_ago: 19, due: false },
  evidence: { resolved: 0, doc_quoted: 0, missing: 0, total: 0 },
  tahap: null,
  errors: [],
};

function Where() {
  const l = useLocation();
  return <output data-testid="where">{l.pathname + l.search}</output>;
}
const renderAt = (path: string) =>
  render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/thesis/*" element={<ThesisScreen />} />
      </Routes>
      <Where />
    </MemoryRouter>,
  );

afterEach(() => {
  vi.unstubAllGlobals();
  window.localStorage.clear();
});

describe("Tesis", () => {
  it("daftar menggabungkan dossier + draft; filter Draft", async () => {
    mockApi({ "/api/thesis/summary": { root_ok: true, generated_at: 0, issuers: [issuer] }, "/api/thesis/drafts": { drafts: [draft], vault_drafts: [], total: 1 } });
    renderAt("/thesis");
    expect(await screen.findByText("Deleveraging + batubara")).toBeInTheDocument();
    expect(screen.getByText("Story: on track · diperbarui 1 Okt")).toBeInTheDocument();
    expect(screen.getByText("ACCEPTANCE")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Draft" }));
    expect(screen.queryByText("Deleveraging + batubara")).toBeNull();
    expect(screen.getByTestId("where")).toHaveTextContent("jenis=draft");
  });

  it("editor: validasi, simpan lokal per ticker, POST lalu kembali ke filter Draft", async () => {
    let body: Record<string, unknown> | null = null;
    mockApi({
      "/api/thesis/drafts": (_u: URL, init: RequestInit) => {
        if (init.method === "POST") {
          body = JSON.parse(String(init.body));
          return json({ ...draft }, 201);
        }
        return json({ drafts: [], vault_drafts: [], total: 0 });
      },
      "/api/thesis/summary": { root_ok: true, generated_at: 0, issuers: [] },
    });
    renderAt("/thesis/new");
    fireEvent.click(screen.getByRole("button", { name: "Simpan draft" }));
    expect(screen.getByText("Judul tesis wajib diisi")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Ticker"), { target: { value: "bbca" } });
    fireEvent.change(screen.getByLabelText("Judul"), { target: { value: "Bank kuat" } });
    fireEvent.change(screen.getByLabelText("Thesis statement"), { target: { value: "CASA tinggi" } });
    fireEvent.change(screen.getByLabelText("Falsifiers 1"), { target: { value: " NIM turun " } });
    expect(JSON.parse(window.localStorage.getItem("finance.thesisDraft.BBCA") ?? "{}").title).toBe("Bank kuat");
    fireEvent.click(screen.getByRole("button", { name: "Simpan draft" }));
    await waitFor(() => expect(screen.getByTestId("where")).toHaveTextContent("/thesis?jenis=draft"));
    expect(body).toMatchObject({ ticker: "BBCA", title: "Bank kuat", thesis_statement: "CASA tinggi", falsifiers: ["NIM turun"], triggers_add: [], thesis_type: "fundamental", scenario_initial: "Menunggu" });
    expect(window.localStorage.getItem("finance.thesisDraft.BBCA")).toBeNull();
  });

  it("hapus draft lewat dialog konfirmasi → DELETE", async () => {
    const del = vi.fn(() => new Response(null, { status: 204 }));
    mockApi({
      [`/api/thesis/drafts/${draft.id}`]: (_u: URL, init: RequestInit) => (init.method === "DELETE" ? del() : json(draft)),
      "/api/thesis/drafts": { drafts: [], vault_drafts: [], total: 0 },
      "/api/thesis/summary": { root_ok: true, generated_at: 0, issuers: [] },
    });
    renderAt(`/thesis/draft/${draft.id}`);
    fireEvent.click(await screen.findByRole("button", { name: "Hapus draft" }));
    expect(del).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Hapus" }));
    await waitFor(() => expect(del).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByTestId("where")).toHaveTextContent("/thesis?jenis=draft"));
  });

  it("dossier 404 → empty state, bukan error", async () => {
    mockApi({ "/api/thesis/BUMI": () => json({ detail: "Dossier BUMI tidak ditemukan" }, 404) });
    renderAt("/thesis/BUMI");
    expect(await screen.findByText("Belum ada dossier BUMI di vault")).toBeInTheDocument();
  });
});
