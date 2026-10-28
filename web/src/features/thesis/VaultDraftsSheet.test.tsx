import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import vault from "../../../../tests/fixtures/thesis_golden/drafts_vault.json";
import { json, mockApi } from "../../test/mockApi";
import DefaultSheet, { VaultDraftsSheet } from "./VaultDraftsSheet";
import type { VaultDraft } from "./types";

const [first, second] = vault.vault_drafts as unknown as VaultDraft[];

function Host() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Buka
      </button>
      <VaultDraftsSheet open={open} onClose={() => setOpen(false)} />
    </>
  );
}

const routes = () => ({
  "/api/thesis/drafts/vault": vault,
  [`/api/thesis/drafts/${first.id}`]: first,
  [`/api/thesis/drafts/${second.id}`]: second,
});
const calls = (f: ReturnType<typeof mockApi>, path: string) => f.mock.calls.filter(([u]) => new URL(String(u), "http://localhost").pathname === path).length;

afterEach(() => document.body.replaceChildren());

describe("VaultDraftsSheet (design.md §3.3 Vault drafts Sheet, D10)", () => {
  it("is the named and default export", () => {
    expect(DefaultSheet).toBe(VaultDraftsSheet);
  });

  it("loads the list only when open: a closed Sheet makes no request", async () => {
    const f = mockApi(routes());
    render(<Host />);
    await new Promise((r) => setTimeout(r, 20));
    expect(calls(f, "/api/thesis/drafts/vault")).toBe(0);
    fireEvent.click(screen.getByRole("button", { name: "Buka" }));
    await screen.findByRole("list", { name: "Draf vault" });
    expect(calls(f, "/api/thesis/drafts/vault")).toBe(1);
  });

  it("lists drafts as 2-line rows: title; status_lifecycle · file name", async () => {
    mockApi(routes());
    render(<Host />);
    fireEvent.click(screen.getByRole("button", { name: "Buka" }));
    const dialog = await screen.findByRole("dialog", { name: "Draf vault" });
    const rows = await within(dialog).findAllByRole("listitem");
    expect(rows).toHaveLength(2);
    expect(rows[0].querySelector('[data-line="1"]')).toHaveTextContent("Minyak naik");
    expect(rows[0].querySelector('[data-line="2"]')).toHaveTextContent("Berjalan · t01-minyak.md");
    expect(rows[1].querySelector('[data-line="1"]')).toHaveTextContent("t02-kosong");
    expect(rows[1].querySelector('[data-line="2"]')).toHaveTextContent("Menunggu (HOLD) · t02-kosong.md");
    for (const r of rows) expect(within(r).getByRole("button")).toHaveClass("min-h-14");
    expect(within(dialog).queryByRole("button", { name: /hapus/i })).toBeNull();
  });

  it("tap opens a read-only detail in the same Sheet: statement, falsifiers, file name; no Hapus", async () => {
    const f = mockApi(routes());
    render(<Host />);
    fireEvent.click(screen.getByRole("button", { name: "Buka" }));
    fireEvent.click(await screen.findByRole("button", { name: /Minyak naik/ }));
    const dialog = await screen.findByRole("dialog", { name: "Minyak naik" });
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    expect(await within(dialog).findByText("Harga minyak naik karena supply.")).toBeInTheDocument();
    expect(calls(f, `/api/thesis/drafts/${first.id}`)).toBe(1);
    expect(within(dialog).getByRole("heading", { name: "Falsifier" })).toBeInTheDocument();
    expect(within(dialog).getByText("Brent < 60")).toBeInTheDocument();
    expect(within(dialog).getByText(/t01-minyak\.md/)).toBeInTheDocument();
    expect(within(dialog).getByText(/baca saja/i)).toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: /hapus/i })).toBeNull();
    expect(f.mock.calls.every(([, init]) => !init || !(init as RequestInit).method || (init as RequestInit).method === "GET")).toBe(true);
  });

  it("a draft without falsifiers says so; Kembali returns to the list and restores focus to the row", async () => {
    mockApi(routes());
    render(<Host />);
    fireEvent.click(screen.getByRole("button", { name: "Buka" }));
    const row = await screen.findByRole("button", { name: /t02-kosong/ });
    fireEvent.click(row);
    const dialog = await screen.findByRole("dialog", { name: "t02-kosong" });
    expect(await within(dialog).findByText("Belum ada falsifier")).toBeInTheDocument();
    const back = within(dialog).getByRole("button", { name: "Kembali ke daftar draf vault" });
    expect(back).toHaveClass("min-h-11");
    fireEvent.click(back);
    expect(await screen.findByRole("dialog", { name: "Draf vault" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: /t02-kosong/ })).toHaveFocus());
  });

  it("closing the Sheet and reopening shows the list again, not the last detail", async () => {
    mockApi(routes());
    render(<Host />);
    fireEvent.click(screen.getByRole("button", { name: "Buka" }));
    fireEvent.click(await screen.findByRole("button", { name: /Minyak naik/ }));
    await screen.findByRole("dialog", { name: "Minyak naik" });
    fireEvent.click(screen.getByRole("button", { name: "Tutup" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Buka" }));
    expect(await screen.findByRole("dialog", { name: "Draf vault" })).toBeInTheDocument();
  });

  it("empty vault, list error with Coba lagi, detail error with Coba lagi", async () => {
    mockApi({ "/api/thesis/drafts/vault": { is_readonly: true, total: 0, vault_drafts: [] } });
    const a = render(<Host />);
    fireEvent.click(screen.getByRole("button", { name: "Buka" }));
    expect(await screen.findByText("Belum ada draf vault")).toBeInTheDocument();
    a.unmount();

    let fail = true;
    mockApi({ "/api/thesis/drafts/vault": () => (fail ? json({ detail: "x" }, 500) : json(vault)) });
    render(<Host />);
    fireEvent.click(screen.getByRole("button", { name: "Buka" }));
    expect(await screen.findByText("Gagal memuat draf vault")).toBeInTheDocument();
    fail = false;
    fireEvent.click(screen.getByRole("button", { name: "Coba lagi" }));
    expect(await screen.findByRole("button", { name: /Minyak naik/ })).toBeInTheDocument();
  });

  it("detail 404 reads as 'Draf tidak ditemukan', other errors as 'Gagal memuat draf'", async () => {
    mockApi({ "/api/thesis/drafts/vault": vault, [`/api/thesis/drafts/${first.id}`]: () => json({ detail: "nope" }, 404), [`/api/thesis/drafts/${second.id}`]: () => json({ detail: "boom" }, 500) });
    render(<Host />);
    fireEvent.click(screen.getByRole("button", { name: "Buka" }));
    fireEvent.click(await screen.findByRole("button", { name: /Minyak naik/ }));
    expect(await screen.findByText("Draf tidak ditemukan")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Kembali ke daftar draf vault" }));
    fireEvent.click(await screen.findByRole("button", { name: /t02-kosong/ }));
    expect(await screen.findByText("Gagal memuat draf")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Coba lagi" })).toBeInTheDocument();
  });
});
