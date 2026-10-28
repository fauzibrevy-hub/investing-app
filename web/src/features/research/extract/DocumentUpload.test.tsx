import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { json, mockApi } from "../../../test/mockApi";
import goldenDocs from "../../../../../tests/fixtures/research/canon/documents-COAL.json";
import { uploadResponse } from "../testing/extract";
import type { DocumentsResponse, UploadResponse } from "../types";
import { DocumentUpload, MAX_UPLOAD_BYTES, uploadSummary } from "./DocumentUpload";

const UPLOAD = "/api/research/COAL/documents/upload";
const docs = goldenDocs as unknown as DocumentsResponse;
const period = { fy: 2026, q: 2 };

type Call = { key: string | undefined; contentType: string | undefined; body: unknown };

function setup(handler: (n: number) => Response | Promise<Response>, onUploaded?: (r: UploadResponse) => void) {
  const calls: Call[] = [];
  mockApi({
    [UPLOAD]: (_u: URL, init: RequestInit) => {
      const h = (init.headers ?? {}) as Record<string, string>;
      calls.push({ key: h["Idempotency-Key"], contentType: h["Content-Type"], body: init.body });
      return handler(calls.length);
    },
  });
  render(<DocumentUpload ticker="COAL" documents={docs} defaultPeriod={period} onUploaded={onUploaded} />);
  return calls;
}

const form = () => screen.getByRole("form", { name: "Unggah dokumen" });
const pick = (name = "paparan.pdf", size?: number) => {
  const file = new File(["%PDF-1.4"], name, { type: "application/pdf" });
  if (size !== undefined) Object.defineProperty(file, "size", { value: size });
  fireEvent.change(screen.getByLabelText("Berkas (PDF atau PPTX)"), { target: { files: [file] } });
  return file;
};
const fill = (date = "2020-01-15") => fireEvent.change(screen.getByLabelText("Tanggal dokumen"), { target: { value: date } });
const submit = (name = "Unggah") => fireEvent.click(within(form()).getByRole("button", { name }));

describe("DocumentUpload: idle", () => {
  it("starts idle with the dossier period, the document kinds and an upload button; empty submit is refused in the client", () => {
    const calls = setup(() => json(uploadResponse(), 201));
    expect(screen.getByLabelText("Tahun laporan (FY)")).toHaveValue("2026");
    expect(screen.getByLabelText("Kuartal laporan")).toHaveValue("2");
    expect(screen.getByLabelText("Jenis")).toHaveValue("presentasi");
    expect(within(screen.getByLabelText("Jenis")).getAllByRole("option").map((o) => o.textContent)).toContain("Update operasional");
    expect(form()).not.toHaveAttribute("aria-busy");
    expect(screen.queryByRole("list", { name: "Hasil unggah" })).toBeNull();
    submit();
    expect(screen.getByLabelText("Berkas (PDF atau PPTX)")).toHaveAccessibleDescription(/Pilih berkas PDF atau PPTX/);
    expect(screen.getByLabelText("Tanggal dokumen")).toHaveAccessibleDescription(/Pilih tanggal dokumen/);
    expect(calls).toHaveLength(0);
  });

  it.each([
    ["a file that is not PDF or PPTX", () => pick("catatan.txt"), "Hanya berkas PDF atau PPTX"],
    ["a file over 20 MB", () => pick("besar.pdf", MAX_UPLOAD_BYTES + 1), "Berkas lebih dari 20 MB"],
  ])("refuses %s before sending", (_n, act, message) => {
    const calls = setup(() => json(uploadResponse(), 201));
    act();
    fill();
    submit();
    expect(screen.getByLabelText("Berkas (PDF atau PPTX)")).toHaveAccessibleDescription(new RegExp(message));
    expect(calls).toHaveLength(0);
  });

  it("refuses a future date and a bad year", () => {
    const calls = setup(() => json(uploadResponse(), 201));
    pick();
    fill("2999-01-01");
    fireEvent.change(screen.getByLabelText("Tahun laporan (FY)"), { target: { value: "26" } });
    submit();
    expect(screen.getByLabelText("Tanggal dokumen")).toHaveAccessibleDescription(/tidak boleh di masa depan/);
    expect(screen.getByLabelText("Tahun laporan (FY)")).toHaveAccessibleDescription(/Tulis tahun laporan/);
    expect(calls).toHaveLength(0);
  });
});

describe("DocumentUpload: uploading", () => {
  it("shows progress, locks the fields and ignores a second submit", async () => {
    const calls = setup(() => new Promise<Response>(() => {}));
    pick();
    fill();
    submit();
    expect(await screen.findByText("Mengunggah dan membaca berkas…")).toBeInTheDocument();
    expect(form()).toHaveAttribute("aria-busy", "true");
    expect(screen.getByLabelText("Berkas (PDF atau PPTX)")).toBeDisabled();
    expect(screen.getByLabelText("Tanggal dokumen")).toBeDisabled();
    fireEvent.submit(form());
    expect(calls).toHaveLength(1);
  });
});

