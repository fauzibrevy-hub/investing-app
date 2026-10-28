import { Button } from "../../../components/Button";
import { Skeleton } from "../../../components/Skeleton";
import { cn } from "../../../lib/cn";
import { useClaimEvals } from "../api";
import { formatMetricValue, unitWord } from "./ClaimCard";
import { ClaimStatusMark, STATUS_WORDS } from "./claimStatus";
import { formatTimestamp } from "./JournalEntry";
import type { Claim } from "../types";

export interface ClaimEvaluationsProps {
  claim: Claim;
  currency?: string | null;
  className?: string;
}

/**
 * Riwayat evaluasi satu klaim (`GET /claims/{id}/evals`), terbaru dulu: waktu, titik + kata status, nilai yang dipakai.
 * Hanya baris yang berubah yang tersimpan (nilai atau status), jadi daftar ini adalah apa yang dilakukan data.
 * Isi `inputs` bentuk bebas (TBD di types.ts) tidak ditampilkan.
 */
export function ClaimEvaluations({ claim, currency = null, className }: ClaimEvaluationsProps) {
  const evals = useClaimEvals(claim.id);
  const items = evals.data ? [...evals.data.items].sort((a, b) => Date.parse(b.evaluated_at) - Date.parse(a.evaluated_at)) : [];
  return (
    <section aria-label="Evaluasi klaim" className={cn("flex flex-col gap-2", className)}>
      <h3 className="text-body-strong text-text">Evaluasi</h3>
      {evals.status === "loading" ? (
        <div role="status" aria-busy="true" className="flex flex-col gap-2">
          <span className="sr-only">Memuat evaluasi…</span>
          <Skeleton height={44} />
        </div>
      ) : null}
      {evals.status === "error" ? (
        <div role="alert" className="flex flex-col items-start gap-1">
          <p className="text-label font-semibold text-status-over">{evals.error?.message ?? "Gagal memuat evaluasi"}</p>
          <Button variant="ghost" onClick={evals.reload}>
            Coba lagi
          </Button>
        </div>
      ) : null}
      {evals.status === "ready" && items.length === 0 ? <p className="text-label text-text-2">Belum dievaluasi</p> : null}
      {items.length ? (
        <ol className="flex flex-col">
          {items.map((e) => (
            <li key={e.evaluated_at} className="flex min-h-11 items-center justify-between gap-3 border-b border-border-hairline py-1.5 last:border-b-0">
              <span className="flex min-w-0 flex-col">
                <ClaimStatusMark status={e.status}>{e.label ?? STATUS_WORDS[e.status]}</ClaimStatusMark>
                <span className="text-caption text-text-2">{formatTimestamp(e.evaluated_at)}</span>
              </span>
              <span className="num shrink-0 text-right text-label text-text">
                {e.value === null ? "—" : `${formatMetricValue(e.value, claim.unit)}${claim.unit === "ccy" ? ` ${unitWord("ccy", currency)}` : ""}`}
              </span>
            </li>
          ))}
        </ol>
      ) : null}
    </section>
  );
}
