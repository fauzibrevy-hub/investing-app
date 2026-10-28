import { useId, type InputHTMLAttributes, type ReactNode, type TextareaHTMLAttributes } from "react";
import { cn } from "../lib/cn";
import { Skeleton } from "./Skeleton";

interface CommonProps {
  label: string;
  hint?: string;
  /** Pesan error; mengaktifkan state error (border `status-over`, `aria-invalid`). */
  error?: string;
  /** Default: input disabled. Dengan `children` (picker): skeleton menggantikan picker. */
  loading?: boolean;
  /** Pesan untuk picker yang daftarnya kosong, mis. "Belum ada kategori di Sheets". */
  emptyMessage?: string;
  className?: string;
  /**
   * Kontrol kustom (picker). Menerima id + atribut aria agar tetap terhubung ke label/hint/error.
   */
  children?: (control: { id: string; "aria-invalid"?: true; "aria-describedby"?: string }) => ReactNode;
}

type InputFieldProps = CommonProps & { multiline?: false } & Omit<InputHTMLAttributes<HTMLInputElement>, "children" | "className">;
type TextareaFieldProps = CommonProps & { multiline: true } & Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "children" | "className">;
export type FormFieldProps = InputFieldProps | TextareaFieldProps;

const controlClass =
  "w-full rounded-sm border bg-surface-sunken px-3 text-body text-text placeholder:text-text-2 focus-visible:border-accent disabled:cursor-not-allowed disabled:opacity-60";

/** Label 600 di atas, input 48 px `surface-sunken`, hint `type-caption` (design.md §8). */
export function FormField(props: FormFieldProps) {
  const { label, hint, error, loading, emptyMessage, className, children, id: idProp, ...controlProps } = props;
  const autoId = useId();
  const id = idProp ?? autoId;
  const hintId = hint ? `${id}-hint` : undefined;
  const errorId = error ? `${id}-error` : undefined;
  const describedBy = [errorId, hintId].filter(Boolean).join(" ") || undefined;
  const a11y = { id, "aria-invalid": error ? (true as const) : undefined, "aria-describedby": describedBy };
  const border = error ? "border-status-over" : "border-transparent";

  let control: ReactNode;
  if (children) {
    if (loading) {
      control = (
        <div className="grid grid-cols-4 gap-2" aria-busy="true" aria-label={`${label} sedang dimuat`}>
          {Array.from({ length: 4 }, (_, i) => (
            <Skeleton key={i} height={72} />
          ))}
        </div>
      );
    } else if (emptyMessage) {
      control = (
        <p id={id} className="rounded-sm bg-surface-sunken px-3 py-3.5 text-label text-text-2">
          {emptyMessage}
        </p>
      );
    } else {
      control = children(a11y);
    }
  } else if (props.multiline) {
    const { multiline: _m, rows = 6, ...rest } = controlProps as { multiline: true } & TextareaHTMLAttributes<HTMLTextAreaElement>;
    control = <textarea {...rest} {...a11y} rows={rows} disabled={loading || rest.disabled} className={cn(controlClass, border, "min-h-12 resize-y py-3")} />;
  } else {
    const { multiline: _m, ...rest } = controlProps as { multiline?: false } & InputHTMLAttributes<HTMLInputElement>;
    control = <input {...rest} {...a11y} disabled={loading || rest.disabled} className={cn(controlClass, border, "h-12")} />;
  }

  return (
    <div className={cn("flex flex-col gap-2", className)}>
      <label htmlFor={id} className="text-label font-semibold text-text">
        {label}
      </label>
      {control}
      {error ? (
        <p id={errorId} className="text-caption text-status-over">
          {error}
        </p>
      ) : null}
      {hint ? (
        <p id={hintId} className="text-caption text-text-2">
          {hint}
        </p>
      ) : null}
    </div>
  );
}
