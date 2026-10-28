import { Plus } from "lucide-react";
import { Button } from "../../components/Button";
import { Sheet } from "../../components/Sheet";
import { useThesisDetail, useThesisDrafts } from "./api";
import { useSheetTrigger } from "../research/components/useSheetTrigger";
import { Block, Empty, LoadError, Loading } from "../riset/ui";
import { DossierView } from "./DossierView";
import { DraftDetail } from "./DraftDetail";
import { DraftEditorSheet } from "./DraftEditor";
import { shortDate } from "./format";

const LOAD_ERROR = "Gagal memuat tesis";

function TesisBody({ ticker }: { ticker: string }) {
  const dossier = useThesisDetail(ticker);
  const drafts = useThesisDrafts();
  const editor = useSheetTrigger();
  const detail = useSheetTrigger<string>();

  // Tidak ada dossier di vault (404) adalah keadaan kosong, bukan galat.
  const noDossier = dossier.status === "error" && dossier.error?.kind === "not_found";
  const dossierFailed = dossier.status === "error" && !noDossier;
  const mine = (drafts.data?.drafts ?? []).filter((d) => d.ticker.toUpperCase() === ticker);
  const write = (
    <Button variant="sunken" icon={<Plus />} onClick={(e) => editor.open(true, e.currentTarget)}>
      Tulis draf tesis
    </Button>
  );

  let body;
  if (dossier.status === "loading" || drafts.status === "loading") body = <Loading label="Memuat tesis…" rows={2} height={92} />;
  // Satu sumber gagal tidak mengosongkan yang lain (finance-app `ThesisScreen.tsx`): galat penuh hanya bila keduanya gagal.
  else if (dossierFailed && drafts.status === "error") body = <LoadError message={LOAD_ERROR} onRetry={() => (dossier.reload(), drafts.reload())} />;
  else if (noDossier && drafts.status === "ready" && mine.length === 0) body = <Empty action={write}>{`Belum ada tesis untuk ${ticker}`}</Empty>;
  else
    body = (
      <>
        {dossier.data ? <DossierView detail={dossier.data} /> : dossierFailed ? <LoadError message={LOAD_ERROR} onRetry={dossier.reload} /> : null}
        {drafts.status === "error" ? (
          <LoadError message={LOAD_ERROR} onRetry={drafts.reload} />
        ) : mine.length ? (
          <section aria-labelledby={`tesis-draf-${ticker}`} className="flex flex-col gap-1">
            <h3 id={`tesis-draf-${ticker}`} className="flex min-h-11 items-center text-title text-text">{`Draf (${mine.length})`}</h3>
            <ul aria-label={`Draf tesis ${ticker}`}>
              {mine.map((d) => (
                <li key={d.id} className="border-b border-border-hairline last:border-b-0">
                  <button type="button" onClick={(e) => detail.open(d.id, e.currentTarget)} className="flex min-h-16 w-full min-w-0 flex-col justify-center gap-0.5 py-2 text-left active:bg-surface-sunken">
                    <span className="line-clamp-2 text-body-strong text-text">{d.title || d.thesis_statement}</span>
                    <span className="truncate text-caption text-text-2">{[d.thesis_type, d.story_status, shortDate(d.updated_at)].filter(Boolean).join(" · ")}</span>
                  </button>
                </li>
              ))}
            </ul>
          </section>
        ) : null}
        <div>{write}</div>
      </>
    );

  return (
    <>
      {body}
      <Sheet open={detail.value !== null} title="Draf tesis" onClose={detail.close}>
        {detail.value !== null ? <DraftDetail id={detail.value} onDeleted={detail.close} /> : null}
      </Sheet>
      {editor.value !== null ? <DraftEditorSheet ticker={ticker} onClose={editor.close} /> : null}
    </>
  );
}

/**
 * Blok "Tesis" (design.md §3.4, plan.md §3.3 block id `tesis`), antara Jurnal dan Harga, lipat: dossier vault baca saja
 * (`GET /api/thesis/{T}`), draf ticker ini (`GET /api/thesis/drafts`, detail + **Hapus draf**), dan **Tulis draf tesis**
 * (`POST /api/thesis/drafts`). Ini tesis tulisan pemilik, terlipat di bawah fakta; header tetap tanpa baris tesis
 * (ui-slicing.md §8 D2). Tanpa `onToggle` blok selalu terbuka (keadaan hanya-tesis, prd.md O1).
 */
export function ThesisBlock({ ticker, open, onToggle }: { ticker: string; open: boolean; onToggle?: () => void }) {
  return (
    <Block id="tesis" title="Tesis" open={open} onToggle={onToggle}>
      <TesisBody ticker={ticker} />
    </Block>
  );
}
