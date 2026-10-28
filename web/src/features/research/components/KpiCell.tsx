import { useEffect, useId, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { Button } from "../../../components/Button";
import { cn } from "../../../lib/cn";
import { ApiError } from "../../../lib/api";
import { putKpi } from "../api";
import { formatDate, periodLabel } from "../format";
import type { KpiPackCell, KpiWriteBody } from "../types";
import { parseNumber, todayWib } from "./ClaimEditor";
import { useDraftKey } from "./draftKey";
import { useFloatingPosition } from "./floating";
import { formatMetric } from "./metricFormat";
import { ProvenanceBadge } from "./ProvenanceBadge";

export const UNKNOWN_TEXT = "Tidak diketahui";
export const SAVED_TEXT = "Tersimpan";

/** Lebar editor (px); `w-[min(300px,…)]` di formnya menjaganya di dalam layar sempit. */
const PANEL_WIDTH = 300;

type Field = "value" | "document" | "document_date";

export interface KpiCellProps {
  ticker: string;
  /** Id metrik paket (tanpa awalan paket), kunci `PUT /{ticker}/kpi/{metric}/{fy}/{q}`. */
  metric: string;
  /** Nama metrik untuk label aksesibel, mis. `Produksi`. */
  label: string;
  /** Satuan tetap metrik (`Mt`, `USD/t`); tidak bisa diubah di editor. */
  unit: string;
  cell: KpiPackCell;
  /** Rata kanan di tabel (bawaan) atau kiri di kartu. */
  align?: "end" | "start";
  className?: string;
}

/**
 * Sel KPI (design.md §3.6, §6, §8): nilai dengan badge `manual` dan tanggal dokumen, **Tidak diketahui** sebagai teks,
 * atau **Isi** bila kosong. Klik membuka editor inline: nilai dengan satuan tetap, dokumen (URL atau teks), tanggal,
 * atau **Tidak diketahui**. Enter menyimpan lewat `PUT /{ticker}/kpi/{metric}/{fy}/{q}`, Esc membatalkan; setelah
 * tersimpan wilayah `aria-live="polite"` membaca "Tersimpan" dan fokus kembali ke sel. Galat tampil di bawah sel.
 */
export function KpiCell({ ticker, metric, label, unit, cell: given, align = "end", className }: KpiCellProps) {
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState("");
  const [unknown, setUnknown] = useState(false);
  const [doc, setDoc] = useState("");
  const [date, setDate] = useState("");
  const [errors, setErrors] = useState<Partial<Record<Field, string>>>({});
  const [formError, setFormError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [announce, setAnnounce] = useState("");
  const restore = useRef(false);
  const key = useDraftKey();
  // Nilai yang baru tersimpan tampil sampai paket dimuat ulang, agar sel tidak sempat menunjukkan nilai lama.
  const [saved, setSaved] = useState<{ from: KpiPackCell; cell: KpiPackCell } | null>(null);
  const cell = saved && saved.from === given ? saved.cell : given;
  const anchor = useRef<HTMLSpanElement>(null);
  const floating = useFloatingPosition(anchor, editing, align, PANEL_WIDTH);
  const form = useRef<HTMLFormElement>(null);

  // Galat isian: fokus ke isian pertama yang salah dan bisa difokus, agar galatnya terbaca (WCAG 3.3.1).
  useEffect(() => {
    if (Object.keys(errors).length) form.current?.querySelector<HTMLElement>('[aria-invalid="true"]:not(:disabled)')?.focus();
  }, [errors]);
  const period = periodLabel(cell.label);

  useEffect(() => {
    if (!editing && restore.current) {
      restore.current = false;
      trigger.current?.focus();
    }
  }, [editing]);

  if (cell.state === "not_applicable") {
    return (
      <span title="Metrik tahunan: diisi di kuartal 4" className={cn("inline-flex min-h-6 items-center text-text-3", className)}>
        <span aria-hidden="true" className="text-caption">Diisi di Q4</span>
        <span className="sr-only">{`${label} ${period}: tidak berlaku, metrik tahunan diisi di kuartal 4`}</span>
      </span>
    );
  }

  const open = () => {
    setValue(cell.state === "entered" && typeof cell.value === "number" ? String(cell.value).replace(".", ",") : "");
    setUnknown(cell.state === "unknown");
    setDoc(cell.provenance?.document ?? "");
    setDate(cell.provenance?.document_date ?? todayWib());
    setErrors({});
    setFormError(null);
    setAnnounce("");
    key.reset();
    setEditing(true);
  };
  const close = () => {
    restore.current = true;
    setEditing(false);
  };

  const save = async (e: FormEvent) => {
    e.preventDefault();
    if (saving) return;
    const n = parseNumber(value);
    const found: Partial<Record<Field, string>> = {};
    if (!unknown && n === null) found.value = "Isi nilai berupa angka, atau pilih Tidak diketahui";
    if (!doc.trim()) found.document = "Tulis URL atau nama dokumen sumber";
    if (!date) found.document_date = "Pilih tanggal dokumen";
    else if (date > todayWib()) found.document_date = "Tanggal dokumen tidak boleh di masa depan";
    setErrors(found);
    setFormError(null);
    if (Object.keys(found).length) return;
    const body: KpiWriteBody = unknown ? { unknown: true, document: doc.trim(), document_date: date } : { value: n!, document: doc.trim(), document_date: date };
    setSaving(true);
    try {
      await putKpi(ticker, metric, cell.fy, cell.q, body, { idempotencyKey: key.forBody(body) });
      key.reset();
      setSaved({
        from: given,
        cell: {
          ...given,
          state: unknown ? "unknown" : "entered",
          value: unknown ? null : n,
          unknown,
          action: undefined,
          provenance: { kind: "manual", document: body.document, document_date: body.document_date, entered_at: new Date().toISOString() },
        },
      });
      setAnnounce(SAVED_TEXT);
      close();
    } catch (err) {
      if (!(err instanceof ApiError)) setFormError("KPI belum tersimpan");
      else if (err.kind === "network" || err.kind === "timeout") setFormError("Tidak ada koneksi, KPI belum tersimpan");
      else {
        const mapped: Partial<Record<Field, string>> = {};
        for (const f of ["value", "document", "document_date"] as const) if (err.fields[f]) mapped[f] = err.fields[f];
        if (err.fields.unknown) mapped.value = err.fields.unknown;
        setErrors(mapped);
        if (!Object.keys(mapped).length) setFormError(err.message);
      }
    } finally {
      setSaving(false);
    }
  };

  const onKeyDown = (e: KeyboardEvent<HTMLFormElement>) => {
    if (e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    close();
  };

  const prov = cell.provenance;
  const provTitle = prov ? `Diisi pemilik · dokumen ${prov.document} · ${formatDate(prov.document_date)}` : undefined;
  let face;
  if (cell.state === "entered" && typeof cell.value === "number") {
    const text = formatMetric(cell.value, unit);
    face = (
      <button ref={trigger} type="button" onClick={open} aria-expanded={editing} aria-label={`${label} ${period}: ${text}, ubah KPI`} className="num min-h-11 min-w-11 max-w-full rounded-sm px-1 text-right [overflow-wrap:anywhere] decoration-dotted underline-offset-2 hover:underline focus-visible:underline">
        {text}
      </button>
    );
  } else if (cell.state === "unknown") {
    face = (
      <button ref={trigger} type="button" onClick={open} aria-expanded={editing} aria-label={`${label} ${period}: ${UNKNOWN_TEXT}, ubah KPI`} className="min-h-11 rounded-sm px-1 text-label text-text-2 decoration-dotted underline-offset-2 hover:underline">
        {UNKNOWN_TEXT}
      </button>
    );
  } else {
    face = (
      <button ref={trigger} type="button" onClick={open} aria-expanded={editing} aria-label={`Isi ${label} ${period}`} className="inline-flex min-h-11 min-w-11 items-center justify-center">
        <span className="inline-flex h-8 items-center rounded-sm bg-surface-sunken px-3 text-caption font-bold text-accent">Isi</span>
      </button>
    );
  }

  const describedBy = (f: Field) => (errors[f] ? `${id}-${f}-error` : undefined);
  return (
    <span ref={anchor} className={cn("relative inline-flex min-w-0 max-w-full flex-col", align === "end" ? "items-end" : "items-start", className)}>
      <span className="inline-flex max-w-full flex-wrap items-center gap-1">
        {face}
        {prov && cell.state !== "not_entered" ? <ProvenanceBadge kind="manual" text={`manual · ${formatDate(prov.document_date)}`} title={provTitle} /> : null}
      </span>
      <span role="status" aria-live="polite" className="sr-only">
        {announce}
      </span>
      {editing ? (
        <form
          ref={form}
          aria-label={`KPI ${label} ${period}`}
          noValidate
          onSubmit={save}
          onKeyDown={onKeyDown}
          style={floating}
          className="fixed z-30 flex w-[min(300px,calc(100vw-8px))] flex-col gap-2 overflow-y-auto overscroll-contain rounded-md border border-border-hairline bg-bg p-3 text-left shadow-popover"
        >
          <div className="flex flex-col gap-1">
            <label htmlFor={`${id}-value`} className="text-label font-semibold text-text">{`Nilai (${unit})`}</label>
            <input
              id={`${id}-value`}
              autoFocus={!unknown}
              inputMode="decimal"
              disabled={unknown}
              value={unknown ? "" : value}
              onChange={(e) => setValue(e.target.value)}
              aria-invalid={errors.value ? true : undefined}
              aria-describedby={describedBy("value")}
              className={cn("num h-11 rounded-sm border bg-surface-sunken px-3 text-right text-body text-text disabled:opacity-60", errors.value ? "border-status-over" : "border-transparent")}
            />
            {errors.value && !unknown ? (
              <p id={`${id}-value-error`} className="text-caption text-status-over">
                {errors.value}
              </p>
            ) : null}
          </div>
          {unknown && errors.value ? (
            <p role="alert" className="text-caption text-status-over">
              {errors.value}
            </p>
          ) : null}
          <label className="flex min-h-11 items-center gap-2 text-label text-text">
            <input type="checkbox" autoFocus={unknown} checked={unknown} onChange={(e) => setUnknown(e.target.checked)} className="h-4 w-4 accent-[var(--accent)]" />
            {UNKNOWN_TEXT}
          </label>
          <div className="flex flex-col gap-1">
            <label htmlFor={`${id}-doc`} className="text-label font-semibold text-text">
              Dokumen (URL atau teks)
            </label>
            <input
              id={`${id}-doc`}
              value={doc}
              onChange={(e) => setDoc(e.target.value)}
              aria-invalid={errors.document ? true : undefined}
              aria-describedby={describedBy("document")}
              className={cn("h-11 rounded-sm border bg-surface-sunken px-3 text-body text-text", errors.document ? "border-status-over" : "border-transparent")}
            />
            {errors.document ? (
              <p id={`${id}-document-error`} className="text-caption text-status-over">
                {errors.document}
              </p>
            ) : null}
          </div>
          <div className="flex flex-col gap-1">
            <label htmlFor={`${id}-date`} className="text-label font-semibold text-text">
              Tanggal dokumen
            </label>
            <input
              id={`${id}-date`}
              type="date"
              max={todayWib()}
              value={date}
              onChange={(e) => setDate(e.target.value)}
              aria-invalid={errors.document_date ? true : undefined}
              aria-describedby={describedBy("document_date")}
              className={cn("h-11 rounded-sm border bg-surface-sunken px-3 text-body text-text", errors.document_date ? "border-status-over" : "border-transparent")}
            />
            {errors.document_date ? (
              <p id={`${id}-document_date-error`} className="text-caption text-status-over">
                {errors.document_date}
              </p>
            ) : null}
          </div>
          {formError ? (
            <p role="alert" className="text-caption text-status-over">
              {formError}
            </p>
          ) : null}
          <div className="-ml-3 flex gap-1">
            <Button variant="ghost" type="submit" loading={saving}>
              Simpan
            </Button>
            <Button variant="ghost" onClick={close}>
              Batal
            </Button>
          </div>
        </form>
      ) : null}
    </span>
  );
}
