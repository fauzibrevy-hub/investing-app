import { useCallback, useId, useMemo, useState } from "react";
import { Button } from "../../components/Button";
import { Sheet } from "../../components/Sheet";
import type { Resource } from "../../lib/useResource";
import { formatDate } from "../research/format";
import { IndicatorTrend, dayLabel } from "../research/desk/IndicatorTrend";
import { changeText, changeWords, orderForDesk, shortUnit, staleSince, unitWords, valueText, type Observation, type ObservationsResponse } from "../research/desk/useMacro";
import { Block, Empty, LoadError, Loading } from "../riset/ui";

/** Kartu yang tampil sebelum "Tampilkan semua indikator": indikator kunci (`KEY_METRICS`) lebih dulu. */
export const MACRO_PREVIEW = 6;

function MacroCard({ o, asOf, onOpen }: { o: Observation; asOf: string; onOpen: (o: Observation) => void }) {
  const unit = shortUnit(o.unit);
  const since = staleSince(o, asOf);
  const change = changeText(o);
  return (
    <li className="min-w-0">
      {/* Satu kartu = satu tombol: angka membuka sheet asalnya (pilar, satuan, kesegaran, tren, waktu ambil). */}
      <button
        type="button"
        aria-haspopup="dialog"
        onClick={() => onOpen(o)}
        className="flex h-full min-h-11 w-full min-w-0 flex-col justify-between gap-1 rounded-md bg-surface-sunken px-3 py-2 text-left active:opacity-[0.85]"
      >
        {/* Baris 1: nama, boleh dua baris (tidak dipotong satu baris). */}
        <span data-line="1" className="line-clamp-2 break-words text-caption text-text-2">
          {o.name}
        </span>
        {/* Baris 2: nilai terakhir dan satuannya, lalu selisih bertanda. Netral: tanpa warna naik/turun. */}
        <span data-line="2" className="flex min-w-0 flex-wrap items-baseline gap-x-2">
          <span className="num whitespace-nowrap text-table text-text">
            {o.value === null ? <span className="text-caption text-text-2">belum ada</span> : valueText(o.value)}
            {o.value !== null ? <span className="sr-only">{` ${unitWords(o.unit)}`}</span> : null}
            {o.value !== null && unit ? <span aria-hidden="true" className="text-caption text-text-2">{` ${unit}`}</span> : null}
          </span>
          <span className="num ml-auto whitespace-nowrap text-caption text-text-2">
            {change === "—" ? (
              <span className="sr-only">selisih tidak tersedia</span>
            ) : (
              <>
                <span aria-hidden="true">{change}</span>
                <span className="sr-only">{`selisih ${changeWords(o)}`}</span>
              </>
            )}
          </span>
          {since ? (
            <span className="w-full text-caption text-text-2">
              <span className="sr-only">diamati </span>
              {dayLabel(since)}
            </span>
          ) : null}
        </span>
      </button>
    </li>
  );
}

/**
 * Blok Makro Beranda (design.md §3.1 blok 4, §5): kartu konteks makro Desk sebagai grid dua kolom kartu dua baris.
 * Hanya `GET /api/macro/observations` (D12), dimuat saat layar dibuka dan saat **Muat ulang**; tanpa skor, rezim, lensa.
 */
export function MacroGrid({ macro }: { macro: Resource<ObservationsResponse> }) {
  const [all, setAll] = useState(false);
  // Id, bukan salinan observasi: sheet mengikuti data terbaru dan menutup sendiri bila indikatornya hilang.
  const [openId, setOpenId] = useState<string | null>(null);
  const listId = useId();
  const data = macro.data;
  const list = useMemo(() => orderForDesk(data?.observations ?? []), [data]);
  const shown = all ? list : list.slice(0, MACRO_PREVIEW);
  const open = list.find((o) => o.metric_id === openId) ?? null;
  // Identitas tetap: Sheet memasang ulang fokusnya setiap `onClose` berganti, jadi render ulang tak boleh menggesernya.
  const close = useCallback(() => setOpenId(null), []);
  const openCard = useCallback((o: Observation) => setOpenId(o.metric_id), []);

  let body;
  if (!data && macro.status === "error") body = <LoadError message="Gagal memuat data makro" onRetry={macro.reload} />;
  else if (!data) body = <Loading label="Memuat data makro…" rows={1} height={96} />;
  else if (list.length === 0)
    body = (
      <Empty
        action={
          <Button variant="ghost" onClick={macro.reload} loading={macro.refreshing} className="-ml-3">
            Muat ulang
          </Button>
        }
      >
        Belum ada data makro
      </Empty>
    );
  else
    body = (
      <>
        {macro.error ? <LoadError message="Gagal memuat ulang data makro" onRetry={macro.reload} /> : null}
        <ul id={listId} role="list" aria-label="Indikator makro: nilai terakhir dan selisih dari nilai sebelumnya" className="m-0 grid list-none grid-cols-2 gap-2 p-0">
          {shown.map((o) => (
            <MacroCard key={o.metric_id} o={o} asOf={data.as_of} onOpen={openCard} />
          ))}
        </ul>
        {list.length > MACRO_PREVIEW ? (
          <Button variant="ghost" aria-expanded={all} aria-controls={listId} onClick={() => setAll((v) => !v)} className="-ml-3 max-w-full self-start !whitespace-normal py-1 text-left">
            {`Tampilkan semua indikator (${list.length})`}
          </Button>
        ) : null}
        {data.as_of ? <p className="text-caption text-text-2">{`Data makro per ${formatDate(data.as_of)}`}</p> : null}
        <Sheet open={open !== null} title={open?.name ?? ""} onClose={close}>
          {open ? <IndicatorTrend o={open} /> : null}
        </Sheet>
      </>
    );

  return (
    <Block id="makro" title="Makro">
      {body}
    </Block>
  );
}
