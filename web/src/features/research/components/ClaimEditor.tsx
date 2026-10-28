import { useEffect, useId, useMemo, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { Button } from "../../../components/Button";
import { FormField } from "../../../components/FormField";
import { Skeleton } from "../../../components/Skeleton";
import { ApiError, MESSAGES } from "../../../lib/api";
import { createClaim } from "../api";
import type { ClaimCreateBody, ClaimMetricOption, Comparator } from "../types";
import { claimPreview, unitWord } from "./ClaimCard";
import { useDraftKey } from "./draftKey";

/** Hari ini sebagai hari WIB (`YYYY-MM-DD`), hari yang dipakai backend untuk jatuh tempo dan tanggal dokumen. */
export function todayWib(now: Date = new Date()): string {
  return new Date(now.getTime() + 7 * 3600 * 1000).toISOString().slice(0, 10);
}

export const STATEMENT_MAX = 200;

const COMPARATORS: Array<{ value: Comparator; label: string }> = [
  { value: "<", label: "kurang dari (<)" },
  { value: "<=", label: "paling banyak (≤)" },
  { value: ">", label: "lebih dari (>)" },
  { value: ">=", label: "paling sedikit (≥)" },
  { value: "between", label: "antara" },
];

type WindowKind = "latest" | "at_due" | "every_quarter" | "quarter";

const WINDOWS: Array<{ value: WindowKind; label: string; quarterlyOnly: boolean }> = [
  { value: "latest", label: "Data terbaru", quarterlyOnly: false },
  { value: "at_due", label: "Pada jatuh tempo", quarterlyOnly: false },
  { value: "every_quarter", label: "Setiap kuartal sampai jatuh tempo", quarterlyOnly: true },
  { value: "quarter", label: "Satu kuartal", quarterlyOnly: true },
];

const QUARTER = /^\d{4}Q[1-4]$/;

/** Field formulir = kunci galat 422 backend. */
type Field = "statement" | "metric" | "comparator" | "threshold" | "threshold_hi" | "window" | "due";
const FIELDS: readonly Field[] = ["statement", "metric", "comparator", "threshold", "threshold_hi", "window", "due"];

/**
 * Teks angka dari kolom isian; kosong atau bukan angka → `null`. Format Indonesia didahulukan, sama dengan angka yang
 * ditampilkan: `1.234,5` dan `1.300` (titik pemisah ribuan) dibaca sebagai 1234,5 dan 1300; `1234.5` dan `1.5`
 * (bukan kelompok tiga digit) sebagai desimal titik.
 */
export function parseNumber(text: string): number | null {
  const t = text.trim().replace(/\s/g, "").replace("−", "-");
  if (!t) return null;
  const grouped = /^-?\d{1,3}(\.\d{3})+$/.test(t);
  const normalized = t.includes(",") || grouped ? t.replace(/\./g, "").replace(",", ".") : t;
  const n = Number(normalized);
  return Number.isFinite(n) ? n : null;
}

/** Kelompok pemilih metrik: inti, paket emiten, harga (design.md §6). */
function groupMetrics(metrics: readonly ClaimMetricOption[], packLabel: string | null) {
  const groups = new Map<string, ClaimMetricOption[]>();
  for (const m of metrics) {
    const name = m.layer === "price" ? "Harga" : m.pack === "core" ? "Inti" : (packLabel ?? m.pack ?? "Paket");
    groups.set(name, [...(groups.get(name) ?? []), m]);
  }
  return [...groups.entries()];
}

const selectClass = "h-12 w-full rounded-sm border border-transparent bg-surface-sunken px-3 text-body text-text";

export interface ClaimEditorProps {
  ticker: string;
  /** `GET /{ticker}/claims` `.metrics`: inti, paket yang berlaku, dan `price.*`. `undefined` saat masih dimuat. */
  metrics: readonly ClaimMetricOption[] | undefined;
  /** Daftar metrik gagal dimuat: galat dengan **Coba lagi** di tempat pemilih, bukan kerangka tanpa akhir. */
  metricsError?: { message: string; retry: () => void } | null;
  packLabel: string | null;
  /** Mata uang pelaporan untuk satuan `ccy`; `null` bila belum diketahui. */
  currency?: string | null;
  /** Dipanggil setelah tersimpan (`true`) atau dibatalkan (`false`). */
  onDone: (saved: boolean) => void;
}

/**
 * Editor klaim inline (design.md §3.4, §6): pernyataan, metrik, pembanding dan ambang dengan satuan, jendela, jatuh
 * tempo, dan kalimat pratinjau "Akan dievaluasi saat …". **Simpan** mengirim `POST /{ticker}/claims` dengan
 * `Idempotency-Key` yang sama untuk percobaan ulang draf yang sama; galat 422 tampil per field.
 */
export function ClaimEditor({ ticker, metrics, metricsError = null, packLabel, currency = null, onDone }: ClaimEditorProps) {
  const formId = useId();
  const [statement, setStatement] = useState("");
  const [metricId, setMetricId] = useState("");
  const [comparator, setComparator] = useState<Comparator>(">=");
  const [threshold, setThreshold] = useState("");
  const [thresholdHi, setThresholdHi] = useState("");
  const [windowKind, setWindowKind] = useState<WindowKind>("latest");
  const [quarter, setQuarter] = useState("");
  const [due, setDue] = useState("");
  const [errors, setErrors] = useState<Partial<Record<Field, string>>>({});
  const [formError, setFormError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  // Kunci mengikuti isi: percobaan ulang setelah timeout tidak membuat klaim kedua, draf yang diubah mendapat kunci baru.
  const key = useDraftKey();
  const form = useRef<HTMLFormElement>(null);

  // Galat isian: fokus ke isian pertama yang salah, agar galatnya terbaca (WCAG 3.3.1).
  useEffect(() => {
    if (Object.keys(errors).length) form.current?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
  }, [errors]);

  const metric = metrics?.find((m) => m.id === metricId) ?? null;
  // Metrik harga hanya menerima `latest` dan `at_due` (plan §5 ASUMSI R4).
  const windows = WINDOWS.filter((w) => !w.quarterlyOnly || !metric || metric.quarterly);
  const effectiveWindow: WindowKind = windows.some((w) => w.value === windowKind) ? windowKind : "latest";
  const quarterText = quarter.trim().toUpperCase();
  const windowValue = effectiveWindow === "quarter" ? `quarter:${quarterText}` : effectiveWindow;
  const unit = metric ? unitWord(metric.unit, currency) : "";
  const today = todayWib();
  const groups = useMemo(() => (metrics ? groupMetrics(metrics, packLabel) : []), [metrics, packLabel]);

  const lo = parseNumber(threshold);
  const hi = parseNumber(thresholdHi);
  const preview =
    metric && lo !== null && (comparator !== "between" || hi !== null) && due && (effectiveWindow !== "quarter" || QUARTER.test(quarterText))
      ? claimPreview({ metricLabel: metric.label, unit: metric.unit, comparator, threshold: lo, thresholdHi: comparator === "between" ? hi : null, window: windowValue, due, since: today, currency })
      : null;

  const validate = (): Partial<Record<Field, string>> => {
    const e: Partial<Record<Field, string>> = {};
    const s = statement.trim();
    if (!s) e.statement = "Tulis pernyataan klaim";
    else if (s.length > STATEMENT_MAX) e.statement = `Paling banyak ${STATEMENT_MAX} karakter`;
    if (!metric) e.metric = "Pilih metrik yang diuji";
    if (lo === null) e.threshold = "Isi ambang berupa angka";
    if (comparator === "between") {
      if (hi === null) e.threshold_hi = "Isi batas atas berupa angka";
      else if (lo !== null && hi < lo) e.threshold_hi = "Batas atas tidak boleh di bawah batas bawah";
    }
    if (effectiveWindow === "quarter" && !QUARTER.test(quarterText)) e.window = "Tulis kuartal sebagai 2026Q3";
    if (!due) e.due = "Pilih tanggal jatuh tempo";
    else if (due < today) e.due = "Jatuh tempo tidak boleh di masa lalu";
    return e;
  };

  const submit = async (ev: FormEvent) => {
    ev.preventDefault();
    if (saving) return;
    const found = validate();
    setErrors(found);
    setFormError(null);
    if (Object.keys(found).length) return;
    const body: ClaimCreateBody = {
      statement: statement.trim(),
      metric: metricId,
      comparator,
      threshold: lo!,
      threshold_hi: comparator === "between" ? hi : null,
      window: windowValue,
      due,
    };
    setSaving(true);
    try {
      await createClaim(ticker, body, { idempotencyKey: key.forBody(body) });
      key.reset();
      onDone(true);
    } catch (e) {
      if (!(e instanceof ApiError)) {
        setFormError("Klaim belum tersimpan");
        return;
      }
      const mapped: Partial<Record<Field, string>> = {};
      const other: string[] = [];
      for (const [k, msg] of Object.entries(e.fields)) {
        if ((FIELDS as readonly string[]).includes(k)) mapped[k as Field] = msg;
        else other.push(msg);
      }
      setErrors(mapped);
      if (e.kind === "network" || e.kind === "timeout") setFormError("Tidak ada koneksi, klaim belum tersimpan");
      else if (other.length) setFormError(other.join(" · "));
      else if (!Object.keys(mapped).length) setFormError(e.message || MESSAGES.validation);
    } finally {
      setSaving(false);
    }
  };

  // Esc menutup editor saja, bukan overlay pane di sekitarnya.
  const onKeyDown = (e: KeyboardEvent<HTMLFormElement>) => {
    if (e.key !== "Escape" || e.defaultPrevented) return;
    e.preventDefault();
    onDone(false);
  };

  return (
    <form ref={form} id={formId} aria-label="Klaim baru" noValidate onSubmit={submit} onKeyDown={onKeyDown} className="flex flex-col gap-3 rounded-md border border-border-hairline p-3">
      <FormField
        label="Pernyataan"
        multiline
        rows={2}
        autoFocus
        maxLength={STATEMENT_MAX}
        value={statement}
        onChange={(e) => setStatement(e.target.value)}
        hint={`${statement.trim().length}/${STATEMENT_MAX}`}
        error={errors.statement}
        required
      />
      {metrics === undefined && metricsError ? (
        <div role="alert" className="flex flex-col items-start gap-1">
          <span className="text-label font-semibold text-text">Metrik</span>
          <p className="text-caption text-status-over">{metricsError.message}</p>
          <Button variant="ghost" onClick={metricsError.retry}>
            Coba lagi
          </Button>
        </div>
      ) : metrics === undefined ? (
        <div role="status" className="flex flex-col gap-2">
          <span className="text-label font-semibold text-text">Metrik</span>
          <Skeleton height={48} />
          <span className="sr-only">Memuat daftar metrik…</span>
        </div>
      ) : (
        <FormField label="Metrik" error={errors.metric} emptyMessage={metrics.length ? undefined : "Belum ada metrik yang bisa diuji"}>
          {(a11y) => (
            <select {...a11y} value={metricId} onChange={(e) => setMetricId(e.target.value)} className={selectClass}>
              <option value="">Pilih metrik</option>
              {groups.map(([name, items]) => (
                <optgroup key={name} label={name}>
                  {items.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.label}
                    </option>
                  ))}
                </optgroup>
              ))}
            </select>
          )}
        </FormField>
      )}
      <FormField label="Pembanding" error={errors.comparator}>
        {(a11y) => (
          <select {...a11y} value={comparator} onChange={(e) => setComparator(e.target.value as Comparator)} className={selectClass}>
            {COMPARATORS.map((c) => (
              <option key={c.value} value={c.value}>
                {c.label}
              </option>
            ))}
          </select>
        )}
      </FormField>
      <div className="grid grid-cols-2 gap-2">
        <FormField
          label={`${comparator === "between" ? "Batas bawah" : "Ambang"}${unit ? ` (${unit})` : ""}`}
          inputMode="decimal"
          value={threshold}
          onChange={(e) => setThreshold(e.target.value)}
          error={errors.threshold}
          className={comparator === "between" ? undefined : "col-span-2"}
        />
        {comparator === "between" ? (
          <FormField label={`Batas atas${unit ? ` (${unit})` : ""}`} inputMode="decimal" value={thresholdHi} onChange={(e) => setThresholdHi(e.target.value)} error={errors.threshold_hi} />
        ) : null}
      </div>
      <FormField label="Jendela evaluasi" error={effectiveWindow === "quarter" ? undefined : errors.window}>
        {(a11y) => (
          <select {...a11y} value={effectiveWindow} onChange={(e) => setWindowKind(e.target.value as WindowKind)} className={selectClass}>
            {windows.map((w) => (
              <option key={w.value} value={w.value}>
                {w.label}
              </option>
            ))}
          </select>
        )}
      </FormField>
      {effectiveWindow === "quarter" ? <FormField label="Kuartal" placeholder="2026Q3" value={quarter} onChange={(e) => setQuarter(e.target.value)} error={errors.window} /> : null}
      <FormField label="Jatuh tempo" type="date" min={today} value={due} onChange={(e) => setDue(e.target.value)} error={errors.due} />
      {/* Bukan wilayah live: kalimat berubah di setiap ketikan; terbaca sebagai deskripsi tombol Simpan. */}
      <p id={`${formId}-preview`} className="text-caption text-text-2">
        {preview ?? "Lengkapi metrik, ambang, dan jatuh tempo untuk melihat kapan klaim dievaluasi."}
      </p>
      {formError ? (
        <p role="alert" className="text-caption text-status-over">
          {formError}
        </p>
      ) : null}
      <div className="-ml-3 flex gap-1">
        <Button variant="ghost" type="submit" loading={saving} aria-describedby={`${formId}-preview`}>
          Simpan
        </Button>
        <Button variant="ghost" onClick={() => onDone(false)}>
          Batal
        </Button>
      </div>
    </form>
  );
}
