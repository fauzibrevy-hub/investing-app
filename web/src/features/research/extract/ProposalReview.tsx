import { useEffect, useId, useRef, useState } from "react";
import { Button } from "../../../components/Button";
import { Card } from "../../../components/Card";
import { Skeleton } from "../../../components/Skeleton";
import { ApiError } from "../../../lib/api";
import { cn } from "../../../lib/cn";
import { invalidate } from "../../../lib/useResource";
import { BASE, acceptProposal, rejectProposal, useFinancials, useProposals } from "../api";
import { useDraftKey } from "../components/draftKey";
import { formatMetric, metricScale, metricUnitHeader } from "../components/metricFormat";
import { ProvenanceBadge } from "../components/ProvenanceBadge";
import { formatDate, formatExact, periodLabel, type Scale } from "../format";
import type { ExtractProposal, ProposalDocument } from "../types";

export const ALL_DOCUMENTS = "";

/** Nilai yang tampil: skala baris untuk `ccy`, format metrik lain; tanpa satuan (satuan ada di judul kelompok). */
const valueText = (p: ExtractProposal, scale: Scale | null) => formatMetric(p.value, p.unit, scale);

const locatorWord = (p: Pick<ExtractProposal, "locator">) => (p.locator === "slide" ? "slide" : "hlm.");

const docLabel = (d: ProposalDocument) => `${d.kind_label}${d.doc_date ? ` ${formatDate(d.doc_date)}` : ""}${d.format ? ` · ${d.format.toUpperCase()}` : ""}`;

/** Pesan 409 backend (Inggris) → kalimat UI; `held` terisi bila sel sudah berisi nilai pemilik. */
export function conflictMessage(err: ApiError): { held: string | null; text: string } {
  const m = /already holds (.+?) \((.*)\); send/.exec(err.message);
  if (m) {
    const held = m[1] === "unknown" ? "Tidak diketahui" : m[1];
    return { held, text: `Sel ini sudah berisi ${held} (${m[2]}).` };
  }
  if (/already (accepted|rejected|superseded)/.test(err.message)) return { held: null, text: "Usulan ini sudah diputuskan; daftar dimuat ulang." };
  if (/no longer in the pack/.test(err.message)) return { held: null, text: "KPI ini tidak lagi ada di paket emiten." };
  if (/unit for this KPI changed/.test(err.message)) return { held: null, text: "Satuan KPI di paket berubah sejak berkas dibaca; unggah ulang berkasnya." };
  return { held: null, text: err.message };
}

/** Kata status untuk badge usulan; `pending` tidak punya kata (hanya tombol). */
const STATUS_WORD: Partial<Record<ExtractProposal["status"], string>> = { rejected: "ditolak", superseded: "digantikan" };

interface RowProps {
  proposal: ExtractProposal;
  scale: Scale | null;
  doc: ProposalDocument | undefined;
  showDoc: boolean;
}

