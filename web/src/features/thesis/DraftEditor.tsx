import { useEffect, useId, useState, type FormEvent } from "react";
import { Plus, X } from "lucide-react";
import { Button } from "../../components/Button";
import { FormField } from "../../components/FormField";
import { Sheet } from "../../components/Sheet";
import { api, ApiError, MESSAGES } from "../../lib/api";
import { cn } from "../../lib/cn";
import { invalidate } from "../../lib/useResource";
import { SCENARIOS, STORY_STATUSES, THESIS_TYPES, type DraftInput } from "../../screens/thesis/types";
import { FIELD } from "../riset/ui";

/** Isian yang belum terkirim, satu kunci per ticker (finance-app memakai awalan `finance.`; asal app ini terpisah). */
export const DRAFT_STORAGE_PREFIX = "investing.thesisDraft.";

const emptyDraft = (ticker: string): DraftInput => ({
  ticker,
  title: "",
  thesis_statement: "",
  thesis_type: "fundamental",
  is_primary: false,
  scenario_initial: "Menunggu",
  story_status_initial: "Belum terkonfirmasi",
  falsifiers: [""],
  triggers_add: [""],
  triggers_exit: [""],
  notes: "",
});

function readLocal(ticker: string): DraftInput | null {
  try {
    const raw = window.localStorage.getItem(DRAFT_STORAGE_PREFIX + ticker);
    return raw ? { ...emptyDraft(ticker), ...(JSON.parse(raw) as Partial<DraftInput>), ticker } : null;
  } catch {
    return null;
  }
}
function writeLocal(ticker: string, value: DraftInput | null) {
  try {
    if (value) window.localStorage.setItem(DRAFT_STORAGE_PREFIX + ticker, JSON.stringify(value));
    else window.localStorage.removeItem(DRAFT_STORAGE_PREFIX + ticker);
  } catch {
    /* storage diblokir: draf tetap di form selama sesi */
  }
}

function ListField({ label, values, onChange, addLabel }: { label: string; values: string[]; onChange: (v: string[]) => void; addLabel: string }) {
  return (
    <fieldset className="m-0 flex flex-col gap-2 border-0 p-0">
      <legend className="mb-2 text-label font-semibold text-text">{label}</legend>
      {values.map((v, i) => (
        <div key={i} className="flex items-center gap-2">
          <input aria-label={`${label} ${i + 1}`} value={v} onChange={(e) => onChange(values.map((x, j) => (j === i ? e.target.value : x)))} className={cn(FIELD, "min-w-0 flex-1 focus-visible:border-accent")} />
          <Button variant="icon" aria-label={`Hapus ${label.toLowerCase()} ${i + 1}`} icon={<X />} onClick={() => onChange(values.length > 1 ? values.filter((_, j) => j !== i) : [""])} />
        </div>
      ))}
      <Button variant="ghost" icon={<Plus />} onClick={() => onChange([...values, ""])} className="-ml-3 self-start">
        {addLabel}
      </Button>
    </fieldset>
  );
}

