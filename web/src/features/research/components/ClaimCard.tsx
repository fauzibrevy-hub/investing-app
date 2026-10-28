import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { Button } from "../../../components/Button";
import { Skeleton } from "../../../components/Skeleton";
import { cn } from "../../../lib/cn";
import { ApiError } from "../../../lib/api";
import { closeClaim, useClaimEvidence, useClaims, useFinancials } from "../api";
import { chooseScale, formatDate, formatPct, formatScaled, formatValue } from "../format";
import type { Claim, ClaimStatus, Comparator, DossierResponse } from "../types";
import { STATUS_ORDER, STATUS_WORDS, ClaimStatusMark } from "./claimStatus";
import { ClaimEditor } from "./ClaimEditor";

/**
 * Satuan metrik untuk kalimat dan label: `ccy` adalah mata uang pelaporan (`Rp` untuk IDR, kode lain apa adanya,
 * "mata uang laporan" bila belum diketahui). Bukan untuk sel tabel.
 */
export function unitWord(unit: string, currency: string | null = null): string {
  if (unit !== "ccy") return unit;
  if (!currency) return "mata uang laporan";
  return currency.toUpperCase() === "IDR" ? "Rp" : currency.toUpperCase();
}

/** Nilai metrik untuk teks klaim: mata uang pada skala yang menjaga 3 sampai 6 digit, persen dan kelipatan 1 desimal. */
export function formatMetricValue(value: number, unit: string): string {
  if (unit === "ccy") {
    const scale = chooseScale([value], "IDR");
    return `${formatScaled(value, scale)}${scale.word ? ` ${scale.word}` : ""}`;
  }
  if (unit === "%" || unit === "pp" || unit === "x") return formatPct(value);
  return formatValue(value, Number.isInteger(value) ? 0 : 2);
}

/** `ccy` ambang disebut penuh (rupiah utuh): angka yang ditulis pemilik, tanpa skala. */
const plain = (n: number) => formatValue(n, Number.isInteger(n) ? 0 : 2);

/** Syarat klaim sebagai kalimat pendek: `< 0 Rp`, `antara 12,5 dan 20 %`. */
export function conditionText(comparator: Comparator, threshold: number, thresholdHi: number | null, unit: string, currency: string | null = null): string {
  const u = unitWord(unit, currency);
  return comparator === "between" ? `antara ${plain(threshold)} dan ${plain(thresholdHi ?? threshold)} ${u}` : `${comparator} ${plain(threshold)} ${u}`;
}

export interface ClaimDraft {
  metricLabel: string;
  unit: string;
  comparator: Comparator;
  threshold: number;
  thresholdHi: number | null;
  window: string;
  due: string;
  /** Awal jendela `every_quarter`: hari klaim dibuat (WIB). */
  since: string;
  currency?: string | null;
}

/**
 * Kalimat "Akan dievaluasi saat …": susunan kalimat sama dengan `preview` backend (`claims::preview`), tetapi satuan
 * dan tanggal ditulis untuk dibaca (`Rp`, `24 Okt 2026`) di tempat backend menyimpan `ccy` dan tanggal ISO.
 */
export function claimPreview(d: ClaimDraft): string {
  const cond = conditionText(d.comparator, d.threshold, d.thresholdHi, d.unit, d.currency ?? null);
  const due = formatDate(d.due);
  if (d.window === "latest") return `Akan dievaluasi setiap ada data baru: ${d.metricLabel} terbaru ${cond}.`;
  if (d.window === "at_due") return `Akan dievaluasi pada ${due}: ${d.metricLabel} ${cond}.`;
  if (d.window === "every_quarter") return `Akan dievaluasi saat tiap laporan kuartal yang terbit sejak ${formatDate(d.since)} sampai ${due}: ${d.metricLabel} ${cond}.`;
  return `Akan dievaluasi saat laporan ${d.window.replace("quarter:", "")} tersedia: ${d.metricLabel} ${cond}.`;
}

export interface ClaimCardProps {
  claim: Claim;
  /** Mata uang pelaporan untuk satuan `ccy`. */
  currency?: string | null;
  /** Dipanggil setelah klaim ditutup di backend. */
  onClosed?: () => void;
  /** Bukti langsung terbuka (kartu lengkap di `Sheet` mobile). */
  defaultEvidenceOpen?: boolean;
  /** Tautan **Buka di Sumber** di daftar bukti; mati di mobile, yang tidak punya bagian Sumber. */
  sourceLink?: boolean;
  className?: string;
}

