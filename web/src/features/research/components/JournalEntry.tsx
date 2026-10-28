import { useId, useRef, useState, type FormEvent } from "react";
import { Button } from "../../../components/Button";
import { Skeleton } from "../../../components/Skeleton";
import { cn } from "../../../lib/cn";
import { ApiError, newIdempotencyKey } from "../../../lib/api";
import { annotateJournal, useJournal } from "../api";
import { formatDate, formatValue } from "../format";
import type { JournalEntry as JournalEntryData, JournalKind } from "../types";
import { JournalQuickAdd, OFFLINE_MESSAGE } from "./JournalQuickAdd";

export const JOURNAL_PAGE = 20;
const ANNOTATION_MAX = 2000;

const KIND_WORDS: Record<JournalKind, string> = {
  catatan: "catatan",
  level: "level",
  keputusan: "keputusan",
  sistem: "sistem",
};

const WIB_OFFSET_MS = 7 * 3600 * 1000;

/** `2026-10-24T09:00:00Z` → `24 Okt 2026 16.00` (jam WIB, hari WIB seperti backend). */
export function formatTimestamp(at: string): string {
  const t = Date.parse(at);
  if (Number.isNaN(t)) return at;
  const wib = new Date(t + WIB_OFFSET_MS).toISOString();
  return `${formatDate(at)} ${wib.slice(11, 13)}.${wib.slice(14, 16)}`;
}

const price = (n: number) => formatValue(n, Number.isInteger(n) ? 0 : 2);

function AnnotationForm({ entryId, onDone }: { entryId: string; onDone: () => void }) {
  const id = useId();
  const [text, setText] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const key = useRef(newIdempotencyKey());

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const body = text.trim();
    if (!body) {
      setError("Tulis isi anotasi");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await annotateJournal(entryId, body, { idempotencyKey: key.current });
      onDone();
    } catch (err) {
      if (err instanceof ApiError && (err.kind === "network" || err.kind === "timeout")) setError(OFFLINE_MESSAGE);
      else setError(err instanceof ApiError ? (err.fields.text ?? err.message) : "Anotasi belum tersimpan");
    } finally {
      setSaving(false);
    }
  };

  return (
    <form aria-label="Anotasi baru" noValidate onSubmit={submit} className="ml-4 flex flex-col gap-1 border-l border-border-hairline pl-3">
      <label htmlFor={id} className="sr-only">
        Isi anotasi
      </label>
      <input
        id={id}
        autoFocus
        value={text}
        maxLength={ANNOTATION_MAX}
        onChange={(e) => {
          key.current = newIdempotencyKey();
          setText(e.target.value);
        }}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            onDone();
          }
        }}
        placeholder="Tambahkan anotasi"
        aria-invalid={error ? true : undefined}
        aria-describedby={error ? `${id}-error` : undefined}
        className="h-11 w-full rounded-sm border border-transparent bg-surface-sunken px-3 text-body text-text placeholder:text-text-2"
      />
      {error ? (
        <p id={`${id}-error`} role="alert" className="text-caption text-status-over">
          {error}
        </p>
      ) : null}
      <div className="-ml-3 flex gap-1">
        <Button variant="ghost" type="submit" loading={saving}>
          Simpan
        </Button>
        <Button variant="ghost" onClick={onDone}>
          Batal
        </Button>
      </div>
    </form>
  );
}

export interface JournalEntryProps {
  entry: JournalEntryData;
  className?: string;
}

/**
 * Satu entri jurnal (design.md §3.5, §6, §7.10): waktu · jenis · harga saat itu · teks; anotasi menjorok di bawahnya.
 * Entri tidak bisa diubah atau dihapus, jadi tidak ada kontrol edit; yang bisa hanya menambah anotasi.
 * Entri `sistem` memakai `text-2`.
 */