function SelectField({ label, value, options, onChange }: { label: string; value: string; options: readonly string[]; onChange: (v: string) => void }) {
  return (
    <FormField label={label}>
      {(a11y) => (
        <select {...a11y} value={value} onChange={(e) => onChange(e.target.value)} className={cn(FIELD, "focus-visible:border-accent")}>
          {options.map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </select>
      )}
    </FormField>
  );
}

/**
 * Editor draf tesis untuk satu ticker dalam Sheet setinggi layar (design.md §3.4; field finance-app design.md §3.4, logika
 * finance-app `ThesisScreen.tsx` `DraftEditor`). Ticker tetap = halaman ini. Isian disimpan lokal per ticker sampai terkirim,
 * jadi menutup Sheet tidak membuangnya; `POST /api/thesis/drafts` hanya menulis berkas draf, tidak pernah vault.
 * **Simpan draf** (52 px) ada di `footer` Sheet, sticky di bawah, terhubung ke form lewat atribut `form`.
 * Dipasang hanya saat terbuka, jadi isian dibaca ulang dari `localStorage` setiap kali dibuka.
 */
export function DraftEditorSheet({ ticker, onClose }: { ticker: string; onClose: () => void }) {
  const formId = useId();
  const [form, setForm] = useState<DraftInput>(() => readLocal(ticker) ?? emptyDraft(ticker));
  const [errors, setErrors] = useState<Partial<Record<keyof DraftInput | "_", string>>>({});
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    const blank = JSON.stringify(form) === JSON.stringify(emptyDraft(ticker));
    writeLocal(ticker, blank ? null : form);
  }, [form, ticker]);

  const set = <K extends keyof DraftInput>(k: K, v: DraftInput[K]) => {
    setErrors((e) => ({ ...e, [k]: undefined, _: undefined }));
    setForm((f) => ({ ...f, [k]: v }));
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (saving) return;
    const next: typeof errors = {};
    if (!form.title.trim()) next.title = "Judul tesis wajib diisi";
    if (!form.thesis_statement.trim()) next.thesis_statement = "Thesis statement wajib diisi";
    if (Object.keys(next).length) return setErrors(next);
    setSaving(true);
    const clean = (xs: string[]) => xs.map((x) => x.trim()).filter(Boolean);
    try {
      await api.post("/api/thesis/drafts", {
        ...form,
        ticker,
        title: form.title.trim(),
        thesis_statement: form.thesis_statement.trim(),
        falsifiers: clean(form.falsifiers),
        triggers_add: clean(form.triggers_add),
        triggers_exit: clean(form.triggers_exit),
      });
      writeLocal(ticker, null);
      invalidate("/api/thesis/");
      onClose();
    } catch (err) {
      const apiErr = err instanceof ApiError ? err : null;
      const fields: typeof errors = {};
      for (const [k, m] of Object.entries(apiErr?.fields ?? {})) if (k in form) fields[k as keyof DraftInput] = m;
      fields._ = apiErr?.message ?? MESSAGES.server;
      setErrors(fields);
      setSaving(false);
    }
  };

  return (
    <Sheet
      open
      title="Tulis draf tesis"
      onClose={onClose}
      className="h-[90vh] supports-[height:100dvh]:h-[90dvh]"
      footer={
        <Button type="submit" form={formId} fullWidth loading={saving}>
          Simpan draf
        </Button>
      }
    >
      <form id={formId} onSubmit={submit} noValidate className="flex flex-col gap-5 pb-2">
        <FormField label="Ticker" value={ticker} readOnly hint="Draf untuk halaman perusahaan ini" />
        <FormField label="Judul" value={form.title} error={errors.title} onChange={(e) => set("title", e.target.value)} />
        <FormField label="Thesis statement" multiline rows={6} value={form.thesis_statement} error={errors.thesis_statement} onChange={(e) => set("thesis_statement", e.target.value)} />
        <SelectField label="Tipe tesis" value={form.thesis_type} options={THESIS_TYPES} onChange={(v) => set("thesis_type", v)} />
        <button
          type="button"
          role="switch"
          aria-checked={form.is_primary}
          onClick={() => set("is_primary", !form.is_primary)}
          className="flex min-h-11 items-center justify-between gap-3 text-left text-label font-semibold text-text"
        >
          Tesis utama
          <span className={cn("relative h-7 w-12 rounded-pill transition-colors", form.is_primary ? "bg-accent" : "bg-border-strong")} aria-hidden="true">
            <span className={cn("absolute top-0.5 h-6 w-6 rounded-pill bg-bg transition-[left]", form.is_primary ? "left-[22px]" : "left-0.5")} />
          </span>
        </button>
        <SelectField label="Skenario awal" value={form.scenario_initial} options={SCENARIOS} onChange={(v) => set("scenario_initial", v)} />
        <SelectField label="Story status awal" value={form.story_status_initial} options={STORY_STATUSES} onChange={(v) => set("story_status_initial", v)} />
        <ListField label="Falsifiers" values={form.falsifiers} onChange={(v) => set("falsifiers", v)} addLabel="Tambah falsifier" />
        <ListField label="Trigger tambah posisi" values={form.triggers_add} onChange={(v) => set("triggers_add", v)} addLabel="Tambah trigger" />
        <ListField label="Trigger keluar" values={form.triggers_exit} onChange={(v) => set("triggers_exit", v)} addLabel="Tambah trigger" />
        <FormField label="Catatan" multiline rows={4} value={form.notes} onChange={(e) => set("notes", e.target.value)} hint="Isian tersimpan di perangkat ini sampai draf terkirim" />
        {errors._ ? (
          <p role="alert" className="text-label font-semibold text-status-over">
            {errors._}
          </p>
        ) : null}
      </form>
    </Sheet>
  );
}
