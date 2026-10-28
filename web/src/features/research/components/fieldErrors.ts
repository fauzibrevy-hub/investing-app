import { ApiError } from "../../../lib/api";
import { MINUS } from "../format";

/** Pesan 422 backend (bahasa Inggris, `src/research/api*.rs`, `documents.rs`) → teks UI. Pesan yang tidak dikenal apa adanya. */
const MESSAGES: Array<[RegExp, (m: RegExpExecArray) => string]> = [
  [/between (-?[\d.]+) and (-?[\d.]+)/, (m) => `Harus antara ${m[1].replace("-", MINUS)} dan ${m[2]}${/integer/.test(m.input) ? ", bilangan bulat" : ""}`],
  [/greater than 0/, () => "Harus di atas nol"],
  [/should be a date/, () => "Tulis tanggal seperti 2026-10-24"],
  [/should be a number/, () => "Harus angka"],
  [/http or https URL/, () => "Tempel satu URL http(s), paling banyak 500 karakter"],
  [/needs a url or text/, () => "Tempel URL atau teks"],
  [/Text must be at most (\d+)/, (m) => `Teks paling banyak ${m[1]} karakter`],
  [/cannot be dated in the future/, () => "Tanggal tidak boleh di masa depan"],
  [/Source note must be 1 to (\d+)/, (m) => `Tulis sumbernya, paling banyak ${m[1]} karakter`],
  [/No such claim/, () => "Klaim ini tidak ada untuk emiten ini"],
  [/should look like \w+:\d{4}Q\d/, () => "Tulis periode seperti 2026Q2"],
  [/Not a KPI of this ticker's pack/, () => "Bukan KPI paket emiten ini"],
  [/exactly one of claim_id and kpi_key/, () => "Pilih satu target: klaim atau KPI"],
  [/No such document/, () => "Dokumen ini tidak ada lagi"],
  [/Only PDF and PPTX files are accepted/, () => "Hanya berkas PDF atau PPTX"],
  [/^Field required/, () => "Wajib diisi"],
  [/^Input should be '/, () => "Pilihan tidak dikenal"],
];

export function fieldMessage(message: string): string {
  for (const [re, text] of MESSAGES) {
    const m = re.exec(message);
    if (m) return text(m);
  }
  return message;
}

/**
 * Galat simpan → galat per field formulir. `rename` memetakan kunci backend ke field formulir; kunci yang tidak
 * ditampilkan formulir (`shown`) masuk ke `_` sehingga tidak ada galat yang hilang. Bukan 422 → `_` dengan pesannya.
 */
export function formErrors(err: unknown, shown: readonly string[], rename: Record<string, string> = {}, fallback = "Gagal menyimpan"): Record<string, string> {
  const fields = err instanceof ApiError ? err.fields : {};
  if (!Object.keys(fields).length) return { _: err instanceof Error && err.message ? err.message : fallback };
  const out: Record<string, string> = {};
  const rest: string[] = [];
  for (const [k, v] of Object.entries(fields)) {
    const key = rename[k] ?? k;
    if (shown.includes(key)) out[key] = fieldMessage(v);
    else rest.push(fieldMessage(v));
  }
  if (rest.length) out._ = rest.join("; ");
  return out;
}
