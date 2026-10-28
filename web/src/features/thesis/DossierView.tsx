import type { ReactNode } from "react";
import { SectionHeader } from "../../components/SectionHeader";
import { cn } from "../../lib/cn";
import type { ThesisDetail } from "../../screens/thesis/types";
import { shortDate } from "./format";
import { Markdown } from "./Markdown";

export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      <SectionHeader title={title} level={3} />
      {children}
    </section>
  );
}

/**
 * Dossier vault, baca saja (design.md §3.4; finance-app `ThesisScreen.tsx` `DossierBody`): thesis statement, lalu Lens
 * (baris label–nilai), Falsifiers, Review log, Evidence, dan "diperbarui <tanggal>".
 */
export function DossierView({ detail: d }: { detail: ThesisDetail }) {
  const updated = shortDate(d.review.last_date);
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-1">
        {d.company ? <p className="text-label text-text-2">{d.company}</p> : null}
        <Markdown text={d.alasan_masuk ?? d.verdict_text ?? d.verdict ?? "Thesis statement belum ditulis"} />
        <p className="text-caption text-text-2">{[d.status, d.narrative_status ? `Story: ${d.narrative_status}` : null, d.tahap ? `Tahap: ${d.tahap}` : null].filter(Boolean).join(" · ")}</p>
        {updated ? <p className="text-caption text-text-2">{`diperbarui ${updated}`}</p> : null}
      </div>
      <Section title="Lens">
        {d.lens.metrics.length ? (
          <div className="flex flex-col">
            {d.lens.metrics.map((m) => (
              <div key={m.metrik} className="flex min-h-11 items-start justify-between gap-4 border-b border-border-hairline py-2">
                <span className="text-label font-semibold text-text">{m.metrik}</span>
                <span className="text-right text-label text-text-2">{m.definisi || m.alasan}</span>
              </div>
            ))}
          </div>
        ) : (
          <p className="text-label text-text-2">Lens belum diisi</p>
        )}
      </Section>
      <Section title="Falsifiers">
        {d.falsifier_items.length ? (
          <ol className="m-0 flex list-decimal flex-col gap-2 pl-5">
            {d.falsifier_items.map((f) => (
              <li key={f.n} className="text-body text-text">
                {f.text}
                {f.dates.length ? <span className="block text-caption text-text-2">{f.dates.join(", ")}</span> : null}
              </li>
            ))}
          </ol>
        ) : (
          <p className="text-label text-text-2">Belum ada falsifier</p>
        )}
      </Section>
      <Section title="Review log">
        {d.review_log.length ? (
          d.review_log.map((e, i) => (
            <div key={i} className="flex flex-col gap-1 border-b border-border-hairline pb-3">
              <p className="text-label font-bold text-text">{[e.date, e.title].filter(Boolean).join(" · ")}</p>
              <ul className="m-0 flex list-disc flex-col gap-1 pl-5">
                {e.bullets.map((b, j) => (
                  <li key={j} className={cn("text-label text-text", b.level === 1 && "ml-4")}>
                    {b.text}
                  </li>
                ))}
              </ul>
            </div>
          ))
        ) : (
          <p className="text-label text-text-2">Belum ada review</p>
        )}
      </Section>
      <Section title="Evidence">
        {d.doc_evidence.length ? (
          d.doc_evidence.map((e, i) => (
            <div key={i} className="flex min-h-11 items-start justify-between gap-4 border-b border-border-hairline py-2">
              <span className="flex flex-col">
                <span className="text-label font-semibold text-text">{`${e.metrik} · ${e.periode}`}</span>
                <span className="text-caption text-text-2">{[e.dokumen, e.hal ? `hal. ${e.hal}` : null].filter(Boolean).join(" · ")}</span>
              </span>
              <span className="num whitespace-nowrap text-right text-label text-text">{`${e.raw} ${e.unit}`.trim()}</span>
            </div>
          ))
        ) : (
          <p className="text-label text-text-2">Belum ada evidence</p>
        )}
      </Section>
    </div>
  );
}