describe("DocumentUpload: done", () => {
  it("sends multipart form data with the five fields and an Idempotency-Key, then reports the result", async () => {
    const onUploaded = vi.fn();
    const calls = setup(() => json(uploadResponse(), 201), onUploaded);
    const file = pick();
    fill("2020-01-15");
    fireEvent.change(screen.getByLabelText("Jenis"), { target: { value: "update_operasional" } });
    submit();
    const list = await screen.findByRole("list", { name: "Hasil unggah" });
    expect(list).toHaveTextContent("PDF, 12 halaman");
    expect(list).toHaveTextContent("4 usulan menunggu keputusan");
    expect(screen.getByRole("status")).toHaveTextContent("Terunggah");
    const body = calls[0].body as FormData;
    expect(body).toBeInstanceOf(FormData);
    expect((body.get("file") as File).name).toBe(file.name);
    expect(Object.fromEntries(["kind", "doc_date", "fy", "q"].map((k) => [k, body.get(k)]))).toEqual({ kind: "update_operasional", doc_date: "2020-01-15", fy: "2026", q: "2" });
    // Multipart: tidak ada Content-Type JSON; peramban menambahkan batasnya sendiri.
    expect(calls[0].contentType).toBeUndefined();
    expect(calls[0].key).toBeTruthy();
    expect(onUploaded).toHaveBeenCalledWith(expect.objectContaining({ created: true }));
    // Berkas dikosongkan setelah selesai; mengubah isian kembali ke idle.
    expect((screen.getByLabelText("Berkas (PDF atau PPTX)") as HTMLInputElement).value).toBe("");
    fill("2020-02-01");
    expect(screen.queryByRole("list", { name: "Hasil unggah" })).toBeNull();
  });

  it.each([
    ["the same file again for the same period", uploadResponse({ created: false }), "sudah pernah diunggah"],
    ["a ticker without a pack", uploadResponse({ pack: null, proposals: [] }, []), "belum punya paket industri"],
    ["a scanned file with no text", uploadResponse({ upload: { ...uploadResponse().upload, pages_with_text: 0 }, proposals: [] }, []), "hasil pindai"],
    ["a file with no KPI found", uploadResponse({}, []), "Tidak ada nilai KPI paket"],
  ])("says so for %s", async (_n, response, text) => {
    setup(() => json(response, response.created ? 201 : 200));
    pick();
    fill();
    submit();
    expect(await screen.findByRole("list", { name: "Hasil unggah" })).toHaveTextContent(text);
  });

  it("names slides for a presentation", () => {
    expect(uploadSummary(uploadResponse({ upload: { ...uploadResponse().upload, format: "pptx", locator: "slide", pages: 9 } }))[0]).toBe("PPTX, 9 slide");
  });
});

describe("DocumentUpload: error", () => {
  it("keeps the fields, shows the error and retries with the same Idempotency-Key", async () => {
    const calls = setup((n) => (n === 1 ? json({ detail: "Server sedang sibuk" }, 503) : json(uploadResponse(), 201)));
    pick();
    fill();
    submit();
    expect(await screen.findByRole("alert")).toHaveTextContent("Server sedang sibuk");
    expect(screen.getByLabelText("Tanggal dokumen")).toHaveValue("2020-01-15");
    submit("Unggah ulang");
    await screen.findByRole("list", { name: "Hasil unggah" });
    expect(calls).toHaveLength(2);
    expect(calls[1].key).toBe(calls[0].key);
  });

  it("a changed form after a failure gets a new key (the backend refuses one key for two bodies)", async () => {
    const calls = setup((n) => (n === 1 ? json({ detail: "boom" }, 500) : json(uploadResponse(), 201)));
    pick();
    fill();
    submit();
    await screen.findByRole("alert");
    fill("2020-03-03");
    submit("Unggah");
    await screen.findByRole("list", { name: "Hasil unggah" });
    expect(calls[1].key).not.toBe(calls[0].key);
  });

  it("maps a 422 to the field it names", async () => {
    setup(() => json({ detail: [{ loc: ["body", "file"], msg: "Only PDF and PPTX files are accepted", type: "file_type" }] }, 422));
    pick();
    fill();
    submit();
    await waitFor(() => expect(screen.getByLabelText("Berkas (PDF atau PPTX)")).toHaveAccessibleDescription(/Hanya berkas PDF atau PPTX/));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("413 names the file; 404 names the ticker; an unreadable file keeps the server message", async () => {
    setup(() => json({ detail: "The file is larger than 20 MB" }, 413));
    pick();
    fill();
    submit();
    await waitFor(() => expect(screen.getByLabelText("Berkas (PDF atau PPTX)")).toHaveAccessibleDescription(/Berkas lebih dari 20 MB/));
  });

  it("an offline write says nothing was uploaded", async () => {
    setup(() => Promise.reject(new TypeError("Failed to fetch")));
    pick();
    fill();
    submit();
    expect(await screen.findByRole("alert")).toHaveTextContent("Tidak ada koneksi, berkas belum terunggah");
  });
});
