import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import desk from "../../../../tests/fixtures/research/canon/desk.json";
import summary from "../../../../tests/fixtures/thesis_golden/summary.json";
import vault from "../../../../tests/fixtures/thesis_golden/drafts_vault.json";
import { json, mockApi } from "../../test/mockApi";
import { expectNoVerdictWords } from "../research/screenAsserts.testutil";
import { setViewport } from "../research/testing/viewport";
import type { DeskResponse } from "../research/types";
import DefaultPerusahaan, { Perusahaan } from "./Perusahaan";

const DESK = desk as unknown as DeskResponse;
const EMPTY_DRAFTS = { drafts: [], vault_drafts: [], total: 0 };

/** Draf biasa; `ticker` di luar semesta riset membuatnya masuk "Tesis lain". */
const draft = (ticker: string, updated = "2026-10-12T01:00:00+00:00") => ({
  id: `draft_${ticker}_1`,
  ticker,
  title: `Tesis ${ticker}`,
  thesis_statement: "x",
  thesis_type: "fundamental",
  is_primary: false,
  scenario_status: "Menunggu",
  story_status: "Belum terkonfirmasi",
  falsifiers: ["y"],
  triggers_add: [],
  triggers_exit: [],
  notes: "",
  created_at: updated,
  updated_at: updated,
});

function renderScreen() {
  return render(
    <MemoryRouter initialEntries={["/perusahaan"]}>
      <Routes>
        <Route path="/perusahaan" element={<Perusahaan />} />
        <Route path="*" element={<p>elsewhere</p>} />
      </Routes>
    </MemoryRouter>,
  );
}

const calls = (f: ReturnType<typeof mockApi>, path: string) => f.mock.calls.filter(([u]) => new URL(String(u), "http://localhost").pathname === path).length;

beforeEach(() => setViewport(393));
afterEach(() => setViewport(1280));

