import { useState, type FormEvent } from "react";
import { Button } from "../../../components/Button";
import { FormField } from "../../../components/FormField";
import { ApiError } from "../../../lib/api";
import { cn } from "../../../lib/cn";
import { uploadDocument } from "../api";
import { todayWib } from "../components/ClaimEditor";
import { useDraftKey } from "../components/draftKey";
import { formErrors } from "../components/fieldErrors";
import type { DocumentKind, DocumentsResponse, QuarterRef, UploadResponse } from "../types";

/** Batas backend: 20 MB (413 di atasnya). */
export const MAX_UPLOAD_BYTES = 20 * 1024 * 1024;
const EXTENSION = /\.(pdf|pptx)$/i;
const FIELDS = ["file", "kind", "doc_date", "fy", "q"] as const;

/** `idle` → `uploading` → `done` | `error`; mengubah isian setelah galat atau selesai kembali ke `idle`. */
export type UploadStatus = "idle" | "uploading" | "error" | "done";

const selectClass = (invalid: boolean) =>
  cn("h-12 w-full rounded-sm border bg-surface-sunken px-3 text-body text-text focus-visible:border-accent", invalid ? "border-status-over" : "border-transparent");

/** Ringkasan hasil unggah dalam kalimat; tidak pernah menyebut nilai, hanya hitungan dan keadaan berkas. */
export function uploadSummary(r: UploadResponse): string[] {
  const unit = r.upload.locator === "slide" ? "slide" : "halaman";
  const lines = [`${r.upload.format.toUpperCase()}, ${r.upload.pages} ${unit}`];
  if (!r.created) lines.push("Berkas ini sudah pernah diunggah untuk periode yang sama; tidak ada dokumen kedua");
  if (!r.pack) lines.push("Emiten ini belum punya paket industri, jadi tidak ada usulan KPI");
  else if (r.upload.pages_with_text === 0) lines.push("Tidak ada teks yang terbaca di berkas ini (mungkin hasil pindai), jadi tidak ada usulan");
  else if (r.proposals.length === 0) lines.push("Tidak ada nilai KPI paket yang ditemukan di berkas ini");
  else lines.push(`${r.proposals.length} usulan menunggu keputusan`);
  return lines;
}

export interface DocumentUploadProps {
  ticker: string;
  documents: DocumentsResponse | undefined;
  /** Periode laporan bawaan, mis. kuartal terakhir cakupan dossier. */
  defaultPeriod?: QuarterRef;
  onUploaded?: (result: UploadResponse) => void;
}

/**
 * **Unggah dokumen** (design.md §4.2 Sumber, plan.md R8): PDF atau PPTX, jenis, tanggal dokumen, dan periode laporan (FY dan
 * kuartal) lewat `POST /{ticker}/documents/upload`. Berkas dibaca oleh aturan di backend dan menghasilkan usulan KPI; tidak ada
 * yang tersimpan sebagai KPI sebelum diterima. Percobaan ulang isian yang sama memakai `Idempotency-Key` yang sama.
 */
