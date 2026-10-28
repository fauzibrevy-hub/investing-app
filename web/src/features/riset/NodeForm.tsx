import { useId, useState, type FormEvent } from "react";
import { Button } from "../../components/Button";
import { FormField } from "../../components/FormField";
import { Sheet } from "../../components/Sheet";
import { addStructureNode, deleteStructureNode, updateStructureNode } from "../research/api";
import { useDraftKey } from "../research/components/draftKey";
import { formErrors } from "../research/components/fieldErrors";
import type { StructureKind, StructureNode, StructureNodeCreate, StructureNodePatch, StructureResponse } from "../research/types";
import { FIELD } from "./ui";

export type NodeFormTarget = { mode: "edit"; node: StructureNode; canDelete: boolean } | { mode: "add"; parent: StructureNode | null };

const SHOWN = ["label", "kind", "metric", "note", "parent"] as const;

export interface NodeFormProps {
  ticker: string;
  structure: StructureResponse;
  target: NodeFormTarget;
  onClose: () => void;
}

/**
 * **Ubah** satu simpul (label, jenis, metrik yang ditautkan, catatan) atau **Tambah cabang** milik pemilik. Simpul usulan
 * yang diubah sekaligus diterima; simpul milik pemilik tanpa cabang bisa dihapus. Batas panjang datang dari backend.
 */
export function NodeForm({ ticker, structure, target, onClose }: NodeFormProps) {
  const formId = useId();
  const edit = target.mode === "edit" ? target.node : null;
  const { limits } = structure;
  const [label, setLabel] = useState(edit?.label ?? "");
  const [kind, setKind] = useState<StructureKind>(edit?.kind ?? structure.kinds[0]?.id ?? "lainnya");
  const [metric, setMetric] = useState(edit?.metric ?? "");
  const [note, setNote] = useState(edit?.note ?? "");
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);
  const [removing, setRemoving] = useState(false);
  const key = useDraftKey();
  const proposed = edit?.state === "proposed";
  const title = edit ? `Ubah ${edit.label}` : target.mode === "add" && target.parent ? `Cabang baru di ${target.parent.label}` : "Cabang baru";

  const save = async (e: FormEvent) => {
    e.preventDefault();
    if (saving) return;
    const found: Record<string, string> = {};
    if (!label.trim()) found.label = "Tulis nama cabang";
    else if (label.trim().length > limits.max_label) found.label = `Paling banyak ${limits.max_label} karakter`;
    if (note.trim().length > limits.max_note) found.note = `Paling banyak ${limits.max_note} karakter`;
    // Ditolak tidak dihitung ke batas peta (server menghitung sama).
    if (!edit && structure.nodes.filter((n) => n.state !== "rejected").length >= limits.max_nodes) {
      found._ = `Peta sudah berisi ${limits.max_nodes} cabang. Tolak satu usulan dulu, lalu tambahkan cabang.`;
    }
    setErrors(found);
    if (Object.keys(found).length) return;
    setSaving(true);
    try {
      if (edit) {
        const body: StructureNodePatch = { label: label.trim(), kind, metric: metric || null, note: note.trim() || null, ...(proposed ? { state: "accepted" as const } : {}) };
        await updateStructureNode(ticker, edit.id, body, { idempotencyKey: key.forBody(body) });
      } else {
        const body: StructureNodeCreate = { label: label.trim(), kind, parent: target.mode === "add" ? (target.parent?.id ?? null) : null, metric: metric || null, note: note.trim() || null };
        await addStructureNode(ticker, body, { idempotencyKey: key.forBody(body) });
      }
      key.reset();
      onClose();
    } catch (err) {
      setErrors(formErrors(err, SHOWN, {}, "Cabang belum tersimpan"));
    } finally {
      setSaving(false);
    }
  };

  const remove = async () => {
    if (!edit || removing) return;
    setRemoving(true);
    try {
      await deleteStructureNode(ticker, edit.id);
      onClose();
    } catch (err) {
      setErrors(formErrors(err, [], {}, "Cabang belum terhapus"));
    } finally {
      setRemoving(false);
    }
  };

  const packMetrics = structure.metrics.filter((m) => m.pack);
  const coreMetrics = structure.metrics.filter((m) => !m.pack);
  return (
    <Sheet
      open
      title={title}
      onClose={onClose}
      footer={
        <div className="flex flex-col gap-2">
          <Button type="submit" form={formId} fullWidth loading={saving}>
            {proposed ? "Simpan dan terima" : "Simpan"}
          </Button>
          {target.mode === "edit" && target.node.owner ? (
            <Button variant="ghost" tone="danger" fullWidth onClick={remove} loading={removing} disabled={!target.canDelete}>
              {target.canDelete ? "Hapus cabang" : "Hapus cabang di bawahnya dulu"}
            </Button>
          ) : null}
        </div>
      }
    >
      <form id={formId} noValidate onSubmit={save} aria-label={title} className="flex flex-col gap-3">
        {proposed && edit?.source ? (
          <p className="text-caption text-text-2">{`Usulan dari ${edit.source.document}, halaman ${edit.source.page}: “${edit.source.snippet}”`}</p>
        ) : null}
        <FormField label="Nama cabang" value={label} maxLength={limits.max_label} autoFocus onChange={(e) => setLabel(e.target.value)} error={errors.label} hint={`${label.trim().length}/${limits.max_label}`} />
        <FormField label="Jenis" error={errors.kind}>
          {(c) => (
            <select {...c} value={kind} onChange={(e) => setKind(e.target.value as StructureKind)} className={FIELD}>
              {structure.kinds.map((k) => (
                <option key={k.id} value={k.id}>
                  {k.label}
                </option>
              ))}
            </select>
          )}
        </FormField>
        <FormField label="Angka terkait (opsional)" hint="Nilai terakhir metrik ini tampil di cabang. Tautkan hanya bila laporan menyebut metriknya." error={errors.metric}>
          {(c) => (
            <select {...c} value={metric} onChange={(e) => setMetric(e.target.value)} className={FIELD}>
              <option value="">Tanpa angka</option>
              {packMetrics.length ? (
                <optgroup label={structure.pack?.label ?? "Paket"}>
                  {packMetrics.map((m) => (
                    <option key={m.id} value={m.id}>{`${m.label} (${m.unit})`}</option>
                  ))}
                </optgroup>
              ) : null}
              {coreMetrics.length ? (
                <optgroup label="Inti">
                  {coreMetrics.map((m) => (
                    <option key={m.id} value={m.id}>{`${m.label} (${m.unit})`}</option>
                  ))}
                </optgroup>
              ) : null}
            </select>
          )}
        </FormField>
        <FormField label="Catatan (opsional)" multiline rows={3} maxLength={limits.max_note} value={note} onChange={(e) => setNote(e.target.value)} error={errors.note} hint={`${note.trim().length}/${limits.max_note}`} />
        {errors._ || errors.parent ? (
          <p role="alert" className="text-caption text-status-over">
            {errors._ ?? errors.parent}
          </p>
        ) : null}
      </form>
    </Sheet>
  );
}
