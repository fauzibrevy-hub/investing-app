import { useEffect, useRef, useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { Button } from "../../components/Button";
import { Sheet } from "../../components/Sheet";
import { Empty, LoadError, Loading } from "../riset/ui";
import { useThesisDraft, useVaultDrafts, vaultFalsifierText } from "./api";
import type { Draft, VaultDraft } from "./types";

/** Teks tiap falsifier: draf vault menyimpan baris tabel, draf biasa teks polos. */
function falsifierTexts(d: Draft | VaultDraft): string[] {
  return (d.falsifiers as Array<string | Record<string, string>>).map((f) => (typeof f === "string" ? f : vaultFalsifierText(f))).filter(Boolean);
}

function DraftDetail({ id }: { id: string }) {
  const r = useThesisDraft(id);
  if (r.status === "loading") return <Loading label="Memuat draf…" rows={1} height={160} />;
  if (r.status === "error" || !r.data) return r.error?.kind === "not_found" ? <Empty>Draf tidak ditemukan</Empty> : <LoadError message="Gagal memuat draf" onRetry={r.reload} />;
  const d = r.data;
  const falsifiers = falsifierTexts(d);
  const file = "file_info" in d ? d.file_info.name : null;
  return (
    <div className="flex flex-col gap-4 pb-2">
      <p className="whitespace-pre-line break-words text-body text-text">{d.thesis_statement}</p>
      <section className="flex flex-col gap-1">
        <h3 className="text-label font-bold text-text">Falsifier</h3>
        {falsifiers.length ? (
          <ul className="m-0 flex list-disc flex-col gap-1 pl-5">
            {falsifiers.map((f, i) => (
              <li key={i} className="break-words text-body text-text">
                {f}
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-label text-text-2">Belum ada falsifier</p>
        )}
      </section>
      <p className="break-words text-caption text-text-2">{`${file ? `Berkas ${file} · ` : ""}baca saja`}</p>
    </div>
  );
}

function OpenSheet({ onClose }: { onClose: () => void }) {
  const list = useVaultDrafts(true);
  const [openId, setOpenId] = useState<string | null>(null);
  const back = useRef<HTMLButtonElement>(null);
  const returnTo = useRef<string | null>(null);
  const drafts = list.data?.vault_drafts ?? [];
  const opened = drafts.find((d) => d.id === openId) ?? null;

  // Fokus mengikuti tampilan: tombol kembali di detail, baris yang dibuka saat kembali ke daftar.
  useEffect(() => {
    if (openId) {
      returnTo.current = openId;
      back.current?.focus();
    } else if (returnTo.current) {
      const row = [...document.querySelectorAll<HTMLElement>("[data-vault-draft]")].find((el) => el.dataset.vaultDraft === returnTo.current);
      returnTo.current = null;
      row?.focus();
    }
  }, [openId]);

  let body;
  if (openId) {
    body = (
      <div className="flex flex-col gap-3">
        <Button ref={back} variant="ghost" icon={<ChevronLeft aria-hidden="true" size={18} strokeWidth={1.75} />} aria-label="Kembali ke daftar draf vault" onClick={() => setOpenId(null)} className="-ml-3 self-start">
          Draf vault
        </Button>
        <DraftDetail id={openId} />
      </div>
    );
  } else if (list.status === "error" && !list.data) body = <LoadError message="Gagal memuat draf vault" onRetry={list.reload} />;
  else if (!list.data) body = <Loading label="Memuat draf vault…" rows={3} height={56} />;
  else if (drafts.length === 0) body = <Empty>Belum ada draf vault</Empty>;
  else
    body = (
      <ul aria-label="Draf vault">
        {drafts.map((d) => (
          <li key={d.id} className="border-b border-border-hairline last:border-b-0">
            <button type="button" data-vault-draft={d.id} onClick={() => setOpenId(d.id)} className="flex min-h-14 w-full min-w-0 items-center gap-2 py-2 text-left active:bg-surface-sunken">
              <span className="flex min-w-0 flex-1 flex-col">
                <span data-line="1" className="truncate text-table font-bold text-text">
                  {d.title}
                </span>
                <span data-line="2" className="truncate text-caption text-text-2">
                  {[d.status_lifecycle, d.file_info.name].filter(Boolean).join(" · ")}
                </span>
              </span>
              <ChevronRight aria-hidden="true" size={18} strokeWidth={1.75} className="shrink-0 text-text-2" />
            </button>
          </li>
        ))}
      </ul>
    );

  return (
    <Sheet open title={opened?.title ?? "Draf vault"} wrapTitle onClose={onClose}>
      {body}
    </Sheet>
  );
}

/**
 * Draf vault (design.md §3.3, D10): draf tanpa ticker, baca saja (backend menolak menghapusnya, jadi tidak ada **Hapus**).
 * Daftar dua baris (judul; `status_lifecycle` · nama berkas); ketuk membuka detail di Sheet yang sama (pernyataan,
 * falsifier, nama berkas). Daftar dimuat hanya saat Sheet terbuka (`GET /api/thesis/drafts/vault`), detail dari
 * `GET /api/thesis/drafts/{id}`. Tertutup berarti tidak terpasang: buka lagi selalu mulai dari daftar.
 */
export function VaultDraftsSheet({ open, onClose }: { open: boolean; onClose: () => void }) {
  return open ? <OpenSheet onClose={onClose} /> : null;
}

export default VaultDraftsSheet;
