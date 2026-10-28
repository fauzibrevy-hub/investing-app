import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent } from "react";
import { Card, type LoadState } from "../../../components/Card";
import { Skeleton } from "../../../components/Skeleton";
import { cn } from "../../../lib/cn";
import type { FactTarget } from "../types";
import { FactChip } from "./FactChip";
import { ProvenanceBadge } from "./ProvenanceBadge";
import type { QuarterTableGroup, QuarterTablePeriod } from "./quarterTableData";

export type { QuarterTableCell, QuarterTableGroup, QuarterTablePeriod, QuarterTableRow } from "./quarterTableData";

export interface QuarterTableProps {
  /** Teks sr-only dan nama wilayah gulir, mis. `Laba rugi, kuartal`. */
  caption: string;
  /** Judul terlihat; pakai `scaleTitle` agar skala tertulis. */
  title?: string;
  /** Akhiran header kolom pertama, mis. `(RP MILIAR)`. */
  unitHeader: string;
  periods: QuarterTablePeriod[];
  groups: QuarterTableGroup[];
  status?: LoadState;
  emptyMessage?: string;
  errorMessage?: string;
  onRetry?: () => void;
  onOpenFact?: (target: FactTarget, trigger: HTMLElement) => void;
  /** False bagi target yang `onOpenFact` tidak bukakan dialog (mis. penanda sumber saham manual); bawaan: dialog. */
  opensDialog?: (target: FactTarget) => boolean;
  className?: string;
}

const SKELETON_COLUMNS = 6;
const SKELETON_ROWS = 5;
/** Lebar alami minimum kolom pertama (px); juga `scroll-padding` sebelum lebar terukur. */
const FIRST_MIN = 200;
const FIRST_COLUMN = "min-w-[200px]";
/** Di jendela sempit kolom pertama dibatasi bagian jendela ini (label membungkus), tetapi tidak di bawah `FIRST_FLOOR`. */
const FIRST_SHARE = 0.45;
const FIRST_FLOOR = 120;
/** Jumlah kolom periode utuh yang diusahakan terlihat di samping kolom pertama, dengan menyempitkannya sampai `FIRST_FLOOR`. */
const MIN_PERIODS = 2;
// Tepi kanan sebagai bayangan dalam: `border-collapse` melepas border sel sticky dari selnya saat tabel digulir.
const stickyHead = "sticky left-0 z-[1] bg-bg shadow-[inset_-1px_0_0_var(--border-hairline)]";
/** Kartu tanpa padding `Card` (kelas padding Card tidak bisa ditimpa karena `cn` tidak menggabung kelas Tailwind). */
const SHELL = "relative min-w-0 max-w-full rounded-md border border-border-hairline bg-bg";

/** Sel yang punya kontrol: nilai dengan asal angka, atau aksi gap. */
/** Satuan baris dalam bahasa Indonesia: `USD/shares` → `USD per saham`, `shares` → `lembar`. */
export const rowUnitText = (unit: string) => unit.replace(/\/shares$/i, " per saham").replace(/^shares$/i, "lembar");

const hasControl = (cell: { value: number | null; fact: FactTarget | null; gapAction?: unknown } | undefined) => !!cell && ((cell.value !== null && cell.fact !== null) || !!cell.gapAction);

/**
 * Lebar kolom pertama (px) untuk jendela gulir selebar `width`, label terpanjang tanpa bungkus `label`, dan kolom periode
 * selebar isinya `columns` (terlama ke terbaru); null bila tabel muat tanpa gulir. Kolom pertama mendapat lebar alaminya
 * (min. `FIRST_MIN`) tetapi paling banyak `FIRST_SHARE` jendela (min. `FIRST_FLOOR`), label membungkus di dalamnya. Kolom
 * terbaru yang utuh dimuat dari kanan; bila kurang dari `MIN_PERIODS`, kolom pertama boleh menyempit sampai `FIRST_FLOOR`
 * agar dua kolom muat. Sisa lebar jendela jadi milik kolom pertama, sehingga jendela berakhir tepat di batas kolom dan
 * tidak ada angka yang terpotong. Di jendela lebar batas itu tidak mengikat: perilakunya sama seperti sebelumnya.
 */
