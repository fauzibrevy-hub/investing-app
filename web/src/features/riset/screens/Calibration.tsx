import { Link, useSearchParams } from "react-router-dom";
import { formatPercent, formatRupiah } from "../../../lib/money";
import { BackHeader } from "../../../shell/BackHeader";
import { useCalibration, useUniverse } from "../../research/api";
import { ScreenFooter } from "../../research/desk/DeskFooter";
import { formatDate } from "../../research/format";
import type { CalibrationKind, CalibrationRow } from "../../research/types";
import { Empty, FIELD, LoadError, Loading, PAGE } from "../ui";

type KindFilter = "semua" | CalibrationKind;

const KIND_OPTIONS: Array<{ value: KindFilter; label: string }> = [
  { value: "semua", label: "Semua jenis" },
  { value: "klaim", label: "Klaim" },
  { value: "level", label: "Level" },
  { value: "keputusan", label: "Keputusan" },
];

/** Baris 2 kartu: `Rp1.530 → Rp1.710 (+11,8%)`, atau kapan hasilnya matang, atau alasan harga kosong. */
export function outcomeText(row: CalibrationRow): { text: string; gap: boolean } {
  if (!row.matured) return { text: row.matures_on ? `Matang ${formatDate(row.matures_on)}` : "Belum matang", gap: false };
  if (row.price_then === null || row.price_now === null || row.return_pct === null) return { text: "Harga belum lengkap, hasil belum dihitung", gap: true };
  return { text: `${formatRupiah(row.price_then)} → ${formatRupiah(row.price_now)} (${formatPercent(row.return_pct, { signed: true })})`, gap: false };
}

function Card({ row }: { row: CalibrationRow }) {
  const outcome = outcomeText(row);
  return (
    <li data-calibration={row.id} className="flex min-w-0 flex-col gap-1 rounded-md border border-border-hairline p-3">
      <span data-line="1" className="flex min-w-0 flex-wrap items-baseline gap-x-2 text-caption text-text-2">
        <span className="num">{formatDate(row.date)}</span>
        <span className="text-table font-bold text-text">{row.ticker}</span>
        <span>{row.kind_label}</span>
        <span className="ml-auto font-semibold text-text">{row.status ?? "Belum dinilai"}</span>
      </span>
      <span data-line="2" className="break-words text-body text-text">
        {row.text}
      </span>
      <span className="num text-label text-text-2">
        {outcome.text}
        {outcome.gap ? (
          <Link to={`/perusahaan/${row.ticker}?section=harga&from=alat`} aria-label={`Lihat harga ${row.ticker}`} className="ml-2 inline-flex min-h-11 items-center font-bold text-accent">
            Lihat harga
          </Link>
        ) : null}
      </span>
      {row.reason ? <span className="text-caption text-text-2">{row.reason}</span> : null}
    </li>
  );
}

/**
 * Kalibrasi `#/alat/kalibrasi` (ui-slicing.md §4): tingkat Berlaku vs Patah per ticker sebagai teks di atas
 * (tanpa warna), lalu butir sebagai kartu dua baris; dua pilihan, ticker dan jenis. Data: `GET /calibration`.
 */
export function Calibration() {
  const [params, setParams] = useSearchParams();
  const ticker = params.get("ticker")?.toUpperCase() || undefined;
  const typeParam = params.get("type");
  const kind: KindFilter = KIND_OPTIONS.some((o) => o.value === typeParam) ? (typeParam as KindFilter) : "semua";
  const cal = useCalibration(ticker, kind === "semua" ? undefined : kind);
  const universe = useUniverse();
  const data = cal.data;
  const set = (key: string, value: string | undefined) => {
    const p = new URLSearchParams(params);
    if (value) p.set(key, value);
    else p.delete(key);
    setParams(p);
  };
  const tickers = [...new Set([...(universe.data?.items.map((i) => i.ticker) ?? []), ...(ticker ? [ticker] : [])])].sort();

  let body;
  if (cal.status === "error" && !data) body = <LoadError message="Gagal memuat kalibrasi" onRetry={cal.reload} />;
  else if (!data) body = <Loading label="Memuat kalibrasi…" rows={3} height={96} />;
  else if (data.items.length === 0)
    body = (
      <Empty action={<Link to="/perusahaan" className="inline-flex min-h-11 items-center text-label font-bold text-accent">Ke Perusahaan</Link>}>
        Belum ada klaim, level, atau keputusan untuk dikalibrasi. Tulis klaim di halaman perusahaan, atau catat level dan keputusan di jurnalnya.
      </Empty>
    );
  else
    body = (
      <ul aria-label="Butir kalibrasi" className="flex flex-col gap-2">
        {data.items.map((row) => (
          <Card key={row.id} row={row} />
        ))}
      </ul>
    );

  return (
    <div className={PAGE}>
      <header className="flex flex-col gap-1">
        <BackHeader />
        <h1 className="text-title text-text">Kalibrasi</h1>
      </header>
      {data ? (
        <section aria-label="Tingkat klaim" className="flex flex-col gap-1 text-label text-text">
          <h2 className="text-body-strong text-text">Klaim: Berlaku vs Patah per ticker</h2>
          {data.hit_rates.length === 0 ? (
            <p className="text-text-2">Belum ada klaim yang bisa dihitung.</p>
          ) : (
            <ul>
              {data.hit_rates.map((h) => (
                <li key={h.ticker}>{h.text}</li>
              ))}
            </ul>
          )}
          <p className="text-caption text-text-2">{`Hasil dihitung setelah ${data.maturity_days} hari sejak dicatat: ${data.matured} matang, ${data.pending} belum.`}</p>
        </section>
      ) : null}
      <div className="grid grid-cols-2 gap-3">
        <label className="flex min-w-0 flex-col gap-1 text-label font-semibold text-text">
          Ticker
          <select value={ticker ?? ""} onChange={(e) => set("ticker", e.target.value || undefined)} className={FIELD}>
            <option value="">Semua</option>
            {tickers.map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
        </label>
        <label className="flex min-w-0 flex-col gap-1 text-label font-semibold text-text">
          Jenis
          <select value={kind} onChange={(e) => set("type", e.target.value === "semua" ? undefined : e.target.value)} className={FIELD}>
            {KIND_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </label>
      </div>
      {cal.error && data ? <LoadError message="Gagal memuat ulang kalibrasi" onRetry={cal.reload} /> : null}
      <p role="status" className="sr-only">
        {data ? `${data.items.length} butir kalibrasi` : ""}
      </p>
      {body}
      <ScreenFooter asOf={data?.as_of} />
    </div>
  );
}
