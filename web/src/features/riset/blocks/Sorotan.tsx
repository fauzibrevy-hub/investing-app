import { useMemo } from "react";
import { usePack } from "../../research/api";
import { computeMovers, type Mover } from "../movers";
import { Block, LoadError, Loading } from "../ui";

function MoverList({ title, items, empty }: { title: string; items: Mover[]; empty: string | null }) {
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <h3 className="text-body-strong text-text">{title}</h3>
      {items.length ? (
        <ul className="flex flex-col">
          {items.map((m) => (
            <li key={m.id} data-mover={m.id} className="flex min-h-11 items-center border-b border-border-hairline text-body text-text last:border-b-0">
              {m.sentence}
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-label text-text-2">{empty}</p>
      )}
    </div>
  );
}

/**
 * Blok 4 "Membaik / Diawasi" (D1): tiga metrik dengan gerak YoY terbesar ke arah baik dan tiga ke arah buruk menurut
 * `direction` paket, masing-masing satu kalimat dengan angkanya. Tanpa skor, tanpa vonis.
 */
export function Sorotan({ ticker }: { ticker: string }) {
  const pack = usePack(ticker);
  const movers = useMemo(() => (pack.data ? computeMovers(pack.data) : null), [pack.data]);
  let body;
  if (pack.status === "error") body = <LoadError message={pack.error?.message ?? "Gagal memuat paket"} onRetry={pack.reload} />;
  else if (pack.status === "loading" || !movers) body = <Loading label="Memuat perubahan…" rows={3} height={32} />;
  else
    body = (
      <div className="grid gap-4 sm:grid-cols-2">
        <MoverList title="Membaik" items={movers.improving} empty={movers.improvingEmpty} />
        <MoverList title="Diawasi" items={movers.watching} empty={movers.watchingEmpty} />
      </div>
    );
  return (
    <Block id="sorotan" title="Membaik / Diawasi">
      {body}
    </Block>
  );
}
