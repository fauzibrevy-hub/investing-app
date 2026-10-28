import { useMemo } from "react";
import { useFinancials, usePack } from "../../research/api";
import { dataGaps, hasGaps } from "../dataGaps";
import { RefreshButton } from "../RefreshButton";
import { Block, LoadError } from "../ui";

const KPI_SHOWN = 8;

/**
 * Blok 6 "Data belum ada": daftar polos celah yang diketahui (KPI belum diisi, pendorong tanpa sumber atau belum diambil,
 * celah XBRL dengan alasannya, kuartal tanpa laporan). Disembunyikan bila tidak ada celah. Pendorong yang belum
 * diambil membawa **Perbarui data**.
 */
export function DataBelumAda({ ticker }: { ticker: string }) {
  const pack = usePack(ticker);
  const taxonomy = useFinancials(ticker, "standalone", 12).data?.taxonomy ?? null;
  const gaps = useMemo(() => (pack.data ? dataGaps(pack.data, taxonomy) : null), [pack.data, taxonomy]);
  if (pack.status === "error" && !pack.data) {
    return (
      <Block id="kosong" title="Data belum ada">
        <LoadError message={pack.error?.message ?? "Gagal memuat paket"} onRetry={pack.reload} />
      </Block>
    );
  }
  if (!gaps || !hasGaps(gaps)) return null;
  const fetchable = gaps.drivers.some((d) => d.fetchable);
  return (
    <Block id="kosong" title="Data belum ada">
      <ul className="flex flex-col gap-2 text-label text-text">
        {gaps.kpi.length ? (
          <li>
            <span className="font-semibold">KPI belum diisi:</span>{" "}
            {gaps.kpi.slice(0, KPI_SHOWN).join(", ")}
            {gaps.kpi.length > KPI_SHOWN ? ` dan ${gaps.kpi.length - KPI_SHOWN} lainnya` : ""}. <span className="text-text-2">Isi lewat tabel Kuartal terakhir.</span>
          </li>
        ) : null}
        {gaps.drivers.map((d) => (
          <li key={d.id}>
            <span className="font-semibold">{d.label}:</span> {d.text}
          </li>
        ))}
        {gaps.quarters.map((q) => (
          <li key={q}>{q}</li>
        ))}
        {gaps.xbrl.map((x) => (
          <li key={x} className="text-text-2">
            {x}
          </li>
        ))}
      </ul>
      {fetchable ? <RefreshButton scope="market" /> : null}
    </Block>
  );
}
