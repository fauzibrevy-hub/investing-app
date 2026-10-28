import { useEffect, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { Button } from "../../../components/Button";
import { ApiError, newIdempotencyKey } from "../../../lib/api";
import { BackHeader } from "../../../shell/BackHeader";
import { addToUniverse, usePacks } from "../../research/api";
import { StepNote, noteError } from "../../research/add/StepNote";
import { StepPeers } from "../../research/add/StepPeers";
import { StepSummary } from "../../research/add/StepSummary";
import { StepTicker } from "../../research/add/StepTicker";
import { tickerBlocker, useTickerCheck } from "../../research/add/useTickerCheck";
import { PAGE } from "../ui";

export const STEPS = ["Emiten", "Pembanding", "Bisnis", "Ringkasan"] as const;

/** Kunci idempotensi per isi permintaan: percobaan ulang yang sama tidak menulis dua kali, isi baru dapat kunci baru. */
function useBodyKeys() {
  const map = useRef(new Map<string, string>());
  return (body: unknown) => {
    const text = JSON.stringify(body);
    let key = map.current.get(text);
    if (!key) {
      key = newIdempotencyKey();
      map.current.set(text, key);
    }
    return key;
  };
}

/** Kode dari `?ticker=` (design.md §3.4): hanya empat huruf yang dipakai; yang lain diabaikan, bukan dipotong menjadi kode lain. */
function prefill(param: string | null): string {
  return param && /^[A-Za-z]{4}$/.test(param) ? param.toUpperCase() : "";
}

/**
 * Tambah emiten `#/perusahaan/tambah` (ui-slicing.md §6): selebar kolom di iPhone, langkah ditulis "Langkah 2 dari 4", tombol utama
 * lengket di atas navigasi bawah. **Mulai** menambahkan emiten (backend mengantre pengambilan harga dan menulis daftar
 * pantau scraper), lalu pembanding, lalu membuka halaman perusahaan di tabel Kuartal terakhir. `?ticker=<T>` mengisi kode
 * dan memeriksanya sekali saat layar dibuka (dari "Tambah ke riset" di halaman tesis).
 */
export function Add() {
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const packs = usePacks();
  const keyFor = useBodyKeys();
  const heading = useRef<HTMLHeadingElement>(null);
  const [step, setStep] = useState(0);
  const [ticker, setTicker] = useState(() => prefill(params.get("ticker")));
  const [pack, setPack] = useState("");
  const [peers, setPeers] = useState<string[]>([]);
  const [note, setNote] = useState("");
  const [showErrors, setShowErrors] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const check = useTickerCheck(ticker);

  useEffect(() => {
    if (step > 0) heading.current?.focus();
  }, [step]);

  const blocker = tickerBlocker(ticker, check);
  const noteProblem = noteError(note);
  const next = () => {
    if (step === 0 && blocker) {
      setShowErrors(true);
      return;
    }
    if (step === 2 && noteProblem) return;
    setShowErrors(false);
    setStep((s) => Math.min(s + 1, STEPS.length - 1));
  };

  const proposed = check.item?.pack ?? null;
  const chosen = pack ? packs.data?.packs.find((p) => p.id === pack) : undefined;
  const packLabel = chosen?.label ?? proposed?.label ?? null;

  const start = async () => {
    setError(null);
    try {
      setBusy(`Menambahkan ${ticker}…`);
      const body = { ticker, role: "manual" as const, ...(pack ? { pack_override: pack } : {}), ...(note.trim() ? { business_note: note.trim() } : {}) };
      await addToUniverse(body, { idempotencyKey: keyFor(body) });
      for (const peer of peers) {
        setBusy(`Menambahkan pembanding ${peer}…`);
        const pb = { ticker: peer, role: "peer" as const, peer_of: ticker };
        await addToUniverse(pb, { idempotencyKey: keyFor(pb) });
      }
      navigate({ pathname: `/perusahaan/${ticker}`, search: "?section=kuartal" });
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "Emiten belum tersimpan");
      setBusy(null);
    }
  };

  const running = busy !== null;
  const last = step === STEPS.length - 1;
  const stepNote = step === 2 ? noteProblem : undefined;

  return (
    <div className={PAGE}>
      <header className="flex flex-col gap-1">
        <BackHeader />
        <h1 className="text-title text-text">Tambah emiten</h1>
      </header>
      <section aria-label="Tambah emiten" className="flex min-w-0 flex-col gap-5 md:max-w-[560px]">
        <h2 ref={heading} tabIndex={-1} className="text-body-strong text-text outline-none">{`Langkah ${step + 1} dari ${STEPS.length}: ${STEPS[step]}`}</h2>
        {step === 0 ? <StepTicker ticker={ticker} onTicker={setTicker} pack={pack} onPack={setPack} check={check} showErrors={showErrors} onEnter={next} /> : null}
        {step === 1 ? <StepPeers ticker={ticker} peers={peers} onChange={setPeers} /> : null}
        {step === 2 ? <StepNote note={note} onChange={setNote} /> : null}
        {step === 3 ? <StepSummary data={{ ticker, item: check.item, packLabel, packSource: pack ? "pilihan" : proposed ? "usulan" : "tanpa", peers, note }} /> : null}
        {error ? (
          <p role="alert" className="text-label text-status-over">
            {error}
          </p>
        ) : null}
        {busy ? (
          <p role="status" className="text-label text-text-2">
            {busy}
          </p>
        ) : null}
      </section>
      {/* Lengket di atas navigasi bawah (dan papan ketik): tombol utama selalu terlihat. */}
      <div data-sticky-actions="" className="sticky bottom-[calc(var(--nav-bottom)+90px)] z-10 -mx-1 flex items-center justify-between gap-3 border-t border-border-hairline bg-bg px-1 py-2 md:max-w-[560px]">
        <Button variant="ghost" disabled={step === 0 || running} onClick={() => setStep((s) => s - 1)}>
          Kembali
        </Button>
        {last ? (
          <Button loading={running} onClick={start}>
            {error ? "Coba lagi" : "Mulai"}
          </Button>
        ) : (
          <Button disabled={stepNote !== undefined} onClick={next}>
            Lanjut
          </Button>
        )}
      </div>
    </div>
  );
}

export default Add;
