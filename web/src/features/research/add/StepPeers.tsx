import { useState } from "react";
import { Button } from "../../../components/Button";
import { FormField } from "../../../components/FormField";
import { useLookup } from "../api";
import { useDebounced } from "./useDebounced";

export const MAX_PEERS = 3;

export interface StepPeersProps {
  /** Emiten yang sedang ditambahkan; tidak boleh jadi pembanding sendiri. */
  ticker: string;
  peers: readonly string[];
  onChange: (peers: string[]) => void;
}

/** Langkah 2: sampai tiga pembanding, dicari dari daftar bursa menurut kode atau nama. */
export function StepPeers({ ticker, peers, onChange }: StepPeersProps) {
  const [q, setQ] = useState("");
  const settled = useDebounced(q.trim());
  const lookup = useLookup(settled || null, 8);
  const full = peers.length >= MAX_PEERS;
  const results = (lookup.data && lookup.data.query === settled.toUpperCase() ? lookup.data.items : []).filter((i) => i.ticker !== ticker);

  return (
    <div className="flex flex-col gap-4">
      <p className="text-label text-text-2">{`Pembanding bersifat opsional; paling banyak ${MAX_PEERS}. Dipakai di tabel Bandingkan.`}</p>
      {peers.length ? (
        <ul aria-label="Pembanding terpilih" className="flex flex-wrap gap-2">
          {peers.map((p) => (
            <li key={p} className="flex items-center gap-1 rounded-pill bg-accent-soft pl-3 text-label font-bold text-accent">
              {p}
              <Button variant="icon" aria-label={`Hapus ${p} dari pembanding`} onClick={() => onChange(peers.filter((x) => x !== p))}>
                ×
              </Button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-label text-text-2">Belum ada pembanding</p>
      )}
      <FormField label="Cari pembanding" hint="Kode atau nama perusahaan" type="search" value={q} autoComplete="off" onChange={(e) => setQ(e.target.value)} />
      {full ? <p role="status" className="text-label text-text-2">{`Sudah ${MAX_PEERS} pembanding; hapus satu untuk menambah yang lain.`}</p> : null}
      <div role="status" aria-live="polite">
        {settled && lookup.status === "loading" ? <p className="text-label text-text-2">Mencari…</p> : null}
        {settled && lookup.status === "error" ? <p className="text-label text-status-over">Gagal mencari</p> : null}
        {settled && lookup.data && lookup.data.query === settled.toUpperCase() && !results.length ? <p className="text-label text-text-2">Tidak ada hasil</p> : null}
      </div>
      {results.length ? (
        <ul aria-label="Hasil pencarian" className="flex flex-col">
          {results.map((r) => {
            const chosen = peers.includes(r.ticker);
            return (
              <li key={r.ticker} className="flex min-h-11 items-center justify-between gap-3 border-b border-border-hairline last:border-b-0">
                <span className="flex min-w-0 flex-col">
                  <span className="text-label font-bold text-text">{r.ticker}</span>
                  <span className="truncate text-caption text-text-2">{r.name ?? "Tanpa nama"}</span>
                </span>
                <Button variant="ghost" disabled={chosen || full} aria-label={chosen ? `${r.ticker} sudah dipilih` : `Tambah ${r.ticker} sebagai pembanding`} onClick={() => onChange([...peers, r.ticker])}>
                  {chosen ? "Dipilih" : "Tambah"}
                </Button>
              </li>
            );
          })}
        </ul>
      ) : null}
    </div>
  );
}
