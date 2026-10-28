import { useState } from "react";
import { Link } from "react-router-dom";
import { ChevronRight, Plus } from "lucide-react";
import { useDesk } from "../research/api";
import { useSheetTrigger } from "../research/components/useSheetTrigger";
import { DeskFooter } from "../research/desk/DeskFooter";
import type { DeskResponse } from "../research/types";
import { Block, LoadError, Loading, PAGE } from "../riset/ui";
import { useThesisDrafts, useThesisSummary } from "../thesis/api";
import { VaultDraftsSheet } from "../thesis/VaultDraftsSheet";
import { CoverageList } from "./CoverageList";
import { tesisLainRows } from "./tesisLain";

/** Blok 5 (design.md §3.3, prd.md O1): lipat, tersembunyi bila tidak ada ticker maupun draf vault. */
function TesisLain({ desk }: { desk: DeskResponse }) {
  const summary = useThesisSummary();
  const drafts = useThesisDrafts();
  const [open, setOpen] = useState(false);
  // Safari iOS tidak memfokuskan tombol yang diketuk: pemicu dicatat supaya fokus kembali ke baris ini (design.md §6).
  const vaultSheet = useSheetTrigger();
  // Satu sumber gagal tidak mengosongkan yang lain (prd.md §7): tampil dari yang berhasil, kegagalan disebut.
  const failed = (summary.status === "error" && !summary.data) || (drafts.status === "error" && !drafts.data);
  const retry = () => {
    if (summary.status === "error") summary.reload();
    if (drafts.status === "error") drafts.reload();
  };
  const settled = summary.status !== "loading" && drafts.status !== "loading";
  const rows = tesisLainRows(desk.coverage, summary.data, drafts.data);
  const vault = drafts.data?.vault_drafts.length ?? 0;
  return (
    <>
      {settled && (rows.length > 0 || vault > 0) ? (
        <Block id="tesis-lain" title={`Tesis lain (${rows.length})`} open={open} onToggle={() => setOpen((v) => !v)}>
          <ul aria-label="Tesis lain">
            {rows.map((r) => (
              <li key={r.ticker} data-thesis={r.ticker} className="border-b border-border-hairline last:border-b-0">
                {/* Halaman perusahaan hanya-tesis; Perusahaan tetap pemiliknya (design.md §2.4 aturan 4). */}
                <Link to={`/perusahaan/${r.ticker}?from=perusahaan`} className="flex min-h-14 min-w-0 items-center gap-2 py-2 text-table active:bg-surface-sunken">
                  <span className="shrink-0 text-body-strong text-text">{r.ticker}</span>{" "}
                  <span className="min-w-0 truncate text-text-2">{r.kind}</span>{" "}
                  {r.updated ? <span className="ml-auto shrink-0 whitespace-nowrap text-caption text-text-2">{`diperbarui ${r.updated}`}</span> : null}
                </Link>
              </li>
            ))}
            {vault > 0 ? (
              <li className="border-b border-border-hairline last:border-b-0">
                <button type="button" aria-haspopup="dialog" onClick={(e) => vaultSheet.open(true, e.currentTarget)} className="flex min-h-14 w-full min-w-0 items-center justify-between gap-2 py-2 text-left text-table active:bg-surface-sunken">
                  <span className="min-w-0 truncate text-body-strong text-text">{`Draf vault (${vault})`}</span>
                  <ChevronRight aria-hidden="true" size={18} strokeWidth={1.75} className="shrink-0 text-text-2" />
                </button>
              </li>
            ) : null}
          </ul>
        </Block>
      ) : null}
      {failed ? <LoadError message="Gagal memuat tesis lain" onRetry={retry} /> : null}
      <VaultDraftsSheet open={vaultSheet.value !== null} onClose={vaultSheet.close} />
    </>
  );
}

/**
 * Perusahaan `#/perusahaan` (design.md §3.3): header dengan **Tambah**, lalu daftar cakupan (`CoverageList`: Dipegang,
 * Manual, Pembanding), Tesis lain, footer. Cakupan dari `GET /api/research/desk`; Tesis lain dari
 * `GET /api/thesis/summary` dan `GET /api/thesis/drafts`, dan tidak menahan daftar cakupan.
 */
export function Perusahaan() {
  const desk = useDesk();
  const data = desk.data;
  let body;
  if (desk.status === "error" && !data) body = <LoadError message="Gagal memuat Perusahaan" onRetry={desk.reload} />;
  else if (!data) body = <Loading label="Memuat Perusahaan…" rows={3} height={64} />;
  else
    body = (
      <>
        {desk.error ? <LoadError message="Gagal memuat ulang Perusahaan" onRetry={desk.reload} /> : null}
        <CoverageList desk={data} />
        <TesisLain desk={data} />
      </>
    );
  return (
    <div className={PAGE}>
      <header className="flex min-h-11 items-center justify-between gap-3">
        <h1 className="text-title text-text">Perusahaan</h1>
        <Link to="/perusahaan/tambah" className="inline-flex min-h-11 min-w-11 items-center gap-1.5 rounded-sm px-3 text-label font-bold text-accent active:opacity-85">
          <Plus aria-hidden="true" size={18} strokeWidth={1.75} />
          Tambah
        </Link>
      </header>
      {body}
      <DeskFooter ages={data?.ages} />
    </div>
  );
}

export default Perusahaan;
