import { useCallback, useMemo, useRef, useState, type ReactNode } from "react";
import { Button } from "../../../components/Button";
import { Sheet } from "../../../components/Sheet";
import { Skeleton } from "../../../components/Skeleton";
import { useFactDetail } from "../api";
import { formatDate, formatExact, formatValue } from "../format";
import type { FactDetail, FactTarget, FinancialsView } from "../types";
import { FactChip } from "./FactChip";
import { FactDrawerContext, type FactDrawerApi } from "./factDrawerContext";
import { useFocusTrap } from "./focusTrap";
import { ProvenanceBadge } from "./ProvenanceBadge";

const VIEW_LABELS: Record<FinancialsView, string> = { standalone: "Kuartal", ttm: "TTM", annual: "Tahunan" };

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-table-head uppercase text-text-2">{label}</dt>
      <dd className="min-w-0 break-words text-label text-text">{children}</dd>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2 border-t border-border-hairline pt-3 first:border-t-0 first:pt-0">
      <h3 className="text-body-strong text-text">{title}</h3>
      {children}
    </section>
  );
}

function contextText(f: FactDetail["fact"]): string {
  return f.ctx_kind === "instant" || !f.start ? `Posisi pada ${formatDate(f.end)}` : `${formatDate(f.start)} s.d. ${formatDate(f.end)}`;
}

export interface FactDrawerBodyProps {
  status: "loading" | "ready" | "error";
  detail?: FactDetail;
  errorMessage?: string;
  onRetry?: () => void;
  /** Operand turunan membuka fakta lain di drawer yang sama. */
  onSelect?: (target: FactTarget) => void;
}

/** Isi drawer menurut design.md §6; murni terhadap `FactDetail` agar tiap state bisa diuji. */
export function FactDrawerBody({ status, detail, errorMessage, onRetry, onSelect }: FactDrawerBodyProps) {
  if (status === "loading") {
    return (
      <div role="status" aria-busy="true" className="flex flex-col gap-4">
        <span className="sr-only">Memuat asal angka…</span>
        {[0, 1, 2].map((i) => (
          <div key={i} className="flex flex-col gap-2">
            <Skeleton width="40%" height={12} />
            <Skeleton height={44} />
          </div>
        ))}
      </div>
    );
  }
  if (status === "error" || !detail) {
    return (
      <div role="alert" className="flex flex-col items-start gap-2" title={errorMessage}>
        <p className="text-label font-semibold text-status-over">Gagal memuat asal angka</p>
        {onRetry ? (
          <Button variant="ghost" onClick={onRetry}>
            Coba lagi
          </Button>
        ) : null}
      </div>
    );
  }

  const { fact, filing, derived, restatements, history } = detail;
  // Dilaporkan persis: desimal yang dinyatakan bila positif, selain itu seluruh pecahan yang ada (decimals INF/null).
  const reported = fact.decimals !== null && fact.decimals > 0 ? formatValue(fact.value, fact.decimals) : formatExact(fact.value);
  const members = fact.dims ? fact.dims.split(";").map((m) => m.replace("=", " = ")) : [];
  const filed = filing.filed_at ? formatDate(filing.filed_at) : `perkiraan ${formatDate(filing.available_from)}`;
  const open = onSelect ? (t: FactTarget) => onSelect(t) : undefined;

  return (
    <div className="flex flex-col gap-4">
      <Section title="Nilai">
        <dl className="flex flex-col gap-2">
          <Field label="Nilai sebagaimana dilaporkan">
            <span className="num text-stat">{reported}</span>
          </Field>
          <Field label="Satuan">{fact.unit}</Field>
          <Field label="Desimal">{fact.decimals === null ? "tidak dinyatakan" : `${fact.decimals} (pembulatan ${filing.rounding})`}</Field>
          <Field label="Elemen">
            <code className="text-caption">{fact.element}</code>
          </Field>
          <Field label="Label">
            {fact.label.id}
            {fact.label.en && fact.label.en !== fact.label.id ? <span className="text-text-2">{` · ${fact.label.en}`}</span> : null}
          </Field>
          {fact.cite ? (
            <Field label="Sumber">
              <span className="flex flex-col gap-1">
                <span className="inline-flex flex-wrap items-center gap-1.5">
                  <ProvenanceBadge kind="pdf" text={`PDF · hal. ${fact.cite.page}`} />
                  <span className="text-text-2">{fact.cite.label}</span>
                </span>
                <span className="text-caption text-text-2">Dibaca dari laporan asli (PDF), bukan XBRL</span>
              </span>
            </Field>
          ) : null}
          <Field label="Konteks">{contextText(fact)}</Field>
          <Field label="Dimensi">{members.length ? members.join(", ") : "Tanpa dimensi"}</Field>
        </dl>
      </Section>

      <Section title="Laporan">
        <dl className="flex flex-col gap-2">
          <Field label="Emiten dan periode">{`${filing.ticker} ${filing.fy} ${filing.period}`}</Field>
          <Field label="Diajukan">{filed}</Field>
          <Field label="Audit">{filing.audited ? "Diaudit" : "Belum diaudit"}</Field>
          <Field label="Taksonomi">{filing.taxonomy}</Field>
          <Field label="Berkas">
            <code className="text-caption">{filing.source_path}</code>
          </Field>
        </dl>
      </Section>

      <Section title="Turunan">
        {derived.length === 0 ? (
          <p className="text-label text-text-2">Angka ini dipakai apa adanya; tidak ada sel turunan.</p>
        ) : (
          <ul className="flex flex-col gap-3">
            {derived.map((d) => (
              <li key={`${d.view}-${d.fy}-${d.q}`} className="flex flex-col gap-1">
                <p className="text-label font-bold text-text">{`${VIEW_LABELS[d.view]} ${d.fy}Q${d.q}`}</p>
                <p className="text-label text-text-2">{d.formula}</p>
                <ul className="flex flex-col">
                  {d.inputs.map((input) => (
                    <li key={`${input.role}-${input.fact.filing_id}-${input.fact.ctx}`} className="flex items-center justify-between gap-2 border-b border-border-hairline">
                      <span className="inline-flex items-center gap-1.5 text-label text-text-2">
                        {input.role}
                        {input.cite ? <ProvenanceBadge kind="pdf" text={`PDF · hal. ${input.cite.page}`} /> : null}
                      </span>
                      <FactChip value={input.value} text={formatExact(input.value)} target={input.fact} label={input.role} period={formatDate(input.fact.end)} onOpen={open ? (t) => open(t) : undefined} />
                    </li>
                  ))}
                </ul>
              </li>
            ))}
          </ul>
        )}
      </Section>

      <Section title="Riwayat penyajian kembali">
        {restatements.length === 0 ? (
          <p className="text-label text-text-2">Tidak pernah disajikan kembali.</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {restatements.map((r) => (
              <li key={r.restated_filing} className="text-label text-text">
                <span className="num">{formatExact(r.original)}</span> ({r.original_filing}) menjadi <span className="num">{formatExact(r.restated)}</span> ({r.restated_filing}), terdeteksi {formatDate(r.detected_at)}
              </li>
            ))}
          </ul>
        )}
        {history.length > 1 ? (
          <ul className="flex flex-col">
            {history.map((h) => (
              <li key={h.filing_id} className="flex items-center justify-between gap-2 border-b border-border-hairline py-1 text-label">
                <span className="inline-flex items-center gap-1.5 text-text-2">
                  {h.filing_id}
                  {h.cite ? <ProvenanceBadge kind="pdf" text={`PDF · hal. ${h.cite.page}`} /> : null}
                </span>
                <span className="num text-text">
                  {formatExact(h.value)}
                  {h.original ? <span className="text-text-2"> · asli</span> : null}
                </span>
              </li>
            ))}
          </ul>
        ) : null}
      </Section>

      <Section title="Tautan">
        <ul className="flex flex-col gap-1 text-label text-text">
          {filing.source_kind === "pdf" ? (
            <>
              <li>{/* TBD: lokasi berkas PDF di komputer pemilik tidak disimpan aplikasi */}Berkas PDF: tautan belum tersedia</li>
              <li>
                Berkas data hasil baca PDF: <code className="text-caption">{filing.source_path}</code>
              </li>
            </>
          ) : filing.source_kind === "xbrl" ? (
            <>
              <li>{/* TBD: URL PDF di IDX belum diketahui (design.md §4.2) */}Berkas PDF: tautan belum tersedia</li>
              <li>
                Berkas XBRL: <code className="text-caption">{filing.source_path}</code>
              </li>
            </>
          ) : (
            <li>
              Asal laporan tidak diketahui: <code className="text-caption">{filing.source_path}</code>
            </li>
          )}
        </ul>
      </Section>
    </div>
  );
}