/** Satu usulan: nilai, halaman, kutipan, dan **Terima** atau **Tolak**. Mengganti nilai pemilik meminta konfirmasi. */
export function ProposalRow({ proposal: p, scale, doc, showDoc }: RowProps) {
  const key = useDraftKey();
  const [busy, setBusy] = useState<"accept" | "reject" | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [announce, setAnnounce] = useState("");
  const acceptButton = useRef<HTMLButtonElement>(null);
  const confirmButton = useRef<HTMLButtonElement>(null);
  const restore = useRef(false);
  const text = valueText(p, scale);
  const period = periodLabel(`${p.fy}Q${p.q}`);
  const name = `${p.metric_label} ${period} ${text} ${locatorWord(p)} ${p.page}`;

  useEffect(() => {
    if (confirm !== null) confirmButton.current?.focus();
    else if (restore.current) {
      restore.current = false;
      acceptButton.current?.focus();
    }
  }, [confirm]);

  async function accept(replace: boolean) {
    setBusy("accept");
    setError(null);
    try {
      const res = await acceptProposal(p.id, replace, { idempotencyKey: key.forBody({ id: p.id, replace }) });
      key.reset();
      setConfirm(null);
      setAnnounce(res.replaced ? "Nilai pemilik diganti; tersimpan di Paket" : "Diterima; tersimpan di Paket");
    } catch (err) {
      if (err instanceof ApiError && err.status === 409) {
        const c = conflictMessage(err);
        if (c.held && !replace) setConfirm(c.text);
        else {
          setConfirm(null);
          setError(c.text);
          // Keputusan lain sudah menang (usulan sudah diputuskan, KPI tidak lagi di paket): daftar dan Paket dibaca ulang.
          invalidate(BASE);
        }
      } else if (err instanceof ApiError && (err.kind === "network" || err.kind === "timeout")) setError("Tidak ada koneksi, belum tersimpan. Coba lagi.");
      else setError(err instanceof Error && err.message ? err.message : "Gagal menerima usulan");
    } finally {
      setBusy(null);
    }
  }

  async function reject() {
    setBusy("reject");
    setError(null);
    try {
      await rejectProposal(p.id, { idempotencyKey: key.forBody({ id: p.id, reject: true }) });
      key.reset();
      setAnnounce("Ditolak; tidak ada yang tersimpan");
    } catch (err) {
      if (err instanceof ApiError && err.status === 409) invalidate(BASE);
      setError(err instanceof ApiError && err.status === 409 ? conflictMessage(err).text : err instanceof Error && err.message ? err.message : "Gagal menolak usulan");
    } finally {
      setBusy(null);
    }
  }

  const pending = p.status === "pending";
  return (
    <li data-proposal={p.id} data-status={p.status} className={cn("flex flex-col gap-2 py-3", (p.status === "rejected" || p.status === "superseded") && "opacity-70")}>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <span className="inline-flex items-center gap-2">
          <span title={`Nilai persis ${formatExact(p.value)}`} className="num text-body-strong text-text">
            {text}
          </span>
          {p.status === "accepted" ? <ProvenanceBadge kind="manual" text="diterima" title="Diterima pemilik; tersimpan sebagai KPI manual dengan dokumen ini sebagai asal" /> : <ProvenanceBadge kind="usulan" text={STATUS_WORD[p.status]} />}
        </span>
        <span className="text-label text-text-2">{`${locatorWord(p)} ${p.page}`}</span>
        {showDoc && doc ? <span className="text-label text-text-2">{docLabel(doc)}</span> : null}
      </div>
      <q className="text-label text-text-2">{p.snippet}</q>
      {pending ? (
        <div className="-ml-3 flex flex-wrap items-center gap-1">
          <Button ref={acceptButton} variant="ghost" loading={busy === "accept" && confirm === null} disabled={busy !== null || confirm !== null} onClick={() => accept(false)} aria-label={`Terima ${name}`}>
            Terima
          </Button>
          <Button variant="ghost" tone="danger" loading={busy === "reject"} disabled={busy !== null || confirm !== null} onClick={reject} aria-label={`Tolak ${name}`}>
            Tolak
          </Button>
        </div>
      ) : null}
      {confirm !== null ? (
        <div role="group" aria-label={`Konfirmasi ganti nilai ${p.metric_label} ${period}`} className="flex flex-col gap-1 rounded-sm bg-surface-sunken p-3">
          <p className="text-label text-text">{`${confirm} Ganti dengan ${text} dari dokumen ini?`}</p>
          <div className="-ml-3 flex gap-1">
            <Button ref={confirmButton} variant="ghost" loading={busy === "accept"} onClick={() => accept(true)} aria-label={`Ganti dengan ${name}`}>
              Ganti
            </Button>
            <Button
              variant="ghost"
              disabled={busy !== null}
              onClick={() => {
                restore.current = true;
                setConfirm(null);
              }}
            >
              Batal
            </Button>
          </div>
        </div>
      ) : null}
      {error ? (
        <p role="alert" className="text-caption text-status-over">
          {error}
        </p>
      ) : null}
      <p role="status" aria-live="polite" className="text-caption text-text-2">
        {announce}
      </p>
    </li>
  );
}

interface GroupProps {
  items: ExtractProposal[];
  docs: Map<string, ProposalDocument>;
  showDoc: boolean;
  currency: string | null;
}

