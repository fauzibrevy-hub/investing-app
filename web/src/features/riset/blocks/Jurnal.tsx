import { useState } from "react";
import { Button } from "../../../components/Button";
import { useDossier, useJournal } from "../../research/api";
import { JournalEntry } from "../../research/components/JournalEntry";
import { JournalQuickAdd } from "../../research/components/JournalQuickAdd";
import { Block, Empty, LoadError, Loading } from "../ui";

export const JOURNAL_RECENT = 5;
const JOURNAL_ALL = 20;

/** Isi blok 7, dipasang hanya saat blok terbuka: komposer satu baris, lima entri terakhir, **Lihat semua**. */
function JurnalBody({ ticker }: { ticker: string }) {
  const [all, setAll] = useState(false);
  const journal = useJournal(ticker, all ? JOURNAL_ALL : JOURNAL_RECENT, 0);
  const close = useDossier(ticker).data?.latest_close?.close ?? null;
  const data = journal.data;
  let list;
  if (journal.status === "error" && !data) list = <LoadError message={journal.error?.message ?? "Gagal memuat jurnal"} onRetry={journal.reload} />;
  else if (!data) list = <Loading label="Memuat jurnal…" rows={2} height={64} />;
  else if (data.items.length === 0) list = <Empty>Belum ada catatan. Tulis satu baris di atas: apa yang Anda lihat, atau keputusan yang Anda ambil hari ini.</Empty>;
  else
    list = (
      <>
        <section aria-label="Entri jurnal" className="flex flex-col gap-3">
          {data.items.map((e) => (
            <JournalEntry key={e.id} entry={e} />
          ))}
        </section>
        {!all && data.total > JOURNAL_RECENT ? (
          <Button variant="ghost" onClick={() => setAll(true)} className="-ml-3 max-w-full self-start !whitespace-normal py-1 text-left">
            {`Lihat semua (${data.total})`}
          </Button>
        ) : null}
        {all && data.total > JOURNAL_ALL ? <p className="text-caption text-text-2">{`Menampilkan ${JOURNAL_ALL} entri terbaru dari ${data.total}.`}</p> : null}
      </>
    );
  return (
    <>
      <JournalQuickAdd ticker={ticker} close={close} />
      {list}
    </>
  );
}

/** Blok 7 "Jurnal" (lipat): `JournalQuickAdd` dan entri terakhir (`GET`/`POST /{ticker}/journal`). */
export function Jurnal({ ticker, open, onToggle }: { ticker: string; open: boolean; onToggle: () => void }) {
  return (
    <Block id="jurnal" title="Jurnal" open={open} onToggle={onToggle}>
      <JurnalBody ticker={ticker} />
    </Block>
  );
}
