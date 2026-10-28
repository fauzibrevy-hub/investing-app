import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { json, mockApi } from "../../../test/mockApi";
import { PROPOSALS, UPLOAD_DOC, proposal, proposalsResponse } from "../testing/extract";
import type { ExtractProposal, ProposalDocument } from "../types";
import { ProposalReview, conflictMessage } from "./ProposalReview";
import { ApiError } from "../../../lib/api";

const LIST = "/api/research/COAL/proposals";
const ACCEPT = (id: string) => `/api/research/proposals/${id}/accept`;
const REJECT = (id: string) => `/api/research/proposals/${id}/reject`;

interface Sent {
  path: string;
  key: string | undefined;
  body: string | undefined;
}

/** Backend tiruan dengan keadaan: daftar usulan yang berubah mengikuti keputusan. */
function backend(opts: { held?: Record<string, string>; documents?: ProposalDocument[]; failFirst?: boolean } = {}) {
  let items: ExtractProposal[] = PROPOSALS.map((p) => ({ ...p }));
  const sent: Sent[] = [];
  const held = new Map(Object.entries(opts.held ?? {}));
  let failed = false;
  const routes: Record<string, unknown> = { [LIST]: () => json(proposalsResponse(items, opts.documents)) };
  for (const p of PROPOSALS) {
    routes[ACCEPT(p.id)] = (_u: URL, init: RequestInit) => {
      const h = (init.headers ?? {}) as Record<string, string>;
      sent.push({ path: ACCEPT(p.id), key: h["Idempotency-Key"], body: init.body as string | undefined });
      if (opts.failFirst && !failed) {
        failed = true;
        return json({ detail: "boom" }, 500);
      }
      const replace = typeof init.body === "string" && init.body.includes('"replace":true');
      const current = held.get(p.kpi_key);
      if (current && !replace) return json({ detail: `${p.metric} 2026Q2 already holds ${current}; send {"replace": true} to replace it` }, 409);
      items = items.map((i) => (i.id === p.id ? { ...i, status: "accepted" as const } : i));
      held.set(p.kpi_key, `${p.value} ${p.unit} (Presentasi 2026-10-20)`);
      return json({ proposal: items.find((i) => i.id === p.id), kpi: {}, evidence: {}, replaced: !!current });
    };
    routes[REJECT(p.id)] = (_u: URL, init: RequestInit) => {
      const h = (init.headers ?? {}) as Record<string, string>;
      sent.push({ path: REJECT(p.id), key: h["Idempotency-Key"], body: init.body as string | undefined });
      items = items.map((i) => (i.id === p.id ? { ...i, status: "rejected" as const } : i));
      return json({ proposal: items.find((i) => i.id === p.id) });
    };
  }
  mockApi(routes);
  return { sent, setItems: (next: ExtractProposal[]) => (items = next) };
}

const accept = (id: string) => screen.getByRole("button", { name: new RegExp(`^Terima ${PROPOSALS.find((p) => p.id === id)!.metric_label}`) });