function KpiGroup({ items, docs, showDoc, currency }: GroupProps) {
  const titleId = useId();
  const first = items[0];
  const scale = metricScale(first.unit, items.map((i) => i.value), currency);
  const accepted = items.find((i) => i.status === "accepted");
  return (
    <section aria-labelledby={titleId} data-kpi={first.kpi_key} className="rounded-md border border-border-hairline px-4 py-3">
      <h4 id={titleId} className="flex flex-wrap items-baseline gap-x-2 text-body-strong text-text">
        {first.metric_label}
        <span className="text-label font-normal text-text-2">{`${periodLabel(`${first.fy}Q${first.q}`)} ${metricUnitHeader(first.unit, currency, scale)}`}</span>
      </h4>
      {accepted && items.some((i) => i.status === "pending") ? <p className="text-caption text-text-2">Satu nilai sudah diterima; menerima yang lain meminta konfirmasi untuk menggantinya.</p> : null}
      <ul className="divide-y divide-border-hairline">
        {items.map((p) => (
          <ProposalRow key={p.id} proposal={p} scale={scale} doc={docs.get(p.document_id)} showDoc={showDoc} />
        ))}
      </ul>
    </section>
  );
}

export interface ProposalReviewProps {
  ticker: string;
  /** Dokumen yang baru diunggah: daftar langsung menyaring ke dokumen itu. */
  focusDocumentId?: string | null;
  className?: string;
}

/**
 * Usulan KPI dari dokumen unggahan, dikelompokkan per KPI (design.md §4.2 Sumber, plan.md R8). Satu nilai diterima per KPI;
 * mengganti nilai pemilik meminta konfirmasi. Nilai yang diterima muncul di Paket sebagai KPI manual berasal dokumen.
 */
export function ProposalReview({ ticker, focusDocumentId, className }: ProposalReviewProps) {
  const proposals = useProposals(ticker);
  const currency = useFinancials(ticker, "standalone", 12).data?.currency ?? null;
  const [picked, setPicked] = useState<string>(ALL_DOCUMENTS);
  const [seenFocus, setSeenFocus] = useState<string | null | undefined>(undefined);
  if (focusDocumentId !== seenFocus) {
    setSeenFocus(focusDocumentId);
    if (focusDocumentId) setPicked(focusDocumentId);
  }

  if (proposals.status === "error") return <Card status="error" errorMessage={proposals.error?.message ?? "Gagal memuat usulan"} onRetry={proposals.reload} minHeight={120} />;
  if (proposals.status === "loading" || !proposals.data) {
    return (
      <div role="status" aria-busy="true" className="flex flex-col gap-2">
        <span className="sr-only">Memuat usulan…</span>
        <Skeleton height={96} />
      </div>
    );
  }
  const data = proposals.data;
  if (data.items.length === 0) return <Card status="empty" emptyMessage="Belum ada usulan. Unggah PDF atau PPTX untuk mendapat usulan nilai KPI." minHeight={96} />;

  const docs = new Map(data.documents.map((d) => [d.id, d]));
  const selected = docs.has(picked) ? picked : ALL_DOCUMENTS;
  const shown = data.items.filter((i) => !selected || i.document_id === selected);
  const groups = new Map<string, ExtractProposal[]>();
  for (const p of shown) groups.set(p.kpi_key, [...(groups.get(p.kpi_key) ?? []), p]);

  return (
    <div className={cn("flex flex-col gap-3", className)}>
      <div className="flex flex-wrap items-end justify-between gap-3">
        <label className="flex flex-col gap-2 text-label font-semibold text-text">
          Dokumen
          <select
            value={selected}
            onChange={(e) => setPicked(e.target.value)}
            className="h-12 min-w-[260px] rounded-sm border border-transparent bg-surface-sunken px-3 text-body font-normal text-text focus-visible:border-accent"
          >
            <option value={ALL_DOCUMENTS}>Semua dokumen</option>
            {data.documents.map((d) => (
              <option key={d.id} value={d.id}>
                {docLabel(d)}
              </option>
            ))}
          </select>
        </label>
        <p className="text-caption text-text-2">{`${data.counts.pending} menunggu · ${data.counts.accepted} diterima · ${data.counts.rejected} ditolak${data.counts.superseded ? ` · ${data.counts.superseded} digantikan` : ""}`}</p>
      </div>
      {groups.size === 0 ? <p className="text-label text-text-2">Dokumen ini tidak punya usulan.</p> : null}
      {[...groups.entries()].map(([k, items]) => (
        <KpiGroup key={k} items={items} docs={docs} showDoc={!selected} currency={currency} />
      ))}
    </div>
  );
}
