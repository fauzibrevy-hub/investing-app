import { FormField } from "../../../components/FormField";

/** Sama dengan batas backend (`MAX_NOTE_CHARS`). */
export const MAX_NOTE_CHARS = 600;

export function noteError(note: string): string | undefined {
  return note.trim().length > MAX_NOTE_CHARS ? `Paling banyak ${MAX_NOTE_CHARS} karakter` : undefined;
}

export interface StepNoteProps {
  note: string;
  onChange: (note: string) => void;
}

/** Langkah 3: satu kalimat tentang bisnisnya; boleh dikosongkan dan diisi nanti di Ringkasan. */
export function StepNote({ note, onChange }: StepNoteProps) {
  return (
    <FormField
      label="Bisnis dalam satu kalimat"
      hint={`Opsional, ${note.trim().length}/${MAX_NOTE_CHARS} karakter. Bisa diubah di Ringkasan.`}
      multiline
      rows={3}
      value={note}
      error={noteError(note)}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}
