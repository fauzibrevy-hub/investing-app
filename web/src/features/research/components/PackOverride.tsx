import { useEffect, useId, useRef, useState } from "react";
import { Button } from "../../../components/Button";
import { Skeleton } from "../../../components/Skeleton";
import { cn } from "../../../lib/cn";
import { ApiError } from "../../../lib/api";
import { updateUniverse, usePacks } from "../api";
import type { DossierResponse } from "../types";

/** Nilai radio untuk "ikuti kode laporan" (mengosongkan `pack_override`). */
const AUTO = "__auto__";

export interface PackOverrideProps {
  ticker: string;
  pack: DossierResponse["pack"];
  className?: string;
}

/**
 * **Ubah paket** / **Tentukan paket**: pilih paket dari `GET /packs` atau ikuti kode laporan, lalu
 * `PUT /universe/{ticker}` `{pack_override}`. Panel non-modal di bawah tombol; Esc menutup dan mengembalikan fokus ke tombol.
 */
export function PackOverride({ ticker, pack, className }: PackOverrideProps) {
  const panelId = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [choice, setChoice] = useState<string>(AUTO);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const packs = usePacks();

  const close = () => {
    setOpen(false);
    trigger.current?.focus();
  };
  const show = () => {
    setChoice(pack.source === "override" && pack.id ? pack.id : AUTO);
    setError(null);
    setOpen(true);
  };
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      e.preventDefault();
      setOpen(false);
      trigger.current?.focus();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open]);

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      await updateUniverse(ticker, { pack_override: choice === AUTO ? null : choice });
      close();
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "Paket belum tersimpan");
    } finally {
      setSaving(false);
    }
  };

  const options = packs.data
    ? [{ id: AUTO, label: "Ikuti kode laporan", hint: pack.id && pack.source === "override" ? "mengosongkan pilihan sendiri" : "paket bawaan dari laporan" }, ...packs.data.packs.map((p) => ({ id: p.id, label: p.label, hint: `${p.id} · ${p.version}` }))]
    : [];

  return (
    <div className={cn("relative", className)}>
      <Button ref={trigger} variant="ghost" aria-expanded={open} aria-controls={open ? panelId : undefined} onClick={() => (open ? close() : show())}>
        {pack.action ?? "Ubah paket"}
      </Button>
      {open ? (
        <div id={panelId} role="group" aria-label="Pilih paket industri" className="absolute left-0 top-12 z-30 flex w-[320px] flex-col gap-3 rounded-md border border-border-hairline bg-bg p-4 shadow-popover">
          {packs.status === "loading" ? <Skeleton height={44} /> : null}
          {packs.status === "error" ? (
            <p role="alert" className="text-label text-status-over">
              Gagal memuat paket{" "}
              <button type="button" className="font-bold text-accent" onClick={packs.reload}>
                Coba lagi
              </button>
            </p>
          ) : null}
          {options.length ? (
            <fieldset className="flex flex-col gap-1">
              <legend className="sr-only">Paket industri</legend>
              {options.map((o) => (
                <label key={o.id} className="flex min-h-11 cursor-pointer items-center gap-3 rounded-sm px-2 hover:bg-surface-sunken">
                  <input type="radio" name={`${panelId}-pack`} value={o.id} checked={choice === o.id} onChange={() => setChoice(o.id)} className="h-4 w-4 accent-[var(--accent)]" />
                  <span className="flex min-w-0 flex-col">
                    <span className="text-label font-semibold text-text">{o.label}</span>
                    <span className="truncate text-caption text-text-2">{o.hint}</span>
                  </span>
                </label>
              ))}
            </fieldset>
          ) : null}
          {error ? (
            <p role="alert" className="text-caption text-status-over">
              {error}
            </p>
          ) : null}
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={close}>
              Batal
            </Button>
            <Button variant="ghost" loading={saving} disabled={!packs.data} onClick={save}>
              Simpan
            </Button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
