import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { json, mockApi } from "../../../test/mockApi";
import { golden } from "../testing/canon";
import type { ClaimsResponse } from "../types";
import { formatDate } from "../format";
import { claimPreview } from "./ClaimCard";
import { ClaimEditor, parseNumber, todayWib } from "./ClaimEditor";

const METRICS = golden<ClaimsResponse>("claims-COAL").metrics;
const FAR = "2099-12-31";

function fill(label: string | RegExp, value: string) {
  fireEvent.change(screen.getByLabelText(label), { target: { value } });
}

function editor(onDone = vi.fn()) {
  render(<ClaimEditor ticker="COAL" metrics={METRICS} packLabel="Tambang batubara" currency="USD" onDone={onDone} />);
  return onDone;
}

const field422 = (field: string, msg: string) => json({ detail: [{ type: "value_error", loc: ["body", field], msg, input: null }] }, 422);

describe("ClaimEditor (design.md §3.4, §6)", () => {
  it("groups the metric picker into Inti, the ticker's pack and Harga", () => {
    editor();
    const picker = screen.getByLabelText("Metrik");
    const groups = within(picker).getAllByRole("group").map((g) => g.getAttribute("label"));
    expect(groups).toEqual(["Inti", "Tambang batubara", "Harga"]);
    expect(within(picker).getByRole("option", { name: "Laba bersih" })).toBeInTheDocument();
  });

  it("shows a skeleton instead of the picker while the metrics load", () => {
    render(<ClaimEditor ticker="COAL" metrics={undefined} packLabel={null} onDone={() => {}} />);
    expect(screen.queryByLabelText("Metrik")).toBeNull();
    expect(screen.getByText("Memuat daftar metrik…")).toBeInTheDocument();
  });

  it("builds the preview sentence 'Akan dievaluasi saat …' from the draft, with the unit and between bounds", () => {
    editor();
    expect(screen.getByText(/Lengkapi metrik/)).toBeInTheDocument();
    fill("Metrik", "core.net_income");
    fill("Pembanding", "between");
    fill("Batas bawah (USD)", "0");
    fill("Batas atas (USD)", "5");
    fill("Jendela evaluasi", "quarter");
    fill("Kuartal", "2026q3");
    fill("Jatuh tempo", FAR);
    expect(screen.getByText("Akan dievaluasi saat laporan 2026Q3 tersedia: Laba bersih antara 0 dan 5 USD.")).toBeInTheDocument();

    fill("Jendela evaluasi", "at_due");
    expect(screen.getByText("Akan dievaluasi pada 31 Des 2099: Laba bersih antara 0 dan 5 USD.")).toBeInTheDocument();
  });

  it("offers only latest and at_due for a price metric", () => {
    editor();
    fill("Metrik", "price.close");
    const windows = within(screen.getByLabelText("Jendela evaluasi")).getAllByRole("option").map((o) => o.getAttribute("value"));
    expect(windows).toEqual(["latest", "at_due"]);
  });

  it("validates before sending: no request for an empty draft or a due date in the past", () => {
    const fetchMock = mockApi({});
    editor();
    fill("Jatuh tempo", "2000-01-01");
    fireEvent.click(screen.getByRole("button", { name: "Simpan" }));
    expect(screen.getByText("Tulis pernyataan klaim")).toBeInTheDocument();
    expect(screen.getByText("Pilih metrik yang diuji")).toBeInTheDocument();
    expect(screen.getByText("Jatuh tempo tidak boleh di masa lalu")).toBeInTheDocument();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("Simpan posts the claim with an Idempotency-Key and maps a 422 to its field; the retry reuses the key", async () => {
    const posts: Array<{ body: unknown; key: string | null }> = [];
    let answer = field422("threshold", "ambang harus berupa angka terhingga");
    mockApi({
      "/api/research/COAL/claims": (_u: URL, init: RequestInit) => {
        posts.push({ body: JSON.parse(String(init.body)), key: new Headers(init.headers).get("Idempotency-Key") });
        return answer;
      },
    });
    const onDone = editor();
    fill("Pernyataan", "  Laba bersih tetap positif  ");
    fill("Metrik", "core.net_income");
    fill("Pembanding", ">");
    fill("Ambang (USD)", "1.000,5");
    fill("Jatuh tempo", FAR);
    fireEvent.click(screen.getByRole("button", { name: "Simpan" }));

    const error = await screen.findByText("ambang harus berupa angka terhingga");
    expect(screen.getByLabelText("Ambang (USD)")).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByLabelText("Ambang (USD)").getAttribute("aria-describedby")).toContain(error.id);
    expect(posts[0].body).toEqual({ statement: "Laba bersih tetap positif", metric: "core.net_income", comparator: ">", threshold: 1000.5, threshold_hi: null, window: "latest", due: FAR });
    expect(posts[0].key).toBeTruthy();
    expect(onDone).not.toHaveBeenCalled();

    answer = json({ id: "c-new" }, 201);
    fireEvent.click(screen.getByRole("button", { name: "Simpan" }));
    await waitFor(() => expect(onDone).toHaveBeenCalledWith(true));
    expect(posts[1].key).toBe(posts[0].key);
  });

  it("a network failure says the claim was not saved", async () => {
    mockApi({ "/api/research/COAL/claims": () => Promise.reject(new TypeError("offline")) });
    editor();
    fill("Pernyataan", "Produksi naik");
    fill("Metrik", "coal_mining.production_mt");
    fill("Ambang (Mt)", "5");
    fill("Jatuh tempo", FAR);
    fireEvent.click(screen.getByRole("button", { name: "Simpan" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Tidak ada koneksi, klaim belum tersimpan");
  });

  it("Batal closes without saving", () => {
    const onDone = editor();
    fireEvent.click(screen.getByRole("button", { name: "Batal" }));
    expect(onDone).toHaveBeenCalledWith(false);
  });

  it("Esc closes the editor without saving and does not reach the pane around it", () => {
    const onDone = editor();
    const outer = vi.fn();
    document.addEventListener("keydown", outer);
    fireEvent.keyDown(screen.getByLabelText(/^Pernyataan/), { key: "Escape" });
    document.removeEventListener("keydown", outer);
    expect(onDone).toHaveBeenCalledWith(false);
    expect(outer.mock.calls[0][0].defaultPrevented).toBe(true);
  });

  it("the preview for latest and every_quarter follows the backend sentence", () => {
    editor();
    fill("Metrik", "coal_mining.production_mt");
    fill("Pembanding", ">=");
    fill("Ambang (Mt)", "5");
    fill("Jatuh tempo", FAR);
    expect(screen.getByText("Akan dievaluasi setiap ada data baru: Produksi terbaru >= 5 Mt.")).toBeInTheDocument();
    fill("Jendela evaluasi", "every_quarter");
    expect(screen.getByText(/^Akan dievaluasi saat tiap laporan kuartal yang terbit sejak .+ sampai 31 Des 2099: Produksi >= 5 Mt\.$/)).toBeInTheDocument();
    // The sentence describes Simpan; it is not a live region that speaks on every keystroke.
    const preview = screen.getByText(/^Akan dievaluasi saat tiap/);
    expect(preview).not.toHaveAttribute("aria-live");
    expect(screen.getByRole("button", { name: "Simpan" }).getAttribute("aria-describedby")).toBe(preview.id);
  });

  it("an edited draft gets a new Idempotency-Key after a failed save; the same draft keeps its key", async () => {
    const keys: string[] = [];
    mockApi({
      "/api/research/COAL/claims": (_u: URL, init: RequestInit) => {
        keys.push(new Headers(init.headers).get("Idempotency-Key") ?? "");
        return Promise.reject(new TypeError("offline"));
      },
    });
    editor();
    fill("Pernyataan", "Produksi naik");
    fill("Metrik", "coal_mining.production_mt");
    fill("Ambang (Mt)", "5");
    fill("Jatuh tempo", FAR);
    const save = screen.getByRole("button", { name: "Simpan" });
    fireEvent.click(save);
    await screen.findByRole("alert");
    await waitFor(() => expect(save).not.toHaveAttribute("aria-busy"));
    fireEvent.click(save);
    await waitFor(() => expect(keys).toHaveLength(2));
    await waitFor(() => expect(save).not.toHaveAttribute("aria-busy"));
    fill("Ambang (Mt)", "6");
    fireEvent.click(save);
    await waitFor(() => expect(keys).toHaveLength(3));
    expect(keys[1]).toBe(keys[0]);
    expect(keys[2]).not.toBe(keys[0]);
  });

  it("moves focus to the first invalid field so its error is read", () => {
    mockApi({});
    editor();
    fill("Pernyataan", "Produksi naik");
    fireEvent.click(screen.getByRole("button", { name: "Simpan" }));
    expect(screen.getByLabelText("Metrik")).toHaveFocus();
  });

  it("maps 422 errors on statement, due and threshold_hi to their fields and the rest to the form", async () => {
    mockApi({
      "/api/research/COAL/claims": () =>
        json(
          {
            detail: [
              { type: "value_error", loc: ["body", "statement"], msg: "pernyataan terlalu panjang", input: null },
              { type: "value_error", loc: ["body", "due"], msg: "jatuh tempo di luar batas", input: null },
              { type: "value_error", loc: ["body", "threshold_hi"], msg: "batas atas tidak sah", input: null },
              { type: "value_error", loc: ["body"], msg: "klaim ganda", input: null },
            ],
          },
          422,
        ),
    });
    editor();
    fill("Pernyataan", "Laba antara nol dan lima");
    fill("Metrik", "core.net_income");
    fill("Pembanding", "between");
    fill("Batas bawah (USD)", "0");
    fill("Batas atas (USD)", "5");
    fill("Jatuh tempo", FAR);
    fireEvent.click(screen.getByRole("button", { name: "Simpan" }));
    expect(await screen.findByText("pernyataan terlalu panjang")).toBeInTheDocument();
    expect(screen.getByLabelText(/^Pernyataan/)).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByLabelText("Jatuh tempo")).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByLabelText("Batas atas (USD)")).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("alert")).toHaveTextContent("klaim ganda");
  });

  it("a failed metric list shows its error with Coba lagi instead of a skeleton", () => {
    const retry = vi.fn();
    render(<ClaimEditor ticker="COAL" metrics={undefined} metricsError={{ message: "Gagal memuat klaim", retry }} packLabel={null} onDone={() => {}} />);
    expect(screen.queryByText("Memuat daftar metrik…")).toBeNull();
    expect(screen.getByRole("alert")).toHaveTextContent("Gagal memuat klaim");
    fireEvent.click(screen.getByRole("button", { name: "Coba lagi" }));
    expect(retry).toHaveBeenCalled();
  });
});

describe("claimPreview against the backend", () => {
  it("matches each golden claim's preview, with ccy written as the currency and dates for reading", () => {
    const claims = golden<ClaimsResponse>("claims-COAL").items;
    expect(claims.length).toBeGreaterThan(0);
    for (const c of claims) {
      if (!c.preview) continue;
      const ours = claimPreview({ metricLabel: c.metric_label, unit: c.unit, comparator: c.comparator, threshold: c.threshold, thresholdHi: c.threshold_hi, window: c.window, due: c.due, since: c.created_at.slice(0, 10), currency: "IDR" });
      const theirs = c.preview.replace(/ ccy\b/g, " Rp").replace(/\d{4}-\d{2}-\d{2}/g, (d) => formatDate(d));
      expect(ours).toBe(theirs);
    }
  });
});

describe("editor helpers", () => {
  it("parseNumber reads Indonesian and plain decimals and rejects text", () => {
    expect(parseNumber("1.234,5")).toBe(1234.5);
    expect(parseNumber("1234.5")).toBe(1234.5);
    expect(parseNumber("1.300")).toBe(1300);
    expect(parseNumber("1.5")).toBe(1.5);
    expect(parseNumber("−3")).toBe(-3);
    expect(parseNumber("")).toBeNull();
    expect(parseNumber("abc")).toBeNull();
  });

  it("todayWib is the WIB calendar day", () => {
    expect(todayWib(new Date("2026-10-24T18:30:00Z"))).toBe("2026-10-25");
    expect(todayWib(new Date("2026-10-24T16:59:00Z"))).toBe("2026-10-24");
  });
});
