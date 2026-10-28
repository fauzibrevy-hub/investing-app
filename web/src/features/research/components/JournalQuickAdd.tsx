import { useEffect, useId, useLayoutEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { Button } from "../../../components/Button";
import { Segmented } from "../../../components/Segmented";
import { cn } from "../../../lib/cn";
import { ApiError, newIdempotencyKey } from "../../../lib/api";
import { createJournalEntry } from "../api";
import { formatValue } from "../format";
import type { JournalCreateBody } from "../types";
import { parseNumber } from "./ClaimEditor";

export const JOURNAL_TEXT_MAX = 2000;
/** Pesan design.md §6 saat tulis gagal karena koneksi. */
export const OFFLINE_MESSAGE = "Tidak ada koneksi, catatan belum tersimpan";

type Kind = JournalCreateBody["kind"];

const KINDS: Array<{ value: Kind; label: string }> = [
  { value: "catatan", label: "Catatan" },
  { value: "level", label: "Level" },
  { value: "keputusan", label: "Keputusan" },
];

/** Tinggi satu baris teks `type-body` + padding (satu baris = 44 px, target sentuh); isian tumbuh sampai tiga baris. */
const LINE_PX = 22;

const PAD_PX = 22;

/*
 * Layar pendek (landscape, varian `[@media(max-height:500px)]:`): komposer ringkas dua baris, jenis catatan di atas, isian dan Simpan sebaris, petunjuk hanya
 * untuk pembaca layar. Tingginya ≈ 93 px agar komposer lengket yang tertahan di awal kolom (header dossier di atasnya
 * ≈ 184 px) tetap di atas FAB dan bottom nav pada layar setinggi 375-393 px.
 */

export interface JournalQuickAddProps {
  ticker: string;
  /** Penutupan terakhir: placeholder harga dan stempel yang dicatat otomatis (`price_at`). */
  close: number | null;
  /** Naik setiap `n` ditekan: fokus pindah ke isian. */
  focusRequest?: number;
  /** Setelah catatan tersimpan. */
  onSaved?: () => void;
  className?: string;
}

/**
 * Catatan cepat (design.md §3.5, §6): satu baris yang tumbuh sampai tiga, jenis `Segmented` catatan · level ·
 * keputusan, harga level opsional dengan placeholder penutupan terakhir. Enter menyimpan (`Shift+Enter` baris baru)
 * lewat `POST /{ticker}/journal`; percobaan ulang draf yang sama memakai `Idempotency-Key` yang sama.
 */
export function JournalQuickAdd({ ticker, close, focusRequest = 0, onSaved, className }: JournalQuickAddProps) {
  const id = useId();
  const input = useRef<HTMLTextAreaElement>(null);
  const [kind, setKind] = useState<Kind>("catatan");
  const [text, setText] = useState("");
  const [level, setLevel] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [fieldErrors, setFieldErrors] = useState<{ text?: string; level?: string }>({});
  const [saved, setSaved] = useState("");
  const key = useRef(newIdempotencyKey());
  const closeText = close !== null ? formatValue(close, Number.isInteger(close) ? 0 : 2) : null;

  // Setiap permintaan dilayani sekali: membuka ulang separuh yang terlipat tidak mencuri fokus lagi.
  const handled = useRef(0);
  useEffect(() => {
    if (focusRequest <= handled.current) return;
    handled.current = focusRequest;
    input.current?.focus();
  }, [focusRequest]);

  // Tumbuh sampai tiga baris, lalu menggulir di dalam isian.
  useLayoutEffect(() => {
    const el = input.current;
    if (!el) return;
    el.style.height = "auto";
    const max = LINE_PX * 3 + PAD_PX;
    el.style.height = `${Math.min(Math.max(el.scrollHeight, LINE_PX + PAD_PX), max)}px`;
  }, [text]);

  // Draf berubah: kunci baru, karena isinya bukan lagi percobaan ulang tulisan yang sama.
  const edit = <T,>(set: (v: T) => void) => (v: T) => {
    key.current = newIdempotencyKey();
    setSaved("");
    set(v);
  };

  const save = async () => {
    if (saving) return;
    const body = text.trim();
    const errs: { text?: string; level?: string } = {};
    if (!body) errs.text = "Tulis isi catatan";
    else if (body.length > JOURNAL_TEXT_MAX) errs.text = `Paling banyak ${JOURNAL_TEXT_MAX} karakter`;
    const price = kind === "level" ? parseNumber(level) : null;
    if (kind === "level" && (price === null || price <= 0)) errs.level = "Isi harga level di atas nol";
    setFieldErrors(errs);
    setError(null);
    if (Object.keys(errs).length) return;
    setSaving(true);
    try {
      await createJournalEntry(ticker, { kind, text: body, level: kind === "level" ? price : null }, { idempotencyKey: key.current });
      key.current = newIdempotencyKey();
      // Hanya isi yang tersimpan yang dikosongkan: ketikan selama permintaan berjalan tetap ada.
      setText((t) => (t.trim() === body ? "" : t));
      setLevel((l) => (kind === "level" && parseNumber(l) === price ? "" : l));
      setSaved("Tersimpan");
      onSaved?.();
    } catch (e) {
      if (e instanceof ApiError && (e.kind === "network" || e.kind === "timeout")) setError(OFFLINE_MESSAGE);
      else if (e instanceof ApiError && Object.keys(e.fields).length) {
        setFieldErrors({ text: e.fields.text, level: e.fields.level });
        const rest = Object.entries(e.fields).filter(([k]) => k !== "text" && k !== "level");
        if (rest.length) setError(rest.map(([, m]) => m).join(" · "));
      } else setError(e instanceof ApiError ? e.message : "Catatan belum tersimpan");
    } finally {
      setSaving(false);
    }
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      void save();
    }
  };
  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    void save();
  };

  const textError = fieldErrors.text ? `${id}-text-error` : undefined;
  const levelError = fieldErrors.level ? `${id}-level-error` : undefined;
  return (
    <form aria-label="Catatan baru" noValidate onSubmit={onSubmit} className={cn("flex flex-col gap-2 [@media(max-height:500px)]:grid [@media(max-height:500px)]:grid-cols-[minmax(0,1fr)_auto] [@media(max-height:500px)]:gap-0", className)}>
      <Segmented label="Jenis catatan" options={KINDS} value={kind} onChange={edit(setKind)} className={kind === "level" ? "[@media(max-height:500px)]:col-start-1" : "[@media(max-height:500px)]:col-span-2"} />
      <label htmlFor={`${id}-text`} className="sr-only">
        Isi catatan
      </label>
      <textarea
        ref={input}
        id={`${id}-text`}
        rows={1}
        value={text}
        maxLength={JOURNAL_TEXT_MAX}
        onChange={(e) => edit(setText)(e.target.value)}
        onKeyDown={onKeyDown}
        placeholder={closeText ? `Tulis ${kind} · harga kini ${closeText}` : `Tulis ${kind}`}
        aria-invalid={fieldErrors.text ? true : undefined}
        aria-describedby={[textError, `${id}-hint`].filter(Boolean).join(" ")}
        className={cn(
          "min-h-11 w-full resize-none overflow-y-auto rounded-sm border bg-surface-sunken px-3 py-[11px] text-body text-text placeholder:text-text-2 focus-visible:border-accent [@media(max-height:500px)]:col-start-1 [@media(max-height:500px)]:row-start-2",
          fieldErrors.text ? "border-status-over" : "border-transparent",
        )}
      />
      {fieldErrors.text ? (
        <p role="alert" id={textError} className="text-caption text-status-over [@media(max-height:500px)]:col-span-2">
          {fieldErrors.text}
        </p>
      ) : null}
      {kind === "level" ? (
        <div className="flex flex-col gap-1 [@media(max-height:500px)]:col-start-2 [@media(max-height:500px)]:row-start-1 [@media(max-height:500px)]:w-32">
          <label htmlFor={`${id}-level`} className="text-label font-semibold text-text [@media(max-height:500px)]:sr-only">
            Harga level
          </label>
          <input
            id={`${id}-level`}
            inputMode="decimal"
            value={level}
            onChange={(e) => edit(setLevel)(e.target.value)}
            placeholder={closeText ?? undefined}
            aria-invalid={fieldErrors.level ? true : undefined}
            aria-describedby={levelError}
            className={cn("num h-11 w-full rounded-sm border bg-surface-sunken px-3 text-right text-body text-text placeholder:text-text-2", fieldErrors.level ? "border-status-over" : "border-transparent")}
          />
          {fieldErrors.level ? (
            <p role="alert" id={levelError} className="text-caption text-status-over [@media(max-height:500px)]:col-span-2">
              {fieldErrors.level}
            </p>
          ) : null}
        </div>
      ) : null}
      <div className="flex items-center justify-between gap-2 [@media(max-height:500px)]:col-start-2 [@media(max-height:500px)]:row-start-2 [@media(max-height:500px)]:pl-2">
        <p id={`${id}-hint`} className="text-caption text-text-2 [@media(max-height:500px)]:sr-only">
          {closeText ? `Enter menyimpan · harga ${closeText} dicatat otomatis` : "Enter menyimpan · belum ada harga untuk dicatat"}
        </p>
        <Button variant="ghost" type="submit" loading={saving}>
          Simpan
        </Button>
      </div>
      <p role="status" aria-live="polite" className="sr-only">
        {saved}
      </p>
      {error ? (
        <p role="alert" className="text-caption text-status-over">
          {error}
        </p>
      ) : null}
    </form>
  );
}
