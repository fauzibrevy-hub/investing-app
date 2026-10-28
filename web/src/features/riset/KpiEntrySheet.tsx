import { useId, useState, type FormEvent } from "react";
import { Button } from "../../components/Button";
import { FormField } from "../../components/FormField";
import { Sheet } from "../../components/Sheet";
import { putKpi, useDocuments } from "../research/api";
import { useDraftKey } from "../research/components/draftKey";
import { parseNumber, todayWib } from "../research/components/ClaimEditor";
import { formErrors } from "../research/components/fieldErrors";
import { DocumentUpload } from "../research/extract/DocumentUpload";
import { ProposalReview } from "../research/extract/ProposalReview";
import { periodLabel } from "../research/format";
import type { KpiPackCell, KpiWriteBody, PackMetric } from "../research/types";

type Field = "value" | "document" | "document_date";
const SHOWN: readonly Field[] = ["value", "document", "document_date"];

export interface KpiEntrySheetProps {
  ticker: string;
  metric: PackMetric;
  cell: KpiPackCell;
  /** Mata uang pelaporan untuk satuan `ccy`. */
  currency: string | null;
  onClose: () => void;
}

/**
 * **Isi** KPI satu kuartal (ui-slicing.md §1 blok 3): angka dengan satuan tetap, atau **Tidak diketahui**, dengan dokumen
 * sumber dan tanggalnya (`PUT /{ticker}/kpi/{metric}/{fy}/{q}`); di bawahnya unggahan dokumen yang menghasilkan usulan
 * KPI untuk diterima atau ditolak. Tidak ada angka yang tersimpan tanpa tindakan pemilik.
 */
export function KpiEntrySheet({ ticker, metric, cell, currency, onClose }: KpiEntrySheetProps) {
  const formId = useId();
  const period = periodLabel(cell.label);
  const unitWord = metric.unit === "ccy" ? (currency ?? "mata uang laporan") : metric.unit;
  const [value, setValue] = useState(cell.state === "entered" && typeof cell.value === "number" ? String(cell.value).replace(".", ",") : "");
  const [unknown, setUnknown] = useState(cell.state === "unknown");
  const [doc, setDoc] = useState(cell.provenance?.document ?? "");
  const [date, setDate] = useState(cell.provenance?.document_date ?? todayWib());
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);
  const [uploadedId, setUploadedId] = useState<string | null>(null);
  const key = useDraftKey();
  const documents = useDocuments(ticker);

  const save = async (e: FormEvent) => {
    e.preventDefault();
    if (saving) return;
    const n = parseNumber(value);
    const found: Record<string, string> = {};
    if (!unknown && n === null) found.value = "Isi nilai berupa angka, atau pilih Tidak diketahui";
    if (!doc.trim()) found.document = "Tulis URL atau nama dokumen sumber";
    if (!date) found.document_date = "Pilih tanggal dokumen";
    else if (date > todayWib()) found.document_date = "Tanggal dokumen tidak boleh di masa depan";
    setErrors(found);
    if (Object.keys(found).length) return;
    const body: KpiWriteBody = unknown ? { unknown: true, document: doc.trim(), document_date: date } : { value: n!, document: doc.trim(), document_date: date };
    setSaving(true);
    try {
      await putKpi(ticker, metric.id, cell.fy, cell.q, body, { idempotencyKey: key.forBody(body) });
      key.reset();
      onClose();
    } catch (err) {
      setErrors(formErrors(err, SHOWN, { unknown: "value" }, "KPI belum tersimpan"));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Sheet
      open
      title={`${metric.label} · ${period}`}
      onClose={onClose}
      footer={
        <Button type="submit" form={formId} fullWidth loading={saving}>
          Simpan
        </Button>
      }
    >
      <div className="flex flex-col gap-4">
        <form id={formId} noValidate onSubmit={save} aria-label={`KPI ${metric.label} ${period}`} className="flex flex-col gap-3">
          <FormField
            label={`Nilai (${unitWord})`}
            inputMode="decimal"
            disabled={unknown}
            value={unknown ? "" : value}
            onChange={(e) => setValue(e.target.value)}
            error={errors.value}
            autoFocus={!unknown}
          />
          <label className="flex min-h-11 items-center gap-2 text-label text-text">
            <input type="checkbox" checked={unknown} onChange={(e) => setUnknown(e.target.checked)} className="h-4 w-4 accent-[var(--accent)]" />
            Tidak diketahui
          </label>
          <FormField label="Dokumen sumber (URL atau teks)" value={doc} onChange={(e) => setDoc(e.target.value)} error={errors.document} />
          <FormField label="Tanggal dokumen" type="date" max={todayWib()} value={date} onChange={(e) => setDate(e.target.value)} error={errors.document_date} />
          {errors._ ? (
            <p role="alert" className="text-caption text-status-over">
              {errors._}
            </p>
          ) : null}
        </form>
        <section aria-label="Unggah dokumen" className="flex flex-col gap-2 border-t border-border-hairline pt-3">
          <h3 className="text-body-strong text-text">Atau unggah dokumen</h3>
          <p className="text-caption text-text-2">Laporan operasional atau presentasi (PDF atau PPTX). Nilai KPI yang ditemukan muncul sebagai usulan; tidak ada yang tersimpan sebelum Anda menerimanya.</p>
          <DocumentUpload ticker={ticker} documents={documents.data} defaultPeriod={{ fy: cell.fy, q: cell.q }} onUploaded={(r) => setUploadedId(r.document.id)} />
          <ProposalReview ticker={ticker} focusDocumentId={uploadedId} />
        </section>
      </div>
    </Sheet>
  );
}
