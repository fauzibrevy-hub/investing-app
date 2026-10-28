import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter } from "react-router-dom";
import { json, mockApi } from "../../test/mockApi";
import { ThesisBlock } from "./ThesisBlock";

// Ported from finance-app `screens/thesis/thesis.test.tsx` (list, editor, delete, dossier 404) to the Tesis block.

const draft = {
  id: "draft_test_1_abc",
  ticker: "BUMI",
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
const otherDraft = { ...draft, id: "draft_other", ticker: "BBCA", title: "Bukan BUMI" };
const vaultDraft = { id: "vault_1", title: "Draf vault", status_lifecycle: "draft", thesis_statement: "v", tahap: "", is_readonly: true, falsifiers: [], file_info: { name: "v.md" } };
const detail = {
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
  alasan_masuk: "Utang turun **cepat** setelah restrukturisasi.\n\n- arus kas naik\n- bunga turun",
  verdict_text: null,
  lens: { metrics: [{ metrik: "Net debt", definisi: "Utang bersih / EBITDA", alasan: "", sumber_data: "", field: "" }], locked: null, approved_at: null },
  falsifier_items: [{ n: 1, text: "Harga batubara < 100", dates: ["2026-12-31"] }],
  review_log: [{ date: "2026-10-01", title: "Q3", bullets: [{ level: 0, text: "Sesuai rencana" }] }],
  doc_evidence: [{ metrik: "Produksi", periode: "Q3 26", raw: "20", unit: "jt ton", dokumen: "LK Q3", hal: "12" }],
  chain: [],
  trigger_tambah: [],
  trigger_buang: [],
};

const notFound = () => json({ detail: "Dossier BUMI tidak ditemukan" }, 404);

const renderBlock = (props: { open?: boolean; onToggle?: () => void } = {}) =>
  render(
    <MemoryRouter>
      <ThesisBlock ticker="BUMI" open={props.open ?? true} onToggle={props.onToggle} />
    </MemoryRouter>,
  );

afterEach(() => {
  vi.unstubAllGlobals();
  window.localStorage.clear();
});

describe("Tesis block (design.md §3.4)", () => {
  it("is a lipat block with id tesis that loads nothing while closed", () => {
    const fetchMock = mockApi({});
    const onToggle = vi.fn();
    const { container } = renderBlock({ open: false, onToggle });
    expect(container.querySelector("section#tesis")).not.toBeNull();
    const toggle = screen.getByRole("button", { name: "Tesis" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(toggle);
    expect(onToggle).toHaveBeenCalledTimes(1);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("shows the vault dossier read-only: statement as markdown, Lens rows, Falsifiers, Review log, Evidence, updated date", async () => {
    mockApi({ "/api/thesis/BUMI": detail, "/api/thesis/drafts": { drafts: [], vault_drafts: [], total: 0 } });
    renderBlock();
    const strong = await screen.findByText("cepat");
    expect(strong.tagName).toBe("STRONG");
    expect(screen.getByText("arus kas naik").tagName).toBe("LI");
    expect(screen.getByText("diperbarui 1 Okt")).toBeInTheDocument();
    expect(screen.getAllByRole("heading", { level: 3 }).map((h) => h.textContent)).toEqual(["Lens", "Falsifiers", "Review log", "Evidence"]);
    expect(screen.getByText("Net debt")).toBeInTheDocument();
    expect(screen.getByText("Utang bersih / EBITDA")).toBeInTheDocument();
    expect(screen.getByText("Harga batubara < 100")).toBeInTheDocument();
    expect(screen.getByText("2026-10-01 · Q3")).toBeInTheDocument();
    expect(screen.getByText("Produksi · Q3 26")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Hapus/ })).toBeNull();
  });

  it("lists only this ticker's drafts as 2-line rows (title; type · story · date), never vault drafts", async () => {
    mockApi({ "/api/thesis/BUMI": notFound, "/api/thesis/drafts": { drafts: [draft, otherDraft], vault_drafts: [vaultDraft], total: 2 } });
    renderBlock();
    const list = await screen.findByRole("list", { name: "Draf tesis BUMI" });
    const rows = within(list).getAllByRole("button");
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveTextContent("ACCEPTANCE");
    expect(rows[0]).toHaveTextContent("fundamental · Belum terkonfirmasi · 12 Okt");
    expect(screen.queryByText("Bukan BUMI")).toBeNull();
    expect(screen.queryByText("Draf vault")).toBeNull();
  });

  it("dossier 404 and no draft → empty state with Tulis draf tesis, not an error", async () => {
    mockApi({ "/api/thesis/BUMI": notFound, "/api/thesis/drafts": { drafts: [otherDraft], vault_drafts: [], total: 1 } });
    renderBlock();
    expect(await screen.findByText("Belum ada tesis untuk BUMI")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Tulis draf tesis" })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("loading skeleton, then error Gagal memuat tesis + Coba lagi reloads", async () => {
    let fail = true;
    mockApi({
      "/api/thesis/BUMI": () => (fail ? json({ detail: "boom" }, 500) : json(detail)),
      "/api/thesis/drafts": () => (fail ? json({ detail: "boom" }, 500) : json({ drafts: [], vault_drafts: [], total: 0 })),
    });
    renderBlock();
    expect(screen.getByRole("status")).toHaveAttribute("aria-busy", "true");
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Gagal memuat tesis");
    fail = false;
    fireEvent.click(within(alert).getByRole("button", { name: "Coba lagi" }));
    expect(await screen.findByText("Net debt")).toBeInTheDocument();
  });

  it("editor in a full-height Sheet: ticker fixed, validation, unsent text kept per ticker, POST, sheet closes", async () => {
    let body: Record<string, unknown> | null = null;
    mockApi({
      "/api/thesis/BUMI": notFound,
      "/api/thesis/drafts": (_u: URL, init: RequestInit) => {
        if (init.method === "POST") {
          body = JSON.parse(String(init.body));
          return json({ ...draft }, 201);
        }
        return json({ drafts: body ? [draft] : [], vault_drafts: [], total: 0 });
      },
    });
    renderBlock();
    fireEvent.click(await screen.findByRole("button", { name: "Tulis draf tesis" }));
    const dialog = screen.getByRole("dialog", { name: "Tulis draf tesis" });
    expect(within(dialog).getByLabelText("Ticker")).toHaveValue("BUMI");
    expect(within(dialog).getByLabelText("Ticker")).toHaveAttribute("readonly");
    const save = within(dialog).getByRole("button", { name: "Simpan draf" });
    fireEvent.click(save);
    expect(within(dialog).getByText("Judul tesis wajib diisi")).toBeInTheDocument();
    fireEvent.change(within(dialog).getByLabelText("Judul"), { target: { value: "Batubara pulih" } });
    fireEvent.change(within(dialog).getByLabelText("Thesis statement"), { target: { value: "Utang turun" } });
    fireEvent.change(within(dialog).getByLabelText("Falsifiers 1"), { target: { value: " Harga < 100 " } });
    expect(JSON.parse(window.localStorage.getItem("investing.thesisDraft.BUMI") ?? "{}").title).toBe("Batubara pulih");

    // Closing keeps the unsent text; reopening restores it.
    fireEvent.click(within(dialog).getByRole("button", { name: "Tutup" }));
    fireEvent.click(screen.getByRole("button", { name: "Tulis draf tesis" }));
    const again = screen.getByRole("dialog", { name: "Tulis draf tesis" });
    expect(within(again).getByLabelText("Judul")).toHaveValue("Batubara pulih");

    fireEvent.click(within(again).getByRole("button", { name: "Simpan draf" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(body).toMatchObject({ ticker: "BUMI", title: "Batubara pulih", thesis_statement: "Utang turun", falsifiers: ["Harga < 100"], triggers_add: [], thesis_type: "fundamental", scenario_initial: "Menunggu" });
    expect(window.localStorage.getItem("investing.thesisDraft.BUMI")).toBeNull();
    expect(await screen.findByRole("list", { name: "Draf tesis BUMI" })).toHaveTextContent("ACCEPTANCE");
  });

  it("draft detail Sheet: Hapus draf asks first, then DELETE and the row is gone", async () => {
    let deleted = false;
    const del = vi.fn(() => {
      deleted = true;
      return new Response(null, { status: 204 });
    });
    mockApi({
      "/api/thesis/BUMI": notFound,
      [`/api/thesis/drafts/${draft.id}`]: (_u: URL, init: RequestInit) => (init.method === "DELETE" ? del() : json(draft)),
      "/api/thesis/drafts": () => json({ drafts: deleted ? [] : [draft], vault_drafts: [], total: deleted ? 0 : 1 }),
    });
    renderBlock();
    fireEvent.click(await screen.findByRole("button", { name: /ACCEPTANCE/ }));
    const sheet = screen.getByRole("dialog", { name: "Draf tesis" });
    expect(await within(sheet).findByText("Skenario: Menunggu", { exact: false })).toBeInTheDocument();
    fireEvent.click(within(sheet).getByRole("button", { name: "Hapus draf" }));
    expect(del).not.toHaveBeenCalled();
    const confirm = screen.getByRole("dialog", { name: "Hapus draf?" });
    fireEvent.click(within(confirm).getByRole("button", { name: "Hapus" }));
    await waitFor(() => expect(del).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(await screen.findByText("Belum ada tesis untuk BUMI")).toBeInTheDocument();
  });
});