describe("ProposalReview: states", () => {
  it("loading: a skeleton, no list", () => {
    mockApi({ [LIST]: () => new Promise<Response>(() => {}) });
    render(<ProposalReview ticker="COAL" />);
    expect(screen.getByText("Memuat usulan…")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Terima/ })).toBeNull();
  });

  it("empty: says how to get proposals", async () => {
    mockApi({ [LIST]: proposalsResponse([]) });
    render(<ProposalReview ticker="COAL" />);
    expect(await screen.findByText(/Belum ada usulan\. Unggah PDF atau PPTX/)).toBeInTheDocument();
  });

  it("error: the real error with a retry", async () => {
    let n = 0;
    mockApi({ [LIST]: () => (++n === 1 ? json({ detail: "boom" }, 500) : json(proposalsResponse())) });
    render(<ProposalReview ticker="COAL" />);
    expect(await screen.findByText("boom")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Coba lagi" }));
    expect(await screen.findByRole("heading", { name: /Produksi/ })).toBeInTheDocument();
  });

  it("ready: one group per KPI with the unit in the heading, the value without a unit, page, snippet and counts", async () => {
    backend();
    render(<ProposalReview ticker="COAL" />);
    const heading = await screen.findByRole("heading", { name: "Produksi Q2 26 (MT)" });
    const group = heading.closest("section")!;
    const items = within(group).getAllByRole("listitem");
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveTextContent("5,4");
    expect(items[0]).not.toHaveTextContent("Mt");
    expect(items[0]).toHaveTextContent("hlm. 3");
    expect(items[0]).toHaveTextContent("Produksi batubara kuartal ini 5,4 juta ton");
    expect(items[0].querySelector('[data-provenance="usulan"]')).toHaveTextContent("usulan");
    expect(screen.getByRole("heading", { name: "Biaya tunai Q2 26 (USD/T)" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Nisbah kupas Q2 26 (BCM/T)" })).toBeInTheDocument();
    expect(screen.getByText("4 menunggu · 0 diterima · 0 ditolak")).toBeInTheDocument();
  });

  it("a slide proposal is located by slide and a currency KPI is scaled with the unit in the heading", async () => {
    const items = [proposal({ id: "p-rev", metric: "revenue_x", metric_label: "Pendapatan", unit: "ccy", value: 2_500_000_000_000, page: 4, locator: "slide", snippet: "Pendapatan 2,5 triliun", rank: 1 })];
    mockApi({ [LIST]: proposalsResponse(items, [{ ...UPLOAD_DOC, format: "pptx", locator: "slide" }]) });
    render(<ProposalReview ticker="COAL" />);
    const heading = await screen.findByRole("heading", { name: /Pendapatan/ });
    expect(heading).toHaveTextContent("(MATA UANG LAPORAN, MILIAR)");
    const row = heading.closest("section")!.querySelector("li")!;
    expect(row).toHaveTextContent("slide 4");
    expect(row.querySelector(".num")).toHaveTextContent("2.500");
  });

  it("filters by document and follows a freshly uploaded one", async () => {
    const second: ProposalDocument = { ...UPLOAD_DOC, id: "d-u-0002", kind: "laporan", kind_label: "Laporan", doc_date: "2026-09-01", format: "pptx", locator: "slide" };
    const items = [...PROPOSALS, proposal({ id: "p-other", document_id: "d-u-0002", metric: "production_mt", metric_label: "Produksi", unit: "Mt", value: 4.9, page: 2, locator: "slide", snippet: "Produksi 4,9 juta ton", rank: 1 })];
    mockApi({ [LIST]: proposalsResponse(items, [UPLOAD_DOC, second]) });
    const { rerender } = render(<ProposalReview ticker="COAL" />);
    await screen.findByRole("heading", { name: /Produksi/ });
    expect(screen.getAllByRole("listitem")).toHaveLength(5);
    // Semua dokumen: baris menyebut dokumennya.
    expect(within(document.querySelector('[data-proposal="p-other"]') as HTMLElement).getByText("Laporan 1 Sep 2026 · PPTX")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Dokumen"), { target: { value: "d-u-0002" } });
    expect(screen.getAllByRole("listitem")).toHaveLength(1);
    expect(screen.getByLabelText("Dokumen")).toHaveValue("d-u-0002");
    rerender(<ProposalReview ticker="COAL" focusDocumentId="d-u-0001" />);
    await waitFor(() => expect(screen.getAllByRole("listitem")).toHaveLength(4));
  });
});

describe("ProposalReview: accept", () => {
  it("accepts a proposal for an empty cell: POST without a body, badge turns to diterima and Paket is told", async () => {
    const { sent } = backend();
    render(<ProposalReview ticker="COAL" />);
    await screen.findByRole("heading", { name: /Biaya tunai/ });
    fireEvent.click(accept("p-cash-1"));
    await waitFor(() => expect(document.querySelector('[data-proposal="p-cash-1"]')).toHaveAttribute("data-status", "accepted"));
    const row = document.querySelector('[data-proposal="p-cash-1"]') as HTMLElement;
    expect(row.querySelector('[data-provenance="manual"]')).toHaveTextContent("diterima");
    expect(within(row).queryByRole("button", { name: /^Terima/ })).toBeNull();
    expect(within(row).getByRole("status")).toHaveTextContent("Diterima; tersimpan di Paket");
    expect(sent).toHaveLength(1);
    expect(sent[0].body).toBeUndefined();
    expect(sent[0].key).toBeTruthy();
  });

  it("an owner value in the cell asks to confirm; Batal sends nothing more and returns focus to Terima", async () => {
    const { sent } = backend({ held: { "production_mt:2026Q2": "6 Mt (laporan operasional sintetis)" } });
    render(<ProposalReview ticker="COAL" />);
    await screen.findByRole("heading", { name: /Produksi/ });
    const first = document.querySelector('[data-proposal="p-prod-1"]') as HTMLElement;
    fireEvent.click(within(first).getByRole("button", { name: /^Terima/ }));
    const confirm = await within(first).findByRole("group", { name: /Konfirmasi ganti nilai Produksi Q2 26/ });
    expect(confirm).toHaveTextContent("Sel ini sudah berisi 6 Mt (laporan operasional sintetis). Ganti dengan 5,40 dari dokumen ini?");
    expect(within(confirm).getByRole("button", { name: /^Ganti dengan/ })).toHaveFocus();
    // Selama konfirmasi terbuka, Terima dan Tolak tidak bisa ditekan lagi.
    expect(within(first).getByRole("button", { name: /^Terima/ })).toHaveAttribute("aria-disabled", "true");
    fireEvent.click(within(confirm).getByRole("button", { name: "Batal" }));
    expect(within(first).queryByRole("group")).toBeNull();
    expect(within(first).getByRole("button", { name: /^Terima/ })).toHaveFocus();
    expect(sent).toHaveLength(1);
    expect(first).toHaveAttribute("data-status", "pending");
  });

  it("Ganti sends replace: true with its own key and reports the replacement", async () => {
    const { sent } = backend({ held: { "production_mt:2026Q2": "6 Mt (laporan operasional sintetis)" } });
    render(<ProposalReview ticker="COAL" />);
    await screen.findByRole("heading", { name: /Produksi/ });
    const first = document.querySelector('[data-proposal="p-prod-1"]') as HTMLElement;
    fireEvent.click(within(first).getByRole("button", { name: /^Terima/ }));
    fireEvent.click(await within(first).findByRole("button", { name: /^Ganti dengan/ }));
    await waitFor(() => expect(first).toHaveAttribute("data-status", "accepted"));
    expect(sent.map((s) => s.body)).toEqual([undefined, JSON.stringify({ replace: true })]);
    expect(sent[1].key).not.toBe(sent[0].key);
    expect(within(first).getByRole("status")).toHaveTextContent("Nilai pemilik diganti; tersimpan di Paket");
    expect(within(first).queryByRole("group")).toBeNull();
    // Satu nilai per KPI: yang lain masih menunggu dan menerimanya meminta konfirmasi lagi.
    expect(screen.getByText(/Satu nilai sudah diterima; menerima yang lain meminta konfirmasi/)).toBeInTheDocument();
    const second = document.querySelector('[data-proposal="p-prod-2"]') as HTMLElement;
    fireEvent.click(within(second).getByRole("button", { name: /^Terima/ }));
    expect(await within(second).findByRole("group", { name: /Konfirmasi ganti nilai/ })).toBeInTheDocument();
  });

  it("an explicit unknown held by the owner reads as Tidak diketahui", () => {
    expect(conflictMessage(new ApiError("http", 'production_mt 2026Q2 already holds unknown (catatan); send {"replace": true} to replace it', { status: 409 }))).toEqual({ held: "Tidak diketahui", text: "Sel ini sudah berisi Tidak diketahui (catatan)." });
  });

  it("a failed accept shows the error and the retry reuses the same Idempotency-Key", async () => {
    const { sent } = backend({ failFirst: true });
    render(<ProposalReview ticker="COAL" />);
    await screen.findByRole("heading", { name: /Biaya tunai/ });
    fireEvent.click(accept("p-cash-1"));
    expect(await screen.findByRole("alert")).toHaveTextContent("boom");
    fireEvent.click(accept("p-cash-1"));
    await waitFor(() => expect(document.querySelector('[data-proposal="p-cash-1"]')).toHaveAttribute("data-status", "accepted"));
    expect(sent).toHaveLength(2);
    expect(sent[1].key).toBe(sent[0].key);
  });

  it("a 409 that is not an owner value (already decided) is shown and the list is read again", async () => {
    mockApi({
      [LIST]: (() => {
        let n = 0;
        return () => json(proposalsResponse(n++ === 0 ? PROPOSALS : PROPOSALS.map((p) => (p.id === "p-cash-1" ? { ...p, status: "rejected" as const } : p))));
      })(),
      [ACCEPT("p-cash-1")]: () => json({ detail: "The proposal was already rejected" }, 409),
    });
    render(<ProposalReview ticker="COAL" />);
    await screen.findByRole("heading", { name: /Biaya tunai/ });
    fireEvent.click(accept("p-cash-1"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Usulan ini sudah diputuskan");
    await waitFor(() => expect(document.querySelector('[data-proposal="p-cash-1"]')).toHaveAttribute("data-status", "rejected"));
  });
});

describe("ProposalReview: reject", () => {
  it("Tolak stores nothing else: POST reject, badge reads ditolak, no buttons left", async () => {
    const { sent } = backend();
    render(<ProposalReview ticker="COAL" />);
    await screen.findByRole("heading", { name: /Nisbah kupas/ });
    fireEvent.click(screen.getByRole("button", { name: /^Tolak Nisbah kupas/ }));
    const row = document.querySelector('[data-proposal="p-sr-1"]') as HTMLElement;
    await waitFor(() => expect(row).toHaveAttribute("data-status", "rejected"));
    expect(row.querySelector('[data-provenance="usulan"]')).toHaveTextContent("ditolak");
    expect(within(row).queryByRole("button")).toBeNull();
    expect(within(row).getByRole("status")).toHaveTextContent("Ditolak; tidak ada yang tersimpan");
    expect(sent).toEqual([{ path: REJECT("p-sr-1"), key: expect.any(String), body: undefined }]);
    expect(screen.getByText("3 menunggu · 0 diterima · 1 ditolak")).toBeInTheDocument();
  });
});

describe("ProposalReview: superseded", () => {
  it("a superseded row is muted, reads digantikan, has no Terima or Tolak, and counts it", async () => {
    const items = PROPOSALS.map((p) => (p.id === "p-prod-2" ? { ...p, status: "superseded" as const } : p));
    mockApi({ [LIST]: () => json(proposalsResponse(items)) });
    render(<ProposalReview ticker="COAL" />);
    await screen.findByRole("heading", { name: "Produksi Q2 26 (MT)" });
    const row = document.querySelector('[data-proposal="p-prod-2"]') as HTMLElement;
    expect(row).toHaveAttribute("data-status", "superseded");
    expect(row.className).toMatch(/opacity-70/);
    expect(row.querySelector('[data-provenance="usulan"]')).toHaveTextContent("digantikan");
    expect(within(row).queryByRole("button")).toBeNull();
    expect(screen.getByText("3 menunggu · 0 diterima · 0 ditolak · 1 digantikan")).toBeInTheDocument();
    expect(document.querySelector('[data-proposal="p-prod-1"] button')).not.toBeNull();
  });
});