function EvidenceList({ claimId, sourceLink }: { claimId: string; sourceLink: boolean }) {
  const evidence = useClaimEvidence(claimId);
  if (evidence.status === "loading") return <Skeleton height={44} />;
  if (evidence.status === "error") {
    return (
      <p role="alert" className="text-caption text-status-over">
        Gagal memuat bukti{" "}
        <button type="button" className="font-bold text-accent" onClick={evidence.reload}>
          Coba lagi
        </button>
      </p>
    );
  }
  const items = evidence.data?.items ?? [];
  return (
    <div className="flex flex-col gap-2">
      {items.length === 0 ? <p className="text-caption text-text-2">Belum ada bukti terlampir</p> : null}
      <ul className="flex flex-col gap-1.5">
        {items.map((d) => (
          <li key={d.id} className="flex flex-col text-caption">
            <span className="font-bold text-text">
              {d.kind_label}
              {d.doc_date ? <span className="font-medium text-text-2">{` · ${formatDate(d.doc_date)}`}</span> : null}
            </span>
            {d.url && webUrl(d.url) ? (
              <a href={webUrl(d.url)!} target="_blank" rel="noreferrer" className="truncate text-accent underline">
                {d.url}
              </a>
            ) : d.url ? (
              <span className="truncate text-text-2">{d.url}</span>
            ) : null}
            {d.text ? <span className="line-clamp-2 text-text-2">{d.text}</span> : null}
          </li>
        ))}
      </ul>
      {sourceLink ? (
        <Link to={{ search: "?section=sumber" }} className="inline-flex min-h-11 items-center text-label font-bold text-accent">
          Buka di Sumber
        </Link>
      ) : null}
    </div>
  );
}

/** Hanya tautan web yang dibuka; skema lain (`javascript:`, `data:`) tampil sebagai teks. */
export function webUrl(raw: string): string | null {
  try {
    const u = new URL(raw);
    return u.protocol === "http:" || u.protocol === "https:" ? u.href : null;
  } catch {
    return null;
  }
}

/**
 * Kartu klaim (design.md §6): titik + kata status, pernyataan, baris meta (metrik · syarat · jatuh tempo), evaluasi
 * terakhir (nilai dan tanggal), jumlah bukti, aksi **Bukti** dan **Tutup klaim**. Klaim hanya ditutup, tidak diedit
 * atau dibuka lagi, jadi **Tutup klaim** meminta satu konfirmasi.
 */