export function JournalEntry({ entry, className }: JournalEntryProps) {
  const [annotating, setAnnotating] = useState(false);
  const addButton = useRef<HTMLButtonElement>(null);
  const system = entry.kind === "sistem";
  const meta = [formatTimestamp(entry.at), entry.price_at !== null ? `harga ${price(entry.price_at)}` : "harga belum ada", entry.level !== null ? `level ${price(entry.level)}` : null].filter(Boolean).join(" · ");

  return (
    <article aria-label={`${KIND_WORDS[entry.kind]} ${formatTimestamp(entry.at)}`} data-kind={entry.kind} className={cn("flex flex-col gap-1 border-b border-border-hairline pb-3 last:border-b-0", className)}>
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span className="inline-flex h-5 items-center rounded-sm bg-surface-sunken px-1.5 text-[11px] font-bold uppercase leading-4 tracking-wide text-text-2">{KIND_WORDS[entry.kind]}</span>
        <span className="text-caption text-text-2">{meta}</span>
      </div>
      <p className={cn("whitespace-pre-wrap break-words text-body", system ? "text-text-2" : "text-text")}>{entry.text}</p>
      {entry.annotations.length ? (
        <ul aria-label="Anotasi" className="ml-4 flex flex-col gap-1.5 border-l border-border-hairline pl-3">
          {entry.annotations.map((a) => (
            <li key={a.id} className="flex flex-col">
              <span className="text-caption text-text-2">{formatTimestamp(a.at)}</span>
              <span className="whitespace-pre-wrap break-words text-label text-text">{a.text}</span>
            </li>
          ))}
        </ul>
      ) : null}
      {annotating ? (
        <AnnotationForm
          entryId={entry.id}
          onDone={() => {
            setAnnotating(false);
            requestAnimationFrame(() => addButton.current?.focus());
          }}
        />
      ) : (
        <div className="-ml-3">
          <Button ref={addButton} variant="ghost" onClick={() => setAnnotating(true)}>
            Tambah anotasi
          </Button>
        </div>
      )}
    </article>
  );
}

export interface JournalPanelProps {
  ticker: string;
  close: number | null;
  focusRequest?: number;
  /**
   * `top` (pane desktop): catatan baru di atas daftar. `sticky` (mobile, design.md §5.2): daftar dulu, catatan baru
   * lengket di bawah, di atas bottom nav.
   */
  composer?: "top" | "sticky";
}

/** Separuh Jurnal di pane kanan: `JournalQuickAdd` lalu entri terbaru dulu, 20 per halaman (`GET /{ticker}/journal`). */
export function JournalPanel({ ticker, close, focusRequest, composer = "top" }: JournalPanelProps) {
  const [offset, setOffset] = useState(0);
  const journal = useJournal(ticker, JOURNAL_PAGE, offset);
  const data = journal.data;
  const total = data?.total ?? 0;
  const quickAdd = <JournalQuickAdd ticker={ticker} close={close} focusRequest={focusRequest} onSaved={() => setOffset(0)} className={composer === "sticky" ? "py-3 [@media(max-height:500px)]:py-0" : undefined} />;

  return (
    <div className="flex flex-col gap-3">
      {/* Catatan baru selalu di halaman pertama: setelah tersimpan daftar kembali ke sana. */}
      {composer === "top" ? quickAdd : null}
      {journal.status === "loading" ? (
        <div role="status" aria-busy="true" className="flex flex-col gap-2">
          <span className="sr-only">Memuat jurnal…</span>
          <Skeleton height={64} />
          <Skeleton height={64} />
        </div>
      ) : null}
      {journal.status === "error" ? (
        <div role="alert" className="flex flex-col items-start gap-1">
          <p className="text-label font-semibold text-status-over">{journal.error?.message ?? "Gagal memuat jurnal"}</p>
          <Button variant="ghost" onClick={journal.reload}>
            Coba lagi
          </Button>
        </div>
      ) : null}
      {journal.status === "ready" && data && data.items.length === 0 ? <p className="text-label text-text-2">Belum ada catatan</p> : null}
      {data && data.items.length ? (
        <section aria-label="Entri jurnal" aria-busy={journal.refreshing || undefined} className="flex flex-col gap-3">
          {data.items.map((e) => (
            <JournalEntry key={e.id} entry={e} />
          ))}
        </section>
      ) : null}
      {total > JOURNAL_PAGE ? (
        <nav aria-label="Halaman jurnal" className="flex items-center justify-between gap-2">
          <Button variant="ghost" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - JOURNAL_PAGE))}>
            Lebih baru
          </Button>
          <span className="num text-caption text-text-2">{`${offset + 1}–${Math.min(offset + JOURNAL_PAGE, total)} dari ${total}`}</span>
          <Button variant="ghost" disabled={offset + JOURNAL_PAGE >= total} onClick={() => setOffset(offset + JOURNAL_PAGE)}>
            Lebih lama
          </Button>
        </nav>
      ) : null}
      {/* Lengket di atas bottom nav, di atas daftar saat digulir. */}
      {composer === "sticky" ? (
        <div data-sticky-composer="" className="sticky bottom-[calc(var(--nav-bottom)+90px)] z-10 -mx-1 border-t border-border-hairline bg-bg px-1">
          {quickAdd}
        </div>
      ) : null}
    </div>
  );
}
