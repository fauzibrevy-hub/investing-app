import { useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { Button } from "../../../components/Button";
import { FormField } from "../../../components/FormField";
import { ApiError } from "../../../lib/api";
import { updateUniverse } from "../../research/api";
import { useDraftKey } from "../../research/components/draftKey";

export const BUSINESS_NOTE_MAX = 600;

/**
 * Satu atau dua kalimat tentang cara emiten ini menghasilkan uang, disunting di tempat (`PUT /universe/{ticker}`
 * `business_note`). Teks yang baru tersimpan tampil sampai dossier dimuat ulang, agar teks lama tidak sempat kembali.
 */
export function BusinessNote({ ticker, note: given }: { ticker: string; note: string | null }) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const editButton = useRef<HTMLButtonElement>(null);
  const key = useDraftKey();
  const [saved, setSaved] = useState<{ from: string | null; note: string | null } | null>(null);
  const note = saved && saved.from === given ? saved.note : given;

  const start = () => {
    setText(note ?? "");
    setError(null);
    key.reset();
    setEditing(true);
  };
  const stop = () => {
    setEditing(false);
    requestAnimationFrame(() => editButton.current?.focus());
  };
  const save = async (e: FormEvent) => {
    e.preventDefault();
    const value = text.trim();
    if (value.length > BUSINESS_NOTE_MAX) {
      setError(`Paling banyak ${BUSINESS_NOTE_MAX} karakter`);
      return;
    }
    if (saving) return;
    setSaving(true);
    setError(null);
    const body = { business_note: value || null };
    try {
      await updateUniverse(ticker, body, { idempotencyKey: key.forBody(body) });
      key.reset();
      setSaved({ from: given, note: body.business_note });
      stop();
    } catch (err) {
      if (err instanceof ApiError && (err.kind === "network" || err.kind === "timeout")) setError("Tidak ada koneksi, deskripsi belum tersimpan");
      else setError(err instanceof ApiError ? (err.fields.business_note ?? err.message) : "Deskripsi belum tersimpan");
    } finally {
      setSaving(false);
    }
  };
  const onKeyDown = (e: KeyboardEvent<HTMLFormElement>) => {
    if (e.key === "Escape") {
      e.preventDefault();
      stop();
    }
  };

  if (editing) {
    return (
      <form aria-label="Sunting deskripsi bisnis" noValidate onSubmit={save} onKeyDown={onKeyDown} className="flex flex-col gap-2">
        <FormField label="Deskripsi bisnis" multiline rows={4} autoFocus maxLength={BUSINESS_NOTE_MAX} value={text} onChange={(e) => setText(e.target.value)} hint={`${text.trim().length}/${BUSINESS_NOTE_MAX}`} error={error ?? undefined} />
        <div className="-ml-3 flex gap-1">
          <Button variant="ghost" type="submit" loading={saving}>
            Simpan
          </Button>
          <Button variant="ghost" onClick={stop}>
            Batal
          </Button>
        </div>
      </form>
    );
  }
  return (
    <div className="flex flex-col items-start gap-1">
      {note ? (
        <p data-testid="business-note" className="whitespace-pre-wrap break-words text-body text-text">
          {note}
        </p>
      ) : (
        <p className="text-label text-text-2">Belum ada deskripsi bisnis. Tulis satu atau dua kalimat tentang cara emiten ini menghasilkan uang.</p>
      )}
      <Button ref={editButton} variant="ghost" onClick={start} className="-ml-3">
        {note ? "Ubah deskripsi" : "Tulis deskripsi"}
      </Button>
    </div>
  );
}
