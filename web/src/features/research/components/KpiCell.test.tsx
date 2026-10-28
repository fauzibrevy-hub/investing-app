import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { json, mockApi } from "../../../test/mockApi";
import { golden } from "../testing/canon";
import type { KpiPackCell, PackResponse } from "../types";
import { KpiCell, SAVED_TEXT, UNKNOWN_TEXT } from "./KpiCell";

const PACK = golden<PackResponse>("pack-COAL");
const production = PACK.dimensions.flatMap((d) => d.metrics).find((m) => m.id === "production_mt")!;
const cells = production.cells as KpiPackCell[];
const entered = cells.find((c) => c.state === "entered")!;
const empty = cells.find((c) => c.state === "not_entered")!;
const PATH = (c: KpiPackCell) => `/api/research/COAL/kpi/production_mt/${c.fy}/${c.q}`;

function capture(path: string, answer: () => Response = () => json({ state: "entered" })) {
  const puts: Array<{ method?: string; body: Record<string, unknown>; key: string | null }> = [];
  mockApi({
    [path]: (_u: URL, init: RequestInit) => {
      puts.push({ method: init.method, body: JSON.parse(String(init.body)), key: new Headers(init.headers).get("Idempotency-Key") });
      return answer();
    },
  });
  return puts;
}

const cell = (c: KpiPackCell) => render(<KpiCell ticker="COAL" metric="production_mt" label="Produksi" unit="Mt" cell={c} />);