export function firstColumnWidth(width: number, label: number, columns: readonly number[]): number | null {
  if (width <= 0) return null;
  const natural = Math.max(FIRST_MIN, Math.ceil(label));
  if (natural + columns.reduce((sum, c) => sum + c, 0) <= width) return null;
  const capped = Math.min(natural, Math.max(FIRST_FLOOR, Math.floor(width * FIRST_SHARE)));
  const fit = (room: number, limit: number): [number, number] => {
    let used = 0;
    let n = 0;
    for (let i = columns.length - 1; i >= 0 && n < limit && used + columns[i] <= room; i--) {
      used += columns[i];
      n += 1;
    }
    return [used, n];
  };
  let [used, fitted] = fit(width - capped, columns.length);
  if (fitted < MIN_PERIODS) {
    const [narrowUsed, narrowFitted] = fit(width - FIRST_FLOOR, MIN_PERIODS);
    if (narrowFitted > fitted) [used, fitted] = [narrowUsed, narrowFitted];
  }
  // Kolom terbaru saja lebih lebar dari jendela dikurangi batas bawah: kolom pertama turun di bawah batas itu (min. 0)
  // agar kolom terbaru tetap utuh, karena angka yang terpotong lebih buruk daripada label yang sempit.
  if (fitted === 0) return Math.max(0, width - (columns[columns.length - 1] ?? 0));
  return width - used;
}

/**
 * Mengukur jendela gulir `el` untuk `firstColumnWidth`. Lebar label diukur tanpa bungkus (sementara `nowrap`), jadi tidak
 * bergantung pada lebar kolom pertama yang sedang dipasang; kolom periode di tabel yang meluap selebar isinya. Dihitung ulang
 * saat jendela atau tabel berubah ukuran (data baru dari cache, font web yang baru terpasang).
 */
function stickyWidth(el: HTMLElement): number | null {
  // Lebar pecahan (bukan `clientWidth` bulat): pembulatan menggeser posisi gulir maksimum dari perhentian snap kolom terbaru.
  const width = el.getBoundingClientRect().width || el.clientWidth;
  // Bukan `scrollWidth`: tabel yang pas karena lebar kolom pertama yang dipasang tetap diukur ulang dari lebar alami,
  // kalau tidak lebar itu berayun antara terpasang dan kosong.
  if (width <= 0) return null;
  const range = document.createRange();
  const heads = [...el.querySelectorAll<HTMLElement>("[data-first]")];
  for (const th of heads) th.style.whiteSpace = "nowrap";
  let label = 0;
  for (const th of heads) {
    range.selectNodeContents(th);
    const cs = getComputedStyle(th);
    label = Math.max(label, range.getBoundingClientRect().width + (parseFloat(cs.paddingLeft) || 0) + (parseFloat(cs.paddingRight) || 0));
  }
  for (const th of heads) th.style.whiteSpace = "";
  const columns = [...el.querySelectorAll<HTMLElement>("thead th:not([data-first])")].map((th) => th.getBoundingClientRect().width);
  return firstColumnWidth(width, label, columns);
}

/**
 * Tabel kuartal (design.md §4, §6): periode sebagai kolom dengan yang terbaru di kanan, kolom pertama sticky
 * (`th scope="row"`), header `type-table-head` dengan satuan, grup baris dengan subtotal tebal, kolom Q4 berbadge
 * `turunan`, sel `FactChip`. Gulir horizontal ada di wilayah gulir di dalam kartu; halaman tidak pernah menggulir.
 * Satu perhentian Tab untuk seluruh tabel (roving tabindex), panah dan Home/End berpindah antar sel.
 * Dibangun sebagai tabel sendiri, bukan di atas `DataTable`: komponen app-wide itu tidak punya kolom pertama
 * sticky, header baris, wadah gulir, maupun grup baris, dan tidak boleh diubah di lane ini (ASUMSI di plan.md).
 */