describe("Perusahaan #/perusahaan (design.md §3.3)", () => {
  it("is the named and default export", () => {
    expect(DefaultPerusahaan).toBe(Perusahaan);
  });

  it("blocks in order: header with Tambah, Dipegang, Manual, Pembanding (n), footer; Tesis lain hidden when both lists are empty", async () => {
    mockApi({ "/api/research/desk": DESK, "/api/thesis/summary": { root_ok: true, generated_at: 0, issuers: [] }, "/api/thesis/drafts": EMPTY_DRAFTS });
    renderScreen();
    await screen.findByRole("region", { name: "Dipegang" });
    expect(screen.getByRole("heading", { level: 1, name: "Perusahaan" })).toBeInTheDocument();
    expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent)).toEqual(["Dipegang", "Manual", "Pembanding (1)"]);
    expect(screen.queryByRole("region", { name: /Tesis lain/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Draf vault/ })).toBeNull();
    const footer = screen.getByRole("contentinfo");
    expect(footer).toHaveTextContent("Bukan saran investasi");
    expect(footer).toHaveTextContent("Umur data");
  });

  it("Tambah is a 44 px link to #/perusahaan/tambah, without from= (a push inside the tab)", async () => {
    mockApi({ "/api/research/desk": DESK, "/api/thesis/summary": { root_ok: true, generated_at: 0, issuers: [] }, "/api/thesis/drafts": EMPTY_DRAFTS });
    renderScreen();
    const add = await screen.findByRole("link", { name: "Tambah" });
    expect(add).toHaveAttribute("href", "/perusahaan/tambah");
    expect(add).toHaveClass("min-h-11", "min-w-11");
  });

  it("renders CoverageList rows: held tickers first, each linking to its company page", async () => {
    mockApi({ "/api/research/desk": DESK, "/api/thesis/summary": { root_ok: true, generated_at: 0, issuers: [] }, "/api/thesis/drafts": EMPTY_DRAFTS });
    renderScreen();
    const held = await screen.findByRole("list", { name: "Dipegang" });
    expect(within(held).getAllByRole("listitem").map((li) => li.getAttribute("data-company"))).toEqual(["BANK", "COAL", "NONE"]);
    expect(within(held).getByRole("link", { name: /COAL/ })).toHaveAttribute("href", "/perusahaan/COAL");
  });

  it("loading: a status skeleton, no content; error: Gagal memuat Perusahaan with Coba lagi that reloads", async () => {
    let fail = true;
    mockApi({
      "/api/research/desk": () => (fail ? json({ detail: "x" }, 500) : json(DESK)),
      "/api/thesis/summary": { root_ok: true, generated_at: 0, issuers: [] },
      "/api/thesis/drafts": EMPTY_DRAFTS,
    });
    renderScreen();
    expect(screen.getByRole("status")).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("link", { name: "Tambah" })).toBeInTheDocument();
    expect(await screen.findByText("Gagal memuat Perusahaan")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Dipegang" })).toBeNull();
    fail = false;
    fireEvent.click(screen.getByRole("button", { name: "Coba lagi" }));
    expect(await screen.findByRole("region", { name: "Dipegang" })).toBeInTheDocument();
  });

  describe("Tesis lain (n) (prd.md O1)", () => {
    const issuers = (summary as { issuers: Array<{ ticker: string }> }).issuers;
    // desk.json: BANK COAL NONE BUST REST COPR. AAAA BBBB CCCC dossier; draf COAL (dalam semesta), ZZZZ, AAAA.
    const routes = (extra: Record<string, unknown> = {}) => ({
      "/api/research/desk": DESK,
      "/api/thesis/summary": summary,
      "/api/thesis/drafts": { drafts: [draft("COAL"), draft("ZZZZ", "2026-10-20T01:00:00+00:00"), draft("AAAA", "2026-10-01T01:00:00+00:00")], vault_drafts: vault.vault_drafts, total: 3 },
      ...extra,
    });

    it("is a lipat block after Pembanding: collapsed, title counts tickers outside the research universe", async () => {
      mockApi(routes());
      renderScreen();
      const toggle = await screen.findByRole("button", { name: "Tesis lain (4)" });
      expect(issuers).toHaveLength(3);
      expect(toggle).toHaveAttribute("aria-expanded", "false");
      expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent)).toEqual(["Dipegang", "Manual", "Pembanding (1)", "Tesis lain (4)"]);
      expect(screen.queryByRole("list", { name: "Tesis lain" })).toBeNull();
      fireEvent.click(toggle);
      expect(toggle).toHaveAttribute("aria-expanded", "true");
      expect(screen.getByRole("list", { name: "Tesis lain" })).toBeInTheDocument();
    });

    it("one row per ticker: TICKER · Dossier/Draf · updated date; a ticker in the universe never appears", async () => {
      mockApi(routes());
      renderScreen();
      fireEvent.click(await screen.findByRole("button", { name: /Tesis lain/ }));
      const rows = within(screen.getByRole("list", { name: "Tesis lain" })).getAllByRole("listitem").filter((r) => r.hasAttribute("data-thesis"));
      const text = (t: string) => rows.find((r) => r.getAttribute("data-thesis") === t)!;
      expect(rows.map((r) => r.getAttribute("data-thesis")).sort()).toEqual(["AAAA", "BBBB", "CCCC", "ZZZZ"]);
      expect(rows.some((r) => r.getAttribute("data-thesis") === "COAL")).toBe(false);
      expect(text("AAAA")).toHaveTextContent(/AAAA\s*Dossier/);
      expect(text("AAAA")).toHaveTextContent("15 Sep 2025");
      expect(text("BBBB")).toHaveTextContent(/BBBB\s*Dossier/);
      expect(text("ZZZZ")).toHaveTextContent(/ZZZZ\s*Draf/);
      expect(text("ZZZZ")).toHaveTextContent("20 Okt 2026");
    });

    it("tap → thesis-only company page #/perusahaan/<T>?from=perusahaan", async () => {
      mockApi(routes());
      renderScreen();
      fireEvent.click(await screen.findByRole("button", { name: /Tesis lain/ }));
      const link = within(screen.getByRole("list", { name: "Tesis lain" })).getByRole("link", { name: /ZZZZ/ });
      expect(link).toHaveAttribute("href", "/perusahaan/ZZZZ?from=perusahaan");
      expect(link).toHaveClass("min-h-14");
    });

    it("last row Draf vault (n) opens VaultDraftsSheet, loading drafts/vault only on open", async () => {
      const f = mockApi(routes({ "/api/thesis/drafts/vault": vault }));
      renderScreen();
      fireEvent.click(await screen.findByRole("button", { name: /Tesis lain/ }));
      const items = within(screen.getByRole("list", { name: "Tesis lain" })).getAllByRole("listitem");
      expect(items[items.length - 1]).toHaveTextContent("Draf vault (2)");
      expect(calls(f, "/api/thesis/drafts/vault")).toBe(0);
      const open = screen.getByRole("button", { name: "Draf vault (2)" });
      expect(open).toHaveAttribute("aria-haspopup", "dialog");
      fireEvent.click(open);
      const dialog = await screen.findByRole("dialog", { name: "Draf vault" });
      expect(await within(dialog).findByRole("button", { name: /Minyak naik/ })).toBeInTheDocument();
      expect(calls(f, "/api/thesis/drafts/vault")).toBe(1);
    });

    it("closing the VaultDraftsSheet returns focus to the Draf vault row even when the tap did not focus it (iOS Safari)", async () => {
      mockApi(routes({ "/api/thesis/drafts/vault": vault }));
      renderScreen();
      fireEvent.click(await screen.findByRole("button", { name: /Tesis lain/ }));
      const row = screen.getByRole("button", { name: "Draf vault (2)" });
      // `fireEvent.click` tidak memfokuskan tombol, seperti ketukan di WebKit iOS.
      expect(document.activeElement).not.toBe(row);
      fireEvent.click(row);
      const dialog = await screen.findByRole("dialog", { name: "Draf vault" });
      fireEvent.click(within(dialog).getByRole("button", { name: "Tutup" }));
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      expect(screen.getByRole("button", { name: "Draf vault (2)" })).toHaveFocus();
    });

    it("only vault drafts: the block shows with the Draf vault row alone; no tickers, no vault drafts: hidden", async () => {
      mockApi({ "/api/research/desk": DESK, "/api/thesis/summary": { root_ok: true, generated_at: 0, issuers: [] }, "/api/thesis/drafts": { drafts: [], vault_drafts: vault.vault_drafts, total: 0 } });
      const a = renderScreen();
      fireEvent.click(await screen.findByRole("button", { name: /Tesis lain/ }));
      expect(screen.getByRole("button", { name: "Draf vault (2)" })).toBeInTheDocument();
      expect(within(screen.getByRole("list", { name: "Tesis lain" })).getAllByRole("listitem")).toHaveLength(1);
      a.unmount();

      mockApi({ "/api/research/desk": DESK, "/api/thesis/summary": { root_ok: true, generated_at: 0, issuers: [] }, "/api/thesis/drafts": { drafts: [draft("COAL")], vault_drafts: [], total: 1 } });
      renderScreen();
      await screen.findByRole("region", { name: "Dipegang" });
      expect(screen.queryByText(/Tesis lain/)).toBeNull();
    });

    it("a failing thesis source never empties coverage: coverage shows, the failure is named with Coba lagi", async () => {
      mockApi({ "/api/research/desk": DESK, "/api/thesis/summary": () => json({ detail: "x" }, 500), "/api/thesis/drafts": EMPTY_DRAFTS });
      renderScreen();
      expect(await screen.findByRole("region", { name: "Dipegang" })).toBeInTheDocument();
      expect(await screen.findByText("Gagal memuat tesis lain")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Coba lagi" })).toBeInTheDocument();
    });

    it("the thesis summary stays loading: coverage is not held back by it", async () => {
      mockApi({ "/api/research/desk": DESK, "/api/thesis/summary": () => new Promise<Response>(() => {}), "/api/thesis/drafts": EMPTY_DRAFTS });
      renderScreen();
      expect(await screen.findByRole("region", { name: "Dipegang" })).toBeInTheDocument();
      expect(screen.queryByText(/Tesis lain/)).toBeNull();
    });
  });

  it("uses no verdict words", async () => {
    mockApi({ "/api/research/desk": DESK, "/api/thesis/summary": summary, "/api/thesis/drafts": { drafts: [], vault_drafts: [], total: 0 } });
    const { container } = renderScreen();
    await screen.findByRole("region", { name: "Dipegang" });
    fireEvent.click(await screen.findByRole("button", { name: /Tesis lain/ }));
    expectNoVerdictWords(container);
  });
});
