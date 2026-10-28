import { Button } from "../../../components/Button";
import { Sheet } from "../../../components/Sheet";
import { useClaims, useFinancials } from "../../research/api";
import { ClaimCard, sortClaims } from "../../research/components/ClaimCard";
import { ClaimEditor } from "../../research/components/ClaimEditor";
import { ClaimEvaluations } from "../../research/components/ClaimEvaluations";
import { ClaimRow } from "../../research/components/ClaimRow";
import { ClaimCountsLine } from "../../research/components/claimStatus";
import { useSheetTrigger } from "../../research/dossier/mobile/useSheetTrigger";
import type { DossierResponse } from "../../research/types";
import { Block, Empty, LoadError, Loading } from "../ui";

/**
 * Blok 5 "Klaim": daftar `ClaimRow` (Patah, Belum teruji, Berlaku, Kedaluwarsa); ketuk membuka kartu dengan evaluasi,
 * bukti, dan **Tutup klaim**; **Tambah klaim** membuka `ClaimEditor` di sheet. Klaim ditulis lewat `POST /{ticker}/claims`.
 */
export function Klaim({ ticker, dossier }: { ticker: string; dossier: DossierResponse }) {
  const claims = useClaims(ticker);
  const currency = useFinancials(ticker, "standalone", 12).data?.currency ?? null;
  const detail = useSheetTrigger<string>();
  const add = useSheetTrigger();
  const items = claims.data ? sortClaims(claims.data.items) : [];
  const selected = items.find((c) => c.id === detail.value) ?? null;
  const addButton = (
    <Button variant="ghost" onClick={(e) => add.open(true, e.currentTarget)}>
      Tambah klaim
    </Button>
  );

  let body;
  if (claims.status === "error") body = <LoadError message={claims.error?.message ?? "Gagal memuat klaim"} onRetry={claims.reload} />;
  else if (claims.status === "loading" || !claims.data) body = <Loading label="Memuat klaim…" rows={2} height={88} />;
  else if (items.length === 0)
    body = (
      <Empty
        action={
          <Button variant="sunken" onClick={(e) => add.open(true, e.currentTarget)}>
            Tambah klaim
          </Button>
        }
      >
        Belum ada klaim. Tulis apa yang Anda yakini tentang {ticker} dengan angka dan tanggal, lalu lihat apakah laporan berikutnya membuktikannya.
      </Empty>
    );
  else
    body = (
      <>
        <ClaimCountsLine counts={claims.data.counts} hideZero />
        <ul aria-label="Daftar klaim" className="flex flex-col gap-2">
          {items.map((c) => (
            <li key={c.id}>
              <ClaimRow claim={c} currency={currency} onOpen={(claim, trigger) => detail.open(claim.id, trigger)} />
            </li>
          ))}
        </ul>
      </>
    );

  return (
    <Block id="klaim" title="Klaim" action={items.length ? addButton : undefined}>
      {body}
      <Sheet open={selected !== null} title="Klaim" onClose={detail.close}>
        {selected ? (
          <div className="flex flex-col gap-4">
            <ClaimCard claim={selected} currency={currency} defaultEvidenceOpen sourceLink={false} />
            <ClaimEvaluations claim={selected} currency={currency} />
          </div>
        ) : null}
      </Sheet>
      <Sheet open={add.value !== null} title="Klaim baru" onClose={add.close}>
        <ClaimEditor
          ticker={ticker}
          metrics={claims.data?.metrics}
          metricsError={claims.status === "error" ? { message: claims.error?.message ?? "Gagal memuat daftar metrik", retry: claims.reload } : null}
          packLabel={dossier.pack.label}
          currency={currency}
          onDone={add.close}
        />
      </Sheet>
    </Block>
  );
}