export function QuarterTable({ caption, title, unitHeader, periods, groups, status = "ready", emptyMessage = "Belum ada laporan", errorMessage, onRetry, onOpenFact, opensDialog, className }: QuarterTableProps) {
  const scroller = useRef<HTMLDivElement>(null);
  const grid = useRef<HTMLTableElement>(null);
  const rows = useMemo(() => groups.flatMap((g) => g.rows), [groups]);
  const firstControl = useMemo<[number, number] | null>(() => {
    for (let r = 0; r < rows.length; r++) for (let c = 0; c < periods.length; c++) if (hasControl(rows[r].cells[c])) return [r, c];
    return null;
  }, [rows, periods.length]);
  const [active, setActive] = useState<[number, number] | null>(null);
  const current = active && hasControl(rows[active[0]]?.cells[active[1]]) ? active : firstControl;

  // Periode terbaru ada di kanan: tabel dibuka menunjukkan kuartal terbaru, bukan yang terlama.
  const ready = status === "ready" && periods.length > 0 && groups.length > 0;
  // Jendela gulir memuat bilangan bulat kolom: sisa lebarnya diberikan ke kolom pertama (sticky). Dengan scroll-snap di
  // awal tiap kolom, tepi kolom sticky selalu jatuh di batas kolom, di ujung kiri, kanan, maupun perhentian di antaranya;
  // tidak ada kolom yang separuh tertutup (pecahan angka di samping label).
  const [first, setFirst] = useState<number | null>(null);
  const pinned = useRef(true);
  const pinnedFor = useRef("");
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!ready || !el) return;
    const fit = () => setFirst(stickyWidth(el));
    fit();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", fit);
      return () => window.removeEventListener("resize", fit);
    }
    const observer = new ResizeObserver(fit);
    observer.observe(el);
    // Tabel juga diamati: lebar kolom berubah tanpa jendela berubah saat tampilan Kuartal, TTM, Tahunan bertukar dari cache.
    if (grid.current) observer.observe(grid.current);
    return () => observer.disconnect();
  }, [ready, periods.length]);
  // Gulir ke periode terbaru saat tabel siap, jumlah periode berubah, atau ukuran kolom berubah selama masih di ujung
  // kanan; pembaca yang sudah menggulir ke kiri tidak dilempar kembali oleh perubahan ukuran.
  useEffect(() => {
    const el = scroller.current;
    if (!ready || !el) return;
    const key = periods.map((p) => p.key).join();
    if (pinnedFor.current !== key) {
      pinnedFor.current = key;
      pinned.current = true;
    }
    if (pinned.current) el.scrollLeft = el.scrollWidth;
  }, [ready, periods.length, first]);
  const firstWidth: CSSProperties | undefined = first ? { width: first, minWidth: first } : undefined;
  const scrollStyle = { scrollPaddingLeft: first ?? FIRST_MIN } as CSSProperties;

  // Sel yang menerima fokus tidak boleh tertutup kolom sticky: scroll-snap mandatory menggantikan gulir bawaan fokus dengan
  // perhentian terdekat, jadi sel dibawa ke tepi kolom sticky bila di luar jendela (WCAG 2.4.11).
  const reveal = (td: HTMLElement) => {
    const el = scroller.current;
    const head = el?.querySelector<HTMLElement>("[data-first]");
    if (!el || !head || el.clientWidth <= 0) return;
    const edge = head.offsetWidth;
    const left = td.offsetLeft - el.scrollLeft;
    if (left < edge - 0.5 || left + td.offsetWidth > el.clientWidth + 0.5) el.scrollLeft = td.offsetLeft - edge;
  };

  const move = (e: KeyboardEvent<HTMLTableElement>) => {
    if (!current) return;
    const delta: Record<string, [number, number]> = { ArrowLeft: [0, -1], ArrowRight: [0, 1], ArrowUp: [-1, 0], ArrowDown: [1, 0] };
    let [r, c] = current;
    if (e.key === "Home") c = -1;
    else if (e.key === "End") c = periods.length;
    const d = delta[e.key] ?? (e.key === "Home" ? [0, 1] : e.key === "End" ? [0, -1] : null);
    if (!d) return;
    e.preventDefault();
    // Lewati sel tanpa kontrol (gap) ke arah yang sama.
    for (let step = 0; step < rows.length * periods.length + 1; step++) {
      r += d[0];
      c += d[1];
      if (r < 0 || r >= rows.length || c < 0 || c >= periods.length) return;
      if (hasControl(rows[r].cells[c])) {
        setActive([r, c]);
        grid.current?.querySelector<HTMLElement>(`[data-cell="${r}:${c}"] button`)?.focus({ preventScroll: true });
        return;
      }
    }
  };

  if (status === "error") return <Card className={cn("min-w-0 max-w-full", className)} status="error" errorMessage={errorMessage ?? "Gagal memuat"} onRetry={onRetry} minHeight={120} />;
  if (status === "empty" || (status === "ready" && !ready)) {
    return <Card className={cn("min-w-0 max-w-full", className)} status="empty" emptyMessage={emptyMessage} minHeight={120} />;
  }

  if (status === "loading") {
    return (
      <section className={cn(SHELL, className)}>
        <div role="status" aria-busy="true" className="relative overflow-x-auto">
          <span className="sr-only">Memuat {caption}…</span>
          <table aria-hidden="true" className="w-full min-w-max border-collapse text-table">
            <tbody>
              {Array.from({ length: SKELETON_ROWS }, (_, r) => (
                <tr key={r} className="h-11 border-b border-border-hairline" data-skeleton-row="">
                  <td className={cn(FIRST_COLUMN, "pl-4 pr-3")}>
                    <Skeleton height={12} width="70%" />
                  </td>
                  {Array.from({ length: SKELETON_COLUMNS }, (_, c) => (
                    <td key={c} className="min-w-[96px] px-3 last:pr-4">
                      <Skeleton height={14} width={56} className="ml-auto" />
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    );
  }

  let flat = -1;
  return (
    <section className={cn(SHELL, className)}>
      {title ? <h3 className="px-4 pb-2 pt-4 text-body-strong text-text">{title}</h3> : null}
      <div ref={scroller} role="region" tabIndex={0} aria-label={`${caption}, gulir untuk kolom lain`} data-scroll-container="" onScroll={(e) => { const t = e.currentTarget; pinned.current = t.scrollWidth - t.clientWidth - t.scrollLeft < 2; }} style={scrollStyle} className="relative max-w-full snap-x snap-mandatory overflow-x-auto overscroll-x-contain">
        <table ref={grid} onKeyDown={move} className="w-full min-w-max border-collapse text-table">
          <caption className="sr-only-table">{caption}</caption>
          <thead>
            <tr className="border-b border-border-hairline">
              <th scope="col" className={cn(stickyHead, FIRST_COLUMN, "h-10 break-words pl-4 pr-3 text-left text-table-head uppercase text-text-2")} data-first="" style={firstWidth}>
                {unitHeader}
              </th>
              {periods.map((p) => (
                <th key={p.key} scope="col" className="h-10 min-w-[96px] snap-start whitespace-nowrap px-3 text-right text-table-head uppercase text-text-2 last:pr-4">
                  <span className="inline-flex items-center justify-end gap-1.5 whitespace-nowrap">
                    {p.label}
                    {p.derived ? <ProvenanceBadge kind="turunan" text="turunan" /> : null}
                    {p.pdf ? <ProvenanceBadge kind="pdf" /> : null}
                  </span>
                </th>
              ))}
            </tr>
          </thead>
          {groups.map((g) => (
            <tbody key={g.id} data-group={g.id}>
              <tr className="border-b border-border-hairline">
                <th scope="rowgroup" className={cn(stickyHead, "h-8 break-words py-1.5 pl-4 pr-3 text-left text-table-head uppercase text-text-2")} data-first="" style={firstWidth}>
                  {g.label}
                </th>
                <td colSpan={periods.length} />
              </tr>
              {g.rows.map((row) => {
                flat += 1;
                const r = flat;
                return (
                  <tr key={row.id} data-subtotal={row.subtotal || undefined} className={cn("border-b border-border-hairline text-text", row.subtotal && "font-bold")}>
                    <th scope="row" className={cn(stickyHead, FIRST_COLUMN, "h-11 break-words py-2 pl-4 pr-3 text-left", row.subtotal ? "font-bold" : "font-medium")} data-first="" style={firstWidth}>
                      {row.label}
                      {row.unit ? <span className="block text-caption font-medium text-text-2 lg:ml-1 lg:inline">{`(${rowUnitText(row.unit)})`}</span> : null}
                    </th>
                    {periods.map((p, i) => {
                      const cell = row.cells[i];
                      const isCurrent = current !== null && current[0] === r && current[1] === i;
                      return (
                        <td key={p.key} data-cell={`${r}:${i}`} onFocus={(e) => { if (!hasControl(cell)) return; setActive([r, i]); reveal(e.currentTarget); }} className="num min-w-[96px] snap-start whitespace-nowrap px-3 text-right last:pr-4">
                          <FactChip
                            value={cell?.value ?? null}
                            text={cell?.text}
                            target={cell?.fact ?? null}
                            label={row.label}
                            period={p.label}
                            flags={cell?.flags}
                            gapAction={cell?.gapAction}
                            compactBadges
                            tabIndex={isCurrent ? 0 : -1}
                            onOpen={onOpenFact}
                            popup={cell?.fact && opensDialog ? opensDialog(cell.fact) : true}
                          />
                        </td>
                      );
                    })}
                  </tr>
                );
              })}
            </tbody>
          ))}
        </table>
      </div>
    </section>
  );
}