describe("KpiCell (design.md §3.6, §6, §8)", () => {
  it("an entered value shows the number with a manual badge and the document date", () => {
    cell(entered);
    expect(screen.getByRole("button", { name: "Produksi Q2 26: 6, ubah KPI" })).toHaveTextContent("6");
    const badge = screen.getByText("manual · 20 Okt 2026");
    expect(badge).toHaveAttribute("data-provenance", "manual");
    expect(badge).toHaveAttribute("title", "Diisi pemilik · dokumen laporan operasional sintetis · 20 Okt 2026");
  });

  it("an empty cell is the Isi action, never a dash", () => {
    cell(empty);
    expect(screen.getByRole("button", { name: /^Isi Produksi Q\d \d\d$/ })).toBeInTheDocument();
    expect(screen.getByText("Isi")).toBeInTheDocument();
    expect(screen.queryByText("—")).toBeNull();
  });

  it("Isi opens the editor; submitting saves PUT /kpi/{metric}/{fy}/{q} with provenance, shows the saved value, announces Tersimpan and returns focus", async () => {
    const puts = capture(PATH(empty));
    cell(empty);
    const trigger = screen.getByRole("button", { name: /^Isi Produksi/ });
    fireEvent.click(trigger);
    const form = screen.getByRole("form", { name: /KPI Produksi/ });
    expect(within(form).getByLabelText("Nilai (Mt)")).toHaveFocus();
    fireEvent.change(within(form).getByLabelText("Nilai (Mt)"), { target: { value: "5,5" } });
    fireEvent.change(within(form).getByLabelText("Dokumen (URL atau teks)"), { target: { value: "https://example.test/ops" } });
    fireEvent.change(within(form).getByLabelText("Tanggal dokumen"), { target: { value: "2026-10-20" } });
    fireEvent.submit(form);
    await waitFor(() => expect(puts).toHaveLength(1));
    expect(puts[0]).toMatchObject({ method: "PUT", body: { value: 5.5, document: "https://example.test/ops", document_date: "2026-10-20" } });
    expect(puts[0].key).toBeTruthy();
    await waitFor(() => expect(screen.queryByRole("form")).toBeNull());
    const live = screen.getByRole("status");
    expect(live).toHaveAttribute("aria-live", "polite");
    expect(live).toHaveTextContent(SAVED_TEXT);
    // The saved value shows before the pack reloads, so the old gap never flashes back.
    const saved = screen.getByRole("button", { name: /^Produksi .*: 5,50, ubah KPI$/ });
    expect(saved).toHaveFocus();
    expect(screen.getByText(/^manual · /)).toBeInTheDocument();
  });

  it("a retry after a failed save reuses the key; an edited draft gets a new one", async () => {
    let n = 0;
    const keys: string[] = [];
    mockApi({
      [PATH(empty)]: (_u: URL, init: RequestInit) => {
        keys.push(new Headers(init.headers).get("Idempotency-Key") ?? "");
        n += 1;
        return n < 3 ? json({ detail: "boom" }, 500) : json({});
      },
    });
    cell(empty);
    fireEvent.click(screen.getByRole("button", { name: /^Isi Produksi/ }));
    const form = screen.getByRole("form", { name: /KPI Produksi/ });
    fireEvent.change(within(form).getByLabelText("Nilai (Mt)"), { target: { value: "5,5" } });
    fireEvent.change(within(form).getByLabelText("Dokumen (URL atau teks)"), { target: { value: "laporan" } });
    fireEvent.change(within(form).getByLabelText("Tanggal dokumen"), { target: { value: "2026-10-20" } });
    fireEvent.submit(form);
    await waitFor(() => expect(keys).toHaveLength(1));
    await waitFor(() => expect(within(form).getByRole("button", { name: "Simpan" })).not.toHaveAttribute("aria-busy", "true"));
    fireEvent.submit(form);
    await waitFor(() => expect(keys).toHaveLength(2));
    expect(keys[1]).toBe(keys[0]);
    await waitFor(() => expect(within(form).getByRole("button", { name: "Simpan" })).not.toHaveAttribute("aria-busy", "true"));
    fireEvent.change(within(form).getByLabelText("Nilai (Mt)"), { target: { value: "5,6" } });
    fireEvent.submit(form);
    await waitFor(() => expect(keys).toHaveLength(3));
    expect(keys[2]).not.toBe(keys[0]);
  });

  it("Tidak diketahui sends {unknown: true} without a value", async () => {
    const puts = capture(PATH(empty));
    cell(empty);
    fireEvent.click(screen.getByRole("button", { name: /^Isi Produksi/ }));
    fireEvent.click(screen.getByLabelText(UNKNOWN_TEXT));
    expect(screen.getByLabelText("Nilai (Mt)")).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Dokumen (URL atau teks)"), { target: { value: "paparan publik" } });
    fireEvent.submit(screen.getByRole("form"));
    await waitFor(() => expect(puts).toHaveLength(1));
    expect(puts[0].body).toMatchObject({ unknown: true, document: "paparan publik" });
    expect(puts[0].body).not.toHaveProperty("value");
  });

  it("an unknown cell renders the words Tidak diketahui as text", () => {
    cell({ ...entered, state: "unknown", value: null, unknown: true });
    expect(screen.getByRole("button", { name: `Produksi Q2 26: ${UNKNOWN_TEXT}, ubah KPI` })).toHaveTextContent(UNKNOWN_TEXT);
  });

  it("Esc cancels without a request and returns focus", () => {
    const fetchMock = mockApi({});
    cell(entered);
    const trigger = screen.getByRole("button", { name: /Produksi Q2 26/ });
    fireEvent.click(trigger);
    expect(screen.getByLabelText("Nilai (Mt)")).toHaveValue("6");
    fireEvent.keyDown(screen.getByLabelText("Nilai (Mt)"), { key: "Escape" });
    expect(screen.queryByRole("form")).toBeNull();
    expect(trigger).toHaveFocus();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("validation and 422 errors render inline under the cell", async () => {
    capture(PATH(empty), () => json({ detail: [{ type: "value_error", loc: ["body", "document_date"], msg: "tanggal dokumen di luar jangkauan", input: null }] }, 422));
    cell(empty);
    fireEvent.click(screen.getByRole("button", { name: /^Isi Produksi/ }));
    fireEvent.submit(screen.getByRole("form"));
    expect(screen.getByText("Isi nilai berupa angka, atau pilih Tidak diketahui")).toBeInTheDocument();
    expect(screen.getByText("Tulis URL atau nama dokumen sumber")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Nilai (Mt)"), { target: { value: "5" } });
    fireEvent.change(screen.getByLabelText("Dokumen (URL atau teks)"), { target: { value: "x" } });
    fireEvent.submit(screen.getByRole("form"));
    expect(await screen.findByText("tanggal dokumen di luar jangkauan")).toBeInTheDocument();
    expect(screen.getByLabelText("Tanggal dokumen")).toHaveAttribute("aria-invalid", "true");
  });

  it("an annual KPI outside Q4 is not applicable (no action)", () => {
    const reserves = PACK.dimensions.flatMap((d) => d.metrics).find((m) => m.id === "reserves_mt")!;
    const na = (reserves.cells as KpiPackCell[]).find((c) => c.state === "not_applicable")!;
    render(<KpiCell ticker="COAL" metric="reserves_mt" label="Cadangan" unit="Mt" cell={na} />);
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getByText(/tidak berlaku/)).toBeInTheDocument();
  });
});
