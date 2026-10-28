import { useState } from "react";
import { Link } from "react-router-dom";
import { Button } from "../../../components/Button";
import { FormField } from "../../../components/FormField";
import { ApiError } from "../../../lib/api";
import { removeFromUniverse, usePacks } from "../api";
import { LETTERS, TICKER_LENGTH, tickerBlocker, type TickerCheck } from "./useTickerCheck";

export interface StepTickerProps {
  ticker: string;
  onTicker: (ticker: string) => void;
  /** `""` mengikuti kode laporan; selain itu id paket yang dipilih sendiri. */
  pack: string;
  onPack: (pack: string) => void;
  check: TickerCheck;
  /** Tombol lanjut sudah ditekan: kesalahan isian ikut tampil. */
  showErrors: boolean;
  onEnter: () => void;
}

/** Langkah 1: kode emiten, dicek ke daftar bursa; kode industri dari laporan dan paket usulan. */
export function StepTicker({ ticker, onTicker, pack, onPack, check, showErrors, onEnter }: StepTickerProps) {
  const packs = usePacks();
  const [removeError, setRemoveError] = useState<string | null>(null);
  const [removing, setRemoving] = useState(false);
  const { item } = check;

  let error: string | undefined;
  if (ticker && !LETTERS.test(ticker)) error = "Hanya huruf A sampai Z";
  else if (ticker.length !== TICKER_LENGTH) error = showErrors ? (ticker ? "Kode emiten terdiri dari empat huruf" : "Isi kode emiten") : undefined;
  else if (!check.pending) error = check.failed || (check.listAvailable && !item) ? (tickerBlocker(ticker, check) ?? undefined) : undefined;

  const held = item?.in_universe && item.role === "held";
  const manual = item?.in_universe && item.role === "manual";
  const peerOnly = item?.in_universe && item.role === "peer";
  const proposed = item?.pack ?? null;
  const choosable = (packs.data?.packs ?? []).filter((p) => p.id !== "core" && p.id !== proposed?.id);

  const remove = async () => {
    setRemoving(true);
    setRemoveError(null);
    try {
      await removeFromUniverse(ticker);
    } catch (e) {
      setRemoveError(e instanceof ApiError ? e.message : "Emiten belum terhapus");
    } finally {
      setRemoving(false);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <FormField
        label="Kode emiten"
        hint="Empat huruf, seperti di bursa"
        value={ticker}
        error={error}
        autoFocus
        autoComplete="off"
        spellCheck={false}
        maxLength={TICKER_LENGTH}
        onChange={(e) => onTicker(e.target.value.toUpperCase().replace(/\s/g, ""))}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            onEnter();
          }
        }}
      />
      <div role="status" aria-live="polite" className="flex flex-col gap-2">
        {check.pending ? <p className="text-label text-text-2">Memeriksa kode…</p> : null}
        {check.failed ? (
          <p className="text-label text-text-2">
            <button type="button" className="font-bold text-accent" onClick={check.reload}>
              Coba lagi
            </button>
          </p>
        ) : null}
        {check.listAvailable === false && !check.pending ? <p className="text-label text-text-2">Daftar bursa tidak tersedia, jadi kode ini belum bisa diperiksa. Pastikan penulisannya benar.</p> : null}
        {item && !check.pending ? (
          <dl className="grid grid-cols-[120px_minmax(0,1fr)] gap-x-3 gap-y-1 rounded-md bg-surface-sunken p-4 text-label">
            <dt className="text-text-2">Nama</dt>
            <dd className="text-text">{item.name ?? "Tidak ada di daftar"}</dd>
            <dt className="text-text-2">Kode industri</dt>
            <dd className="text-text">{item.industry_code ?? "Belum ada laporan"}</dd>
            <dt className="text-text-2">Paket usulan</dt>
            <dd className="text-text">{proposed ? proposed.label : "Belum ada paket untuk kode ini"}</dd>
          </dl>
        ) : null}
        {held ? (
          <p className="text-label text-text">
            {`${item.ticker} dipegang di portofolio: sudah ada di Riset dan tidak bisa dihapus. `}
            <Link className="font-bold text-accent" to={`/research/${item.ticker}`}>
              Buka dossier
            </Link>
          </p>
        ) : null}
        {manual ? (
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-label text-text">
              {`${item.ticker} sudah ada di Riset. `}
              <Link className="font-bold text-accent" to={`/research/${item.ticker}`}>
                Buka dossier
              </Link>
            </p>
            <Button variant="ghost" tone="danger" loading={removing} onClick={remove}>
              {`Hapus ${item.ticker} dari Riset`}
            </Button>
          </div>
        ) : null}
        {peerOnly ? <p className="text-label text-text-2">{`${item.ticker} sudah ada sebagai pembanding; melanjutkan menjadikannya emiten manual.`}</p> : null}
        {removeError ? (
          <p role="alert" className="text-caption text-status-over">
            {removeError}
          </p>
        ) : null}
      </div>
      {item && !check.pending && !held && !manual ? (
        <fieldset className="flex flex-col gap-1">
          <legend className="text-label font-bold text-text">Paket</legend>
          {packs.status === "error" ? <p className="text-caption text-status-over">Gagal memuat daftar paket</p> : null}
          <label className="flex min-h-11 items-center gap-3 rounded-sm px-2 hover:bg-surface-sunken">
            <input type="radio" name="add-pack" checked={pack === ""} onChange={() => onPack("")} className="h-4 w-4 accent-[var(--accent)]" />
            <span className="text-label text-text">{proposed ? `Ikuti kode industri: ${proposed.label}` : "Tanpa paket dulu"}</span>
          </label>
          {choosable.map((p) => (
            <label key={p.id} className="flex min-h-11 items-center gap-3 rounded-sm px-2 hover:bg-surface-sunken">
              <input type="radio" name="add-pack" checked={pack === p.id} onChange={() => onPack(p.id)} className="h-4 w-4 accent-[var(--accent)]" />
              <span className="text-label text-text">{p.label}</span>
            </label>
          ))}
        </fieldset>
      ) : null}
    </div>
  );
}
