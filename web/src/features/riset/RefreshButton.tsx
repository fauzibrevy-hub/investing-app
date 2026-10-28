import { useState } from "react";
import { Button } from "../../components/Button";
import { ApiError } from "../../lib/api";
import { REFRESH_SETTLE_MS, refreshResearch } from "../research/api";

export interface RefreshButtonProps {
  /** `ticker`: harga dan laporan emiten ini (`POST /refresh?ticker=`). `market`: seluruh pembaruan, termasuk pendorong. */
  scope?: "ticker" | "market";
  /** Wajib untuk `scope="ticker"`. */
  ticker?: string;
  label?: string;
}

/**
 * **Perbarui data**. Backend menjawab 202 dan bekerja di latar belakang, jadi hasilnya muncul sekitar
 * `REFRESH_SETTLE_MS` kemudian; dibatasi satu per menit (429 dijelaskan apa adanya).
 *
 * Pendorong (BI rate, kurs, batu bara …) bukan milik satu emiten: penjadwal sengaja melewatkannya pada pembaruan satu
 * emiten (`scheduler.rs`), jadi celah pendorong memakai `scope="market"` yang memanggil pembaruan tanpa `ticker`.
 */
export function RefreshButton({ scope = "ticker", ticker, label = "Perbarui data" }: RefreshButtonProps) {
  const [state, setState] = useState<"idle" | "busy" | "done">("idle");
  const [error, setError] = useState<string | null>(null);
  const market = scope === "market";
  const run = async () => {
    setState("busy");
    setError(null);
    try {
      await refreshResearch(market ? undefined : ticker);
      setState("done");
    } catch (e) {
      setState("idle");
      setError(e instanceof ApiError ? e.message : "Perbarui data gagal dijalankan");
    }
  };
  const settle = Math.round(REFRESH_SETTLE_MS / 1000);
  return (
    <div className="flex flex-col items-start gap-1">
      <Button variant="ghost" onClick={run} loading={state === "busy"} className="rounded-md bg-surface-sunken px-4 text-accent">
        {label}
      </Button>
      {state === "done" ? (
        <p role="status" className="text-caption text-text-2">
          {market ? `Pembaruan data pasar berjalan; hasilnya muncul sekitar ${settle} detik lagi.` : `Pembaruan ${ticker} berjalan; hasilnya muncul sekitar ${settle} detik lagi.`}
        </p>
      ) : null}
      {error ? (
        <p role="alert" className="text-caption text-status-over">
          {error}
        </p>
      ) : null}
    </div>
  );
}
