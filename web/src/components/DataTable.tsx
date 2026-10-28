import type { ReactNode } from "react";
import { cn } from "../lib/cn";
import type { LoadState } from "./Card";
import { ListState } from "./TransactionRow";
import { Skeleton } from "./Skeleton";

export interface Column<T> {
  key: string;
  /** Header uppercase dengan satuan, mis. "NOMINAL (RP)" (design.md §6.4). */
  header: string;
  align?: "left" | "right";
  render: (row: T) => ReactNode;
  /** Lebar kolom CSS, mis. "96px". */
  width?: string;
  className?: string;
}

interface DataTableProps<T> {
  caption: string;
  columns: Column<T>[];
  rows: T[];
  rowKey: (row: T) => string;
  status?: LoadState;
  emptyMessage?: string;
  errorMessage?: string;
  onRetry?: () => void;
  onRowClick?: (row: T) => void;
  selectedKey?: string | null;
  /** Jumlah baris skeleton saat loading. */
  skeletonRows?: number;
  className?: string;
}

/**
 * Tabel padat desktop (design.md §4): baris 40 px (44 px bila bisa dipilih), `type-table`, header `type-table-head` `text-2`
 * sticky, pemisah 1 px hairline, tanpa zebra, kolom angka rata kanan `tabular-nums`.
 * Baris yang bisa dipilih bisa difokus dan diaktifkan dengan Enter/Space.
 */
export function DataTable<T>({
  caption,
  columns,
  rows,
  rowKey,
  status = "ready",
  emptyMessage = "Belum ada data",
  errorMessage,
  onRetry,
  onRowClick,
  selectedKey,
  skeletonRows = 5,
  className,
}: DataTableProps<T>) {
  const alignClass = (c: Column<T>) => (c.align === "right" ? "text-right num" : "text-left");
  return (
    <div className={cn("min-w-0", className)}>
      <table className="w-full border-collapse text-table">
        <caption className="sr-only-table">{caption}</caption>
        <colgroup>
          {columns.map((c) => (
            <col key={c.key} style={c.width ? { width: c.width } : undefined} />
          ))}
        </colgroup>
        <thead className="sticky top-0 z-[1] bg-bg">
          <tr className="border-b border-border-hairline">
            {columns.map((c) => (
              <th key={c.key} scope="col" className={cn("h-10 whitespace-nowrap px-3 text-table-head uppercase text-text-2 first:pl-0 last:pr-0", alignClass(c))}>
                {c.header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {status === "loading"
            ? Array.from({ length: skeletonRows }, (_, i) => (
                <tr key={i} className="h-10 border-b border-border-hairline" aria-hidden="true">
                  {columns.map((c) => (
                    <td key={c.key} className="px-3 first:pl-0 last:pr-0">
                      <Skeleton height={12} width={c.align === "right" ? 64 : "70%"} className={c.align === "right" ? "ml-auto" : undefined} />
                    </td>
                  ))}
                </tr>
              ))
            : status === "ready"
              ? rows.map((row) => {
                  const key = rowKey(row);
                  const selected = selectedKey === key;
                  return (
                    <tr
                      key={key}
                      className={cn(
                        "border-b border-border-hairline text-text",
                        // Baris yang bisa dipilih 44 px: target sentuh §9 mengalahkan tinggi baris 40 px §4.
                        onRowClick ? "h-11" : "h-10",
                        onRowClick && "cursor-pointer hover:bg-surface-sunken focus-visible:bg-surface-sunken",
                        selected && "bg-surface-sunken",
                      )}
                      tabIndex={onRowClick ? 0 : undefined}
                      aria-selected={onRowClick ? selected : undefined}
                      onClick={onRowClick ? () => onRowClick(row) : undefined}
                      onKeyDown={
                        onRowClick
                          ? (e) => {
                              if (e.key === "Enter" || e.key === " ") {
                                e.preventDefault();
                                onRowClick(row);
                              }
                            }
                          : undefined
                      }
                    >
                      {columns.map((c) => (
                        <td
                          key={c.key}
                          // Angka tidak pernah dipotong (§7); hanya kolom teks yang ber-ellipsis.
                          className={cn("px-3 first:pl-0 last:pr-0", c.align === "right" ? "whitespace-nowrap" : "max-w-0 truncate", alignClass(c), c.className)}
                        >
                          {c.render(row)}
                        </td>
                      ))}
                    </tr>
                  );
                })
              : null}
        </tbody>
      </table>
      {status === "empty" || (status === "ready" && rows.length === 0) ? <ListState status="empty" message={emptyMessage} /> : null}
      {status === "error" ? <ListState status="error" message={errorMessage} onRetry={onRetry} /> : null}
    </div>
  );
}
