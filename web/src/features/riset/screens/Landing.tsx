import { useCallback, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { ChevronRight } from "lucide-react";
import { Button } from "../../../components/Button";
import { Sheet } from "../../../components/Sheet";
import { cn } from "../../../lib/cn";
import { useDesk } from "../../research/api";
import { calendarWhen } from "../../research/components/CalendarItem";
import { CoverageMeter } from "../../research/components/CoverageMeter";
import { ClaimCountsLine } from "../../research/components/claimStatus";
import { MacroContextCard } from "../../research/desk/MacroContextCard";
import { DeskFooter } from "../../research/desk/DeskScreen";
import { TYPE_WORD } from "../../research/desk/MobileInbox";
import { useInbox } from "../../research/desk/useInbox";
import { formatAge, formatDate, formatDay, periodLabel } from "../../research/format";
import type { CoverageRow, DeskResponse } from "../../research/types";
import { Block, Empty, LoadError, Loading, PAGE } from "../ui";
import { groupCoverage, latestReportQuarter } from "./landingData";

/** Berapa butir kotak masuk yang tampil sebelum **Lihat semua** (ui-slicing.md §2 blok 3). */
export const INBOX_FIRST = 5;
/** Berapa butir kalender yang tampil (blok 4). */
export const CALENDAR_FIRST = 5;

function CompanyRow({ row }: { row: CoverageRow }) {
  const latest = latestReportQuarter(row);
  const kpiDone = row.kpi.entered + row.kpi.unknown;
  const next = row.next_report;
  return (
    <li data-company={row.ticker} className="border-b border-border-hairline last:border-b-0">
      <Link to={`/riset/${row.ticker}`} className="flex min-h-16 flex-col justify-center gap-0.5 py-2 active:bg-surface-sunken">
        <span data-line="1" className="flex min-w-0 items-baseline gap-2 text-table">
          <span className="shrink-0 text-body-strong text-text">{row.ticker}</span>
          <span className="min-w-0 truncate text-text-2">{row.pack.label ?? "Tanpa paket"}</span>
          <span className="ml-auto shrink-0 whitespace-nowrap text-caption text-text-2">{latest ? periodLabel(latest) : "belum ada laporan"}</span>
        </span>
        <span data-line="2" className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-0.5 text-caption text-text-2">
          <ClaimCountsLine counts={row.claims} hideZero />
          <span className="inline-flex items-center gap-1.5">
            KPI
            <CoverageMeter populated={kpiDone} total={row.kpi.expected} label={`Cakupan KPI ${row.ticker}`} />
          </span>
          {next ? <span className={cn(next.overdue && "font-semibold text-status-warn")}>{`Laporan berikut: ${periodLabel(next.label)}, batas ${formatDay(next.deadline)}${next.overdue ? " (lewat)" : ""}`}</span> : null}
        </span>
      </Link>
    </li>
  );
}

function Companies({ desk }: { desk: DeskResponse }) {
  const [showPeers, setShowPeers] = useState(false);
  const { held, manual, peers } = groupCoverage(desk.coverage);
  const own = [...held, ...manual];
  return (
    <Block id="perusahaan" title="Perusahaan saya">
      {own.length === 0 ? (
        <Empty
          action={
            <Link to="/riset/add" className="inline-flex min-h-11 items-center text-label font-bold text-accent">
              Tambah emiten
            </Link>
          }
        >
          Belum ada emiten di Riset. Tambah satu kode emiten untuk mulai mencatat strukturnya, klaim, dan jurnalnya.
        </Empty>
      ) : (
        <ul aria-label="Perusahaan saya">
          {own.map((r) => (
            <CompanyRow key={r.ticker} row={r} />
          ))}
        </ul>
      )}
      {peers.length ? (
        <>
          <Button variant="ghost" aria-expanded={showPeers} onClick={() => setShowPeers((v) => !v)} className="self-start -ml-3">
            {`Pembanding (${peers.length})`}
          </Button>
          {showPeers ? (
            <ul aria-label="Pembanding">
              {peers.map((r) => (
                <CompanyRow key={r.ticker} row={r} />
              ))}
            </ul>
          ) : null}
        </>
      ) : null}
    </Block>
  );
}

const KIND_TEXT = "Peristiwa";

function InboxBlock({ desk, reload }: { desk: DeskResponse; reload: () => void }) {
  const navigate = useNavigate();
  const inbox = useInbox({ desk, onReload: reload });
  const { filter, setFilter, shown, countOf, dismiss, pending, errors, announce, region } = inbox;
  const [all, setAll] = useState(false);
  const [openId, setOpenId] = useState<string | null>(null);
  const close = useCallback(() => setOpenId(null), []);
  const opened = shown.find((e) => e.id === openId) ?? null;
  const rows = all ? shown : shown.slice(0, INBOX_FIRST);
  // Backend mengirim paling banyak `inbox.length` butir terbaru, sedangkan chip menghitung semuanya: sebut keduanya.
  const chip = desk.filters.find((f) => f.group === filter);
  const total = chip ? countOf(filter, chip.count) : null;
  const truncated = total !== null && total > shown.length;
  return (
    <Block id="kotak-masuk" title="Kotak masuk">
      {/* `scroll-px`: tanpa itu `scroll-snap-align: start` menggulirkan chip pertama ke tepi layar dan gutter 18 px hilang. */}
      <div role="group" aria-label="Filter kotak masuk" className="hscroll -mx-[var(--gutter-left)] flex scroll-px-[var(--gutter-left)] gap-2 px-[var(--gutter-left)]">
        {desk.filters.map((f) => {
          const count = countOf(f.group, f.count);
          return (
            <Button key={f.group} variant="chip" selected={filter === f.group} onClick={() => setFilter(f.group)}>
              {count === null ? f.label : `${f.label} ${count}`}
            </Button>
          );
        })}
      </div>
      <div ref={region} tabIndex={-1} role="group" aria-label="Kotak masuk" className="outline-none">
        {rows.length === 0 ? (
          <Empty>Tidak ada yang perlu perhatian{filter === "semua" ? "." : " untuk filter ini."}</Empty>
        ) : (
          <ul>
            {rows.map((e) => (
              <li key={e.id} data-event-id={e.id} className="min-h-[60px] border-b border-border-hairline last:border-b-0">
                <button type="button" aria-haspopup="dialog" onClick={() => setOpenId(e.id)} className="flex min-h-[60px] w-full min-w-0 flex-col justify-center py-2 text-left active:bg-surface-sunken">
                  <span data-line="1" className="flex min-w-0 items-baseline gap-2 text-table">
                    <span className="min-w-0 max-w-[55%] shrink-0 truncate font-bold text-text">{e.ticker ?? "Semua emiten"}</span>
                    <span className="min-w-0 truncate text-text-2">{TYPE_WORD[e.kind]}</span>
                    {e.age_days !== null ? <span className="ml-auto shrink-0 whitespace-nowrap text-caption text-text-2">{formatAge(e.age_days)}</span> : null}
                  </span>
                  <span data-line="2" className="line-clamp-2 break-words text-table text-text">
                    {e.text ?? KIND_TEXT}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
      {!all && shown.length > INBOX_FIRST ? (
        <Button variant="ghost" onClick={() => setAll(true)} className="-ml-3 max-w-full self-start !whitespace-normal py-1 text-left">
          {truncated ? `Lihat semua (${shown.length} terbaru dari ${total})` : `Lihat semua (${shown.length})`}
        </Button>
      ) : null}
      {all && truncated ? <p className="text-caption text-text-2">{`Menampilkan ${shown.length} terbaru dari ${total}`}</p> : null}
      <Sheet
        open={opened !== null}
        title={opened ? `${opened.ticker ?? "Semua emiten"} · ${TYPE_WORD[opened.kind]}` : "Butir"}
        onClose={close}
        footer={
          opened ? (
            <div className="flex flex-col gap-2">
              <Button variant="primary" fullWidth disabled={!opened.ticker} onClick={() => navigate(`/riset/${opened.ticker}${opened.section ? `?section=${opened.section}` : ""}`)}>
                Buka riset
              </Button>
              <Button
                variant="sunken"
                fullWidth
                loading={pending.has(opened.id)}
                onClick={async () => {
                  if (await dismiss(opened)) setOpenId(null);
                }}
              >
                Abaikan
              </Button>
            </div>
          ) : undefined
        }
      >
        {opened ? (
          <div className="flex flex-col gap-2 pb-2">
            <p className="text-body text-text">{opened.text ?? KIND_TEXT}</p>
            <p className="text-caption text-text-2">
              <span className="num">{formatDate(opened.at)}</span>
              {opened.age_days !== null ? ` · ${formatAge(opened.age_days)}` : ""}
            </p>
            {errors[opened.id] ? (
              <p role="alert" className="text-caption text-status-over">
                {errors[opened.id]}
              </p>
            ) : null}
          </div>
        ) : null}
      </Sheet>
      <p role="status" aria-live="polite" className="sr-only">
        {announce}
      </p>
    </Block>
  );
}

function CalendarBlock({ desk, open, onToggle }: { desk: DeskResponse; open: boolean; onToggle: () => void }) {
  const next = desk.calendar.slice(0, CALENDAR_FIRST);
  return (
    <Block id="kalender" title="Kalender" open={open} onToggle={onToggle}>
      {next.length === 0 ? (
        <Empty>Tidak ada jadwal 90 hari ke depan.</Empty>
      ) : (
        <ul>
          {next.map((item) => (
            <li key={`${item.ticker}-${item.kind}-${item.date}-${item.label}`} className="h-[60px] border-b border-border-hairline last:border-b-0">
              <div className="flex h-full min-w-0 flex-col justify-center">
                <span data-line="1" className="flex min-w-0 items-baseline gap-2 text-table">
                  <time dateTime={item.window?.end ?? item.date} className="num shrink-0 text-text">
                    {calendarWhen(item)}
                  </time>
                  <span className="shrink-0 font-bold text-text">{item.ticker}</span>
                  {item.projected ? <span className="shrink-0 text-caption text-text-2">perkiraan</span> : null}
                  {item.overdue ? <span className="shrink-0 text-caption font-bold text-status-warn">lewat batas</span> : null}
                </span>
                <span data-line="2" className="block truncate text-table text-text-2">
                  {item.label}
                  {item.note ? ` · ${item.note}` : ""}
                </span>
              </div>
            </li>
          ))}
        </ul>
      )}
    </Block>
  );
}

const MORE: Array<{ to: string; label: string }> = [
  { to: "/riset/compare", label: "Bandingkan" },
  { to: "/riset/compare/calibration", label: "Kalibrasi" },
  { to: "/riset/packs", label: "Paket & aturan" },
];

function More() {
  return (
    <Block id="lainnya" title="Lainnya">
      <ul>
        {MORE.map((m) => (
          <li key={m.to} className="border-b border-border-hairline last:border-b-0">
            <Link to={m.to} className="flex min-h-11 items-center justify-between gap-2 text-body text-text">
              {m.label}
              <ChevronRight aria-hidden="true" size={18} strokeWidth={1.75} className="text-text-2" />
            </Link>
          </li>
        ))}
      </ul>
    </Block>
  );
}

/**
 * Landing `#/riset` (ui-slicing.md §2): header dengan **Tambah**, perusahaan saya, kotak masuk (lima pertama), kalender
 * (lipat), tautan lainnya, konteks makro, footer. Semua dari `GET /desk`; tidak ada papan cakupan mendatar.
 */
export function Landing() {
  const desk = useDesk();
  const [calendarOpen, setCalendarOpen] = useState(false);
  const data = desk.data;
  let body;
  if (desk.status === "error" && !data) body = <LoadError message="Gagal memuat Riset" onRetry={desk.reload} />;
  else if (!data) body = <Loading label="Memuat Riset…" rows={3} height={64} />;
  else
    body = (
      <>
        {desk.error ? <LoadError message="Gagal memuat ulang Riset" onRetry={desk.reload} /> : null}
        <Companies desk={data} />
        <InboxBlock desk={data} reload={desk.reload} />
        <CalendarBlock desk={data} open={calendarOpen} onToggle={() => setCalendarOpen((o) => !o)} />
      </>
    );
  return (
    <div className={PAGE}>
      <header className="flex min-h-11 items-center justify-between gap-3">
        <h1 className="text-title text-text">Riset</h1>
        <Link to="/riset/add" className="on-accent inline-flex h-11 items-center rounded-pill bg-accent px-5 text-label font-bold text-text-on-accent">
          Tambah
        </Link>
      </header>
      {body}
      <More />
      <MacroContextCard />
      <DeskFooter ages={data?.ages} />
    </div>
  );
}
