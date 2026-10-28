import { useCallback, useId, useMemo, useState } from "react";
import { Button } from "../../../components/Button";
import { Card } from "../../../components/Card";
import { SectionHeader } from "../../../components/SectionHeader";
import { Sheet } from "../../../components/Sheet";
import { Skeleton } from "../../../components/Skeleton";
import { formatDate } from "../format";
import { IndicatorTrend, dayLabel } from "./IndicatorTrend";
import { changeText, changeWords, orderForDesk, shortUnit, staleSince, unitWords, useMacroObservations, valueText, type Observation } from "./useMacro";

/** Indikator yang tampil sebelum "Tampilkan semua": kartu konteks, bukan layar Macro. */
export const MACRO_PREVIEW = 6;

/**
 * Kolom selisih dan nilai punya lebar minimum dan tumbuh bagi angka panjang ("16.234,57", "−1.234,56 % yoy").
 * Keduanya rata kanan, jadi tepi kanan nilai sejajar di semua baris; nama (flex-1, dipotong) mengalah lebih dulu.
 */
const VALUE_COL = "min-w-[88px]";
const CHANGE_COL = "min-w-[60px]";

function Row({ o, asOf, onOpen }: { o: Observation; asOf: string; onOpen: (o: Observation) => void }) {
  const unit = shortUnit(o.unit);
  const since = staleSince(o, asOf);
  const change = changeText(o);
  return (
    <li className="border-b border-border-hairline last:border-b-0">
      {/* Tombol penuh satu baris: angka membuka sheet asalnya (pilar, satuan, kesegaran, tren, waktu ambil). */}
      <button
        type="button"
        aria-haspopup="dialog"
        onClick={() => onOpen(o)}
        className="flex min-h-11 w-full min-w-0 items-center gap-2 px-3 py-2 text-left hover:bg-surface-sunken active:bg-surface-sunken"
      >
        {/* Nama boleh dua baris (tidak dipotong); satuan dan tanggal amatan di baris kedua yang lebih kecil. */}
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="line-clamp-2 break-words text-table text-text">{o.name}</span>
          {unit || since ? (
            <span className="text-caption text-text-2">
              {unit ? <span aria-hidden="true">{unit}</span> : null}
              {unit && since ? <span aria-hidden="true"> · </span> : null}
              {since ? (
                <>
                  <span className="sr-only">diamati </span>
                  {dayLabel(since)}
                </>
              ) : null}
            </span>
          ) : null}
        </span>
        <span className={`num ${VALUE_COL} shrink-0 whitespace-nowrap text-right text-table text-text`}>
          {o.value === null ? <span className="text-caption text-text-2">belum ada</span> : valueText(o.value)}
          {o.value !== null ? <span className="sr-only">{` ${unitWords(o.unit)}`}</span> : null}
        </span>
        <span className={`num ${CHANGE_COL} shrink-0 whitespace-nowrap text-right text-table text-text-2`}>
          {change === "—" ? (
            <span className="sr-only">selisih tidak tersedia</span>
          ) : (
            <>
              <span aria-hidden="true">{change}</span>
              <span className="sr-only">{`selisih ${changeWords(o)}`}</span>
            </>
          )}
        </span>
      </button>
    </li>
  );
}

/**
 * Kartu konteks makro di Desk (prd.md §9, keputusan pemilik R7.4): indikator kunci dari daftar observasi makro,
 * hanya baca. Netral: tanpa skor, rezim, atau warna naik/turun. Baris satu baris (nama dipotong), angka tabular,
 * sasaran ketuk 44 px; muat, galat, dan kosong memakai keadaan Card yang sama dengan blok Desk lain.
 * Dipakai Desk desktop dan mobile.
 */
export function MacroContextCard() {
  const macro = useMacroObservations();
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
  const openRow = useCallback((o: Observation) => setOpenId(o.metric_id), []);

  let body;
  if (!data && macro.status === "error") {
    body = <Card status="error" errorMessage="Gagal memuat konteks makro" onRetry={macro.reload} minHeight={96} />;
  } else if (!data) {
    body = (
      <div role="status" aria-busy="true" className="flex flex-col gap-2">
        <span className="sr-only">Memuat konteks makro…</span>
        <Skeleton className="h-24 w-full" />
      </div>
    );
  } else {
    body = (
      <>
        {macro.error ? <Card status="error" errorMessage="Gagal memuat ulang konteks makro" onRetry={macro.reload} /> : null}
        <Card status={list.length === 0 ? "empty" : "ready"} emptyMessage="Belum ada observasi makro" minHeight={list.length === 0 ? 96 : undefined} className={list.length === 0 ? undefined : "!p-0"}>
          <div aria-hidden="true" className="flex items-center gap-2 border-b border-border-hairline px-3 py-2 text-table-head uppercase text-text-2">
            <span className="min-w-0 flex-1">Indikator</span>
            <span className={`${VALUE_COL} shrink-0 text-right`}>Nilai</span>
            <span className={`${CHANGE_COL} shrink-0 text-right`}>Selisih</span>
          </div>
          <ul id={listId} role="list" aria-label="Indikator makro: nilai terakhir dan selisih dari nilai sebelumnya" className="m-0 list-none p-0">
            {shown.map((o) => (
              <Row key={o.metric_id} o={o} asOf={data.as_of} onOpen={openRow} />
            ))}
          </ul>
        </Card>
        {list.length > MACRO_PREVIEW ? (
          <Button variant="ghost" aria-expanded={all} aria-controls={listId} onClick={() => setAll((v) => !v)} className="max-w-full self-start !whitespace-normal py-1 text-left">
            {`Tampilkan semua indikator (${list.length})`}
          </Button>
        ) : null}
        {data.as_of ? <p className="text-caption text-text-2">{`Data makro per ${formatDate(data.as_of)}`}</p> : null}
        <Sheet open={open !== null} title={open?.name ?? ""} onClose={close}>
          {open ? <IndicatorTrend o={open} /> : null}
        </Sheet>
      </>
    );
  }

  return (
    <section aria-label="Konteks makro" className="flex min-w-0 flex-col gap-3">
      <SectionHeader title="Konteks makro" level={2} variant="section" />
      {body}
    </section>
  );
}
