import { cn } from "../../../lib/cn";
import { formatDay } from "../format";
import type { CalendarItem as CalendarEntry } from "../types";

/** Tanggal, atau jendela `1 Okt – 30 Okt` untuk batas POJK (ditampilkan sebagai jendela, bukan tanggal tunggal). */
export function calendarWhen(item: CalendarEntry): string {
  return item.window ? `${formatDay(item.window.start)} – ${formatDay(item.window.end)}` : formatDay(item.date);
}

export interface CalendarItemProps {
  item: CalendarEntry;
  className?: string;
}

/** Baris kalender (design.md §4.1 blok 4): tanggal atau jendela, `TICKER`, jenis. Perkiraan dan lewat batas ditulis sebagai kata. */
export function CalendarItem({ item, className }: CalendarItemProps) {
  return (
    <li className={cn("flex min-h-11 items-center gap-3 border-b border-border-hairline px-4 py-2 last:border-b-0", className)}>
      <time dateTime={item.window?.end ?? item.date} className="num w-40 shrink-0 text-table text-text">
        {calendarWhen(item)}
      </time>
      <span className="w-12 shrink-0 text-table font-bold text-text">{item.ticker}</span>
      <span className="min-w-0 flex-1 break-words text-table text-text">
        {item.label}
        {item.note ? <span className="text-text-2">{` · ${item.note}`}</span> : null}
      </span>
      {item.projected ? <span className="shrink-0 text-caption text-text-2">perkiraan</span> : null}
      {item.overdue ? <span className="shrink-0 text-caption font-bold text-status-warn">lewat batas</span> : null}
    </li>
  );
}