export interface FactDrawerProps {
  target: FactTarget | null;
  onClose: () => void;
  onSelect?: (target: FactTarget) => void;
  /** Pemicu yang menerima fokus kembali. */
  returnFocusTo?: HTMLElement | null;
}

/**
 * `Sheet` bawah di semua lebar, dengan jebakan fokus dan Esc (design.md §6, §8; D13: tidak ada panel desktop).
 * Memuat `GET /facts/{filing_id}/{element}?ctx=&dims=` dari target chip. Tanpa target: tidak dirender.
 */
export function FactDrawer({ target, onClose, onSelect, returnFocusTo }: FactDrawerProps) {
  const res = useFactDetail(target);
  if (!target) return null;
  const focusKey = `${target.filing_id}|${target.element}|${target.ctx}|${target.dims}`;
  return (
    <Sheet open title="Asal angka" onClose={onClose}>
      <FocusTrap onClose={onClose} returnFocusTo={returnFocusTo} focusKey={focusKey} />
      <FactDrawerBody status={res.status} detail={res.data} errorMessage={res.error?.message} onRetry={res.reload} onSelect={onSelect} />
    </Sheet>
  );
}

/** `Sheet` menangani Esc dan fokus masuk, tetapi tidak menjebak Tab dan memulihkan fokus ke elemen yang aktif saat dibuka. */
function FocusTrap({ onClose, returnFocusTo, focusKey }: { onClose: () => void; returnFocusTo?: HTMLElement | null; focusKey: string }) {
  const marker = useRef<HTMLSpanElement>(null);
  useFocusTrap(marker, true, onClose, { closest: '[role="dialog"]', handleEscape: false, returnFocusTo, refocusKey: focusKey });
  return <span ref={marker} hidden />;
}

/** Satu drawer bersama untuk semua `FactChip` di bawahnya; operand turunan membuka fakta lain di drawer yang sama. */
export function FactDrawerProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<{ target: FactTarget; trigger: HTMLElement | null } | null>(null);
  const open = useCallback<FactDrawerApi["open"]>((target, trigger) => setState((prev) => ({ target, trigger: trigger ?? prev?.trigger ?? null })), []);
  const close = useCallback(() => setState(null), []);
  const isOpen = state !== null;
  const api = useMemo<FactDrawerApi>(() => ({ open, close, isOpen }), [open, close, isOpen]);
  return (
    <FactDrawerContext.Provider value={api}>
      {children}
      {state ? <FactDrawer target={state.target} onClose={close} onSelect={open} returnFocusTo={state.trigger} /> : null}
    </FactDrawerContext.Provider>
  );
}