export function ClaimCard({ claim, currency = null, onClosed, defaultEvidenceOpen = false, sourceLink = true, className }: ClaimCardProps) {
  const [evidenceOpen, setEvidenceOpen] = useState(defaultEvidenceOpen);
  const [confirming, setConfirming] = useState(false);
  const [closing, setClosing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [announce, setAnnounce] = useState("");
  const closed = claim.closed_at !== null;
  const last = claim.last_eval;
  const card = useRef<HTMLElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const refocusClose = useRef(false);

  // Batal pada konfirmasi: fokus kembali ke **Tutup klaim**, bukan ke <body>.
  useEffect(() => {
    if (!confirming && refocusClose.current) {
      refocusClose.current = false;
      closeButton.current?.focus();
    }
  }, [confirming]);

  const cancel = () => {
    refocusClose.current = true;
    setConfirming(false);
  };

  const close = async () => {
    setClosing(true);
    setError(null);
    try {
      await closeClaim(claim.id);
      // Tombolnya hilang bersama konfirmasi: fokus pindah ke kartu dan penutupan diumumkan.
      card.current?.focus();
      setAnnounce("Klaim ditutup");
      setConfirming(false);
      onClosed?.();
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "Klaim belum ditutup");
    } finally {
      setClosing(false);
    }
  };

  return (
    <article
      ref={card}
      tabIndex={-1}
      aria-label={claim.statement}
      data-claim-status={claim.status}
      // Klaim tertutup bergaris putus-putus di atas latar biasa: teks status tetap di atas kontras 4,5:1.
      className={cn("flex flex-col gap-1.5 rounded-md border border-border-hairline p-3 outline-none", closed && "border-dashed", className)}
    >
      <span role="status" className="sr-only">
        {announce}
      </span>
      <div className="flex items-center justify-between gap-2">
        <ClaimStatusMark status={claim.status}>{claim.status_label ?? STATUS_WORDS[claim.status]}</ClaimStatusMark>
        {closed ? <span className="text-caption text-text-2">{`Ditutup ${formatDate(claim.closed_at!)}`}</span> : null}
      </div>
      <p className="text-body text-text">{claim.statement}</p>
      <p className="text-caption text-text-2">
        {`${claim.metric_label} ${conditionText(claim.comparator, claim.threshold, claim.threshold_hi, claim.unit, currency)} · jatuh tempo ${formatDate(claim.due)}`}
      </p>
      <p className="text-caption text-text-2">
        {last && last.value !== null ? `Terakhir ${formatMetricValue(last.value, claim.unit)}${claim.unit === "ccy" ? ` ${unitWord("ccy", currency)}` : ""} · ${formatDate(last.evaluated_at)}` : last ? `Belum ada nilai · ${formatDate(last.evaluated_at)}` : "Belum dievaluasi"}
        {` · ${claim.evidence_count} bukti`}
      </p>
      <div className="-ml-3 flex flex-wrap items-center">
        <Button variant="ghost" aria-expanded={evidenceOpen} onClick={() => setEvidenceOpen((o) => !o)}>
          Bukti
        </Button>
        {!closed && !confirming ? (
          <Button ref={closeButton} variant="ghost" onClick={() => setConfirming(true)}>
            Tutup klaim
          </Button>
        ) : null}
      </div>
      {confirming ? (
        <div role="group" aria-label="Konfirmasi tutup klaim" className="flex flex-col gap-1 rounded-sm bg-surface-sunken p-2">
          <p className="text-caption text-text">Klaim yang ditutup tidak bisa diedit atau dibuka lagi.</p>
          <div className="flex gap-1">
            <Button variant="ghost" tone="danger" loading={closing} onClick={close}>
              Ya, tutup
            </Button>
            <Button variant="ghost" autoFocus onClick={cancel}>
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
      {evidenceOpen ? <EvidenceList claimId={claim.id} sourceLink={sourceLink} /> : null}
    </article>
  );
}

const rank = (c: Claim) => (c.closed_at ? STATUS_ORDER.length : STATUS_ORDER.indexOf(c.status as ClaimStatus));

/** Urutan klaim: Patah, Belum teruji, Berlaku, Kedaluwarsa; yang sudah ditutup di bawah. Stabil di dalam status. */
export function sortClaims(items: readonly Claim[]): Claim[] {
  return items.map((c, i) => [c, i] as const).sort(([a, i], [b, j]) => rank(a) - rank(b) || i - j).map(([c]) => c);
}

export interface ClaimsPanelProps {
  ticker: string;
  dossier: DossierResponse;
}

/** Separuh Klaim di pane kanan: **Tambah klaim**, editor inline di puncak daftar, lalu `ClaimCard` terurut. */
export function ClaimsPanel({ ticker, dossier }: ClaimsPanelProps) {
  const claims = useClaims(ticker);
  // Mata uang pelaporan untuk satuan `ccy` (entri cache yang sama dengan Ringkasan dan Keuangan).
  const currency = useFinancials(ticker).data?.currency ?? null;
  const [adding, setAdding] = useState(false);
  const addButton = useRef<HTMLButtonElement>(null);
  const returnFocus = useRef(false);
  const items = claims.data ? sortClaims(claims.data.items) : [];

  // Editor ditutup: fokus kembali ke **Tambah klaim**, bukan ke awal halaman.
  useEffect(() => {
    if (!adding && returnFocus.current) {
      returnFocus.current = false;
      addButton.current?.focus();
    }
  }, [adding]);

  const done = () => {
    returnFocus.current = true;
    setAdding(false);
  };

  return (
    <div className="flex flex-col gap-3">
      {!adding ? (
        <div className="-ml-3">
          <Button ref={addButton} variant="ghost" onClick={() => setAdding(true)}>
            Tambah klaim
          </Button>
        </div>
      ) : (
        <ClaimEditor
          ticker={ticker}
          metrics={claims.data?.metrics}
          metricsError={claims.status === "error" ? { message: claims.error?.message ?? "Gagal memuat daftar metrik", retry: claims.reload } : null}
          packLabel={dossier.pack.label}
          currency={currency}
          onDone={done}
        />
      )}
      {claims.status === "loading" ? (
        <div role="status" aria-busy="true" className="flex flex-col gap-2">
          <span className="sr-only">Memuat klaim…</span>
          <Skeleton height={96} />
          <Skeleton height={96} />
        </div>
      ) : null}
      {claims.status === "error" ? (
        <div role="alert" className="flex flex-col items-start gap-1">
          <p className="text-label font-semibold text-status-over">{claims.error?.message ?? "Gagal memuat klaim"}</p>
          <Button variant="ghost" onClick={claims.reload}>
            Coba lagi
          </Button>
        </div>
      ) : null}
      {claims.status === "ready" && items.length === 0 && !adding ? <p className="text-label text-text-2">Belum ada klaim. Tambah klaim untuk mencatat apa yang Anda yakini.</p> : null}
      {items.map((c) => (
        <ClaimCard key={c.id} claim={c} currency={currency} />
      ))}
    </div>
  );
}
