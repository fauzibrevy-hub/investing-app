import { useCallback, useState } from "react";
import { useNavigate } from "react-router-dom";
import { RefreshCw } from "lucide-react";
import { Button } from "../../components/Button";
import { Sheet } from "../../components/Sheet";
import { useDesk } from "../research/api";
import { calendarWhen } from "../research/components/CalendarItem";
import { DeskFooter } from "../research/desk/DeskFooter";
import { TYPE_WORD, useInbox } from "../research/desk/useInbox";
import { useMacroObservations } from "../research/desk/useMacro";
import { formatAge, formatDate } from "../research/format";
import type { DeskResponse, ResearchEvent } from "../research/types";
import { Block, Empty, LoadError, Loading, PAGE } from "../riset/ui";
import { MacroGrid } from "./MacroGrid";

/** Berapa butir kotak masuk yang tampil sebelum **Lihat semua** (design.md §3.1 blok 2). */
export const INBOX_FIRST = 5;
/** Berapa butir kalender yang tampil sebelum **Lihat semua** (blok 3). */
export const CALENDAR_FIRST = 5;

const KIND_TEXT = "Peristiwa";

/** Halaman perusahaan di blok butir itu; Beranda tetap pemiliknya (design.md §2.4 aturan 4). */
function companyHref(e: ResearchEvent & { ticker: string }): string {
  const q = new URLSearchParams();
  if (e.section) q.set("section", e.section);
  q.set("from", "beranda");
  return `/perusahaan/${e.ticker}?${q}`;
}

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
    <Block id="perlu-perhatian" title="Perlu perhatian">
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
              <Button
                variant="primary"
                fullWidth
                disabled={!opened.ticker}
                onClick={() => {
                  if (opened.ticker) navigate(companyHref({ ...opened, ticker: opened.ticker }));
                }}
              >
                Buka halaman perusahaan
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

function CalendarBlock({ desk }: { desk: DeskResponse }) {
  const [all, setAll] = useState(false);
  const items = all ? desk.calendar : desk.calendar.slice(0, CALENDAR_FIRST);
  return (
    <Block id="kalender" title="Kalender">
      {items.length === 0 ? (
        <Empty>Tidak ada jadwal 90 hari ke depan.</Empty>
      ) : (
        <ul>
          {items.map((item) => (
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
      {!all && desk.calendar.length > CALENDAR_FIRST ? (
        <Button variant="ghost" onClick={() => setAll(true)} className="-ml-3 self-start">
          {`Lihat semua (${desk.calendar.length})`}
        </Button>
      ) : null}
    </Block>
  );
}

/**
 * Beranda `#/beranda` (design.md §3.1): yang perlu perhatian pemilik sekarang. Header dengan **Muat ulang**, kotak masuk
 * (lima pertama), kalender (lima berikutnya), Makro, footer. Kotak masuk dan kalender dari `GET /api/research/desk`,
 * Makro dari `GET /api/macro/observations` saja (D12); daftar perusahaan ada di layar Perusahaan.
 */
export function Beranda() {
  const desk = useDesk();
  const macro = useMacroObservations();
  const data = desk.data;
  const reloadAll = () => {
    desk.reload();
    macro.reload();
  };
  let body;
  if (desk.status === "error" && !data) body = <LoadError message="Gagal memuat Beranda" onRetry={desk.reload} />;
  else if (!data) body = <Loading label="Memuat Beranda…" rows={3} height={60} />;
  else
    body = (
      <>
        {desk.error ? <LoadError message="Gagal memuat ulang Beranda" onRetry={desk.reload} /> : null}
        <InboxBlock desk={data} reload={desk.reload} />
        <CalendarBlock desk={data} />
      </>
    );
  return (
    <div className={PAGE}>
      <header className="flex min-h-11 items-center justify-between gap-3">
        <h1 className="text-title text-text">Beranda</h1>
        <Button variant="icon" aria-label="Muat ulang" icon={<RefreshCw strokeWidth={1.75} />} loading={desk.refreshing || macro.refreshing} onClick={reloadAll} />
      </header>
      {body}
      <MacroGrid macro={macro} />
      <DeskFooter ages={data?.ages} />
    </div>
  );
}

export default Beranda;
