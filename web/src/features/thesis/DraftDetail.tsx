import { useState } from "react";
import { Trash2 } from "lucide-react";
import { Button } from "../../components/Button";
import { Sheet } from "../../components/Sheet";
import { api, ApiError, MESSAGES } from "../../lib/api";
import { invalidate } from "../../lib/useResource";
import { THESIS_BASE, useThesisDraft } from "./api";
import type { Draft } from "./types";
import { Empty, LoadError, Loading } from "../riset/ui";
import { Section } from "./DossierView";
import { shortDate } from "./format";
import { Markdown } from "./Markdown";

/**
 * Detail draf tesis satu ticker di dalam Sheet (design.md §3.4; finance-app `ThesisScreen.tsx` `DraftBody`).
 * **Hapus draf** (ghost, `status-over`) meminta konfirmasi dulu; draf vault tidak pernah sampai di sini (Perusahaan "Draf vault").
 */
export function DraftDetail({ id, onDeleted }: { id: string; onDeleted: () => void }) {
  const r = useThesisDraft<Draft>(id);
  const [confirm, setConfirm] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (r.status === "error") return r.error?.kind === "not_found" ? <Empty>Draf tidak ditemukan</Empty> : <LoadError message={r.error?.message ?? "Gagal memuat draf"} onRetry={r.reload} />;
  if (!r.data) return <Loading label="Memuat draf…" rows={3} height={64} />;
  const d = r.data;

  const remove = async () => {
    setDeleting(true);
    setError(null);
    try {
      await api.delete(`${THESIS_BASE}/drafts/${encodeURIComponent(id)}`);
      setConfirm(false);
      invalidate(`${THESIS_BASE}/`);
      onDeleted();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : MESSAGES.server);
    } finally {
      setDeleting(false);
    }
  };

  const lists: [string, string[]][] = [
    ["Falsifiers", d.falsifiers],
    ["Trigger tambah posisi", d.triggers_add],
    ["Trigger keluar", d.triggers_exit],
  ];

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <p className="text-body-strong text-text">{d.title}</p>
        <Markdown text={d.thesis_statement} />
        <p className="text-caption text-text-2">
          {[d.thesis_type, d.is_primary ? "tesis utama" : null, `Skenario: ${d.scenario_status}`, `Story: ${d.story_status}`, `diperbarui ${shortDate(d.updated_at)}`].filter(Boolean).join(" · ")}
        </p>
      </div>
      {lists.map(([label, xs]) => (
        <Section key={label} title={label}>
          {xs.length ? (
            <ul className="m-0 flex list-disc flex-col gap-1 pl-5">
              {xs.map((x, i) => (
                <li key={i} className="text-body text-text">
                  {x}
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-label text-text-2">—</p>
          )}
        </Section>
      ))}
      {d.notes ? (
        <Section title="Catatan">
          <Markdown text={d.notes} />
        </Section>
      ) : null}
      <Button variant="ghost" tone="danger" icon={<Trash2 />} onClick={() => setConfirm(true)} className="-ml-3 self-start">
        Hapus draf
      </Button>
      <Sheet
        open={confirm}
        title="Hapus draf?"
        onClose={() => (deleting ? undefined : setConfirm(false))}
        footer={
          <div className="flex gap-3">
            <Button variant="sunken" className="flex-1" onClick={() => setConfirm(false)} disabled={deleting}>
              Batal
            </Button>
            <Button className="flex-1 !bg-status-over" onClick={remove} loading={deleting}>
              Hapus
            </Button>
          </div>
        }
      >
        <p className="text-body text-text">{`Draf "${d.title}" dihapus dari daftar draf. Dossier di vault tidak tersentuh.`}</p>
        {error ? (
          <p role="alert" className="mt-3 text-label font-semibold text-status-over">
            {error}
          </p>
        ) : null}
      </Sheet>
    </div>
  );
}