export function DocumentUpload({ ticker, documents, defaultPeriod, onUploaded }: DocumentUploadProps) {
  const kinds = documents?.kinds ?? [{ id: "presentasi" as DocumentKind, label: "Presentasi" }];
  const [status, setStatus] = useState<UploadStatus>("idle");
  const [file, setFile] = useState<File | null>(null);
  const [fileKey, setFileKey] = useState(0);
  const [kind, setKind] = useState<DocumentKind>("presentasi");
  const [docDate, setDocDate] = useState("");
  const [fy, setFy] = useState(defaultPeriod ? String(defaultPeriod.fy) : "");
  const [q, setQ] = useState(defaultPeriod ? String(defaultPeriod.q) : "");
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [result, setResult] = useState<UploadResponse | null>(null);
  const key = useDraftKey();
  const busy = status === "uploading";

  const edit = <T,>(set: (v: T) => void) => (v: T) => {
    set(v);
    if (status === "error" || status === "done") setStatus("idle");
  };

  function validate(): Record<string, string> {
    const found: Record<string, string> = {};
    if (!file) found.file = "Pilih berkas PDF atau PPTX";
    else if (!EXTENSION.test(file.name)) found.file = "Hanya berkas PDF atau PPTX";
    else if (file.size > MAX_UPLOAD_BYTES) found.file = "Berkas lebih dari 20 MB";
    if (!docDate) found.doc_date = "Pilih tanggal dokumen";
    else if (docDate > todayWib()) found.doc_date = "Tanggal dokumen tidak boleh di masa depan";
    const year = Number(fy);
    if (!/^\d{4}$/.test(fy.trim()) || year < 2000 || year > 2100) found.fy = "Tulis tahun laporan seperti 2026";
    if (!["1", "2", "3", "4"].includes(q)) found.q = "Pilih kuartal laporan";
    return found;
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (busy) return;
    const local = validate();
    setErrors(local);
    if (Object.keys(local).length) {
      setStatus("error");
      return;
    }
    const f = file!;
    const form = new FormData();
    form.set("file", f);
    form.set("kind", kind);
    form.set("doc_date", docDate);
    form.set("fy", fy.trim());
    form.set("q", q);
    // Isian yang sama (berkas, jenis, tanggal, periode) setelah gagal memakai kunci yang sama: tidak ada dokumen kedua.
    const idempotencyKey = key.forBody({ ticker, kind, doc_date: docDate, fy: fy.trim(), q, name: f.name, size: f.size, modified: f.lastModified });
    setStatus("uploading");
    try {
      const res = await uploadDocument(ticker, form, { idempotencyKey });
      key.reset();
      setResult(res);
      setErrors({});
      setStatus("done");
      setFile(null);
      setFileKey((n) => n + 1);
      onUploaded?.(res);
    } catch (err) {
      if (err instanceof ApiError && err.status === 413) setErrors({ file: "Berkas lebih dari 20 MB" });
      else if (err instanceof ApiError && err.status === 404) setErrors({ _: "Emiten ini tidak ada di daftar riset" });
      else if (err instanceof ApiError && (err.kind === "network" || err.kind === "timeout")) setErrors({ _: "Tidak ada koneksi, berkas belum terunggah. Coba lagi." });
      else setErrors(formErrors(err, FIELDS, { body: "_" }, "Gagal mengunggah"));
      setStatus("error");
    }
  }

  return (
    <form onSubmit={submit} noValidate aria-label="Unggah dokumen" aria-busy={busy || undefined} className="flex flex-col gap-3">
      <FormField
        key={fileKey}
        label="Berkas (PDF atau PPTX)"
        type="file"
        accept=".pdf,.pptx,application/pdf,application/vnd.openxmlformats-officedocument.presentationml.presentation"
        disabled={busy}
        onChange={(e) => edit(setFile)(e.target.files?.[0] ?? null)}
        error={errors.file}
        hint="Paling besar 20 MB. Dibaca di server ini oleh aturan, tanpa layanan luar."
      />
      <div className="grid gap-3 sm:grid-cols-2">
        <FormField label="Jenis" error={errors.kind}>
          {(a11y) => (
            <select {...a11y} value={kind} disabled={busy} onChange={(e) => edit(setKind)(e.target.value as DocumentKind)} className={selectClass(!!errors.kind)}>
              {kinds.map((k) => (
                <option key={k.id} value={k.id}>
                  {k.label}
                </option>
              ))}
            </select>
          )}
        </FormField>
        <FormField label="Tanggal dokumen" type="date" max={todayWib()} value={docDate} disabled={busy} onChange={(e) => edit(setDocDate)(e.target.value)} error={errors.doc_date} hint="Tanggal terbit dokumen" />
        <FormField label="Tahun laporan (FY)" inputMode="numeric" value={fy} disabled={busy} onChange={(e) => edit(setFy)(e.target.value)} error={errors.fy} hint="Periode yang dilaporkan, bukan tanggal terbit" />
        <FormField label="Kuartal laporan" error={errors.q}>
          {(a11y) => (
            <select {...a11y} value={q} disabled={busy} onChange={(e) => edit(setQ)(e.target.value)} className={selectClass(!!errors.q)}>
              <option value="">Pilih kuartal…</option>
              {[1, 2, 3, 4].map((n) => (
                <option key={n} value={String(n)}>{`Q${n}`}</option>
              ))}
            </select>
          )}
        </FormField>
      </div>
      {errors._ ? (
        <p role="alert" className="text-caption text-status-over">
          {errors._}
        </p>
      ) : null}
      <div className="flex items-center gap-3">
        <Button type="submit" variant="sunken" loading={busy}>
          {status === "error" && !Object.keys(errors).some((k) => k !== "_") ? "Unggah ulang" : "Unggah"}
        </Button>
        <p role="status" aria-live="polite" className="text-caption text-text-2">
          {busy ? "Mengunggah dan membaca berkas…" : status === "done" && result ? "Terunggah" : ""}
        </p>
      </div>
      {status === "done" && result ? (
        <ul aria-label="Hasil unggah" className="flex flex-col gap-1 text-label text-text-2">
          {uploadSummary(result).map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
      ) : null}
    </form>
  );
}
