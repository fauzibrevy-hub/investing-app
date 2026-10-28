import type { LookupItem } from "../types";

export interface SummaryData {
  ticker: string;
  item: LookupItem | null;
  /** Label paket yang berlaku, `null` bila tanpa paket. */
  packLabel: string | null;
  /** `usulan` dari kode industri, `pilihan` bila dipilih sendiri. */
  packSource: "usulan" | "pilihan" | "tanpa";
  peers: readonly string[];
  note: string;
}

/** Langkah 4: ringkasan isian sebelum **Mulai**. */
export function StepSummary({ data }: { data: SummaryData }) {
  const { item } = data;
  const pack = data.packLabel ? `${data.packLabel} (${data.packSource === "pilihan" ? "pilihan sendiri" : "dari kode industri"})` : "Tanpa paket dulu";
  const rows: Array<[string, string]> = [
    ["Emiten", item?.name ? `${data.ticker} · ${item.name}` : data.ticker],
    ["Kode industri", item?.industry_code ?? "Belum ada laporan"],
    ["Paket", pack],
    ["Pembanding", data.peers.length ? data.peers.join(", ") : "Tidak ada"],
    ["Bisnis", data.note.trim() || "Belum diisi"],
  ];
  return (
    <div className="flex flex-col gap-4">
      <dl className="grid grid-cols-[120px_minmax(0,1fr)] gap-x-3 gap-y-2 rounded-md bg-surface-sunken p-4 text-label">
        {rows.map(([k, v]) => (
          <div key={k} className="contents">
            <dt className="text-text-2">{k}</dt>
            <dd className="break-words text-text">{v}</dd>
          </div>
        ))}
      </dl>
      <p className="text-label text-text-2">Mulai menambahkan emiten ke Riset, mengambil riwayat harga, memasukkannya ke daftar pantau scraper, lalu membuka dossier. Emiten bisa dipakai sebelum ada KPI; papan cakupan menunjukkan kuartal KPI yang masih kosong.</p>
    </div>
  );
}
