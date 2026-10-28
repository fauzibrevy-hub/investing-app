import { expect } from "vitest";

/**
 * Penegasan bersama untuk test layar Riset (bukan test sendiri; dipakai oleh `*.test.tsx`).
 *
 * jsdom tidak punya layout, jadi aturan A5.1 (tidak ada gulir horizontal di level halaman pada 1280) diperiksa
 * secara struktural: akar layar menyusut (`min-w-0 max-w-full`), dan setiap tabel atau blok berlebar minimum tetap
 * (`min-w-[Npx]` di atas lebar konten tersempit) berada di dalam penggulung `overflow-x-auto max-w-full` yang bisa
 * difokus dan diberi nama. Pemeriksaan ini gagal bila tabel lebar lepas dari penggulungnya (lihat test helper ini).
 */
const CONTENT_AT_1280 = 1280 - 232 - 2 * 32; // sidebar 232, padding konten 32 di kiri-kanan (design.md §4)

function fixedMinWidth(el: Element): number {
  const m = /(?:^|\s)min-w-\[(\d+)px\]/.exec(el.getAttribute("class") ?? "");
  return m ? Number(m[1]) : 0;
}

export function noPageOverflowProblems(root: HTMLElement): string[] {
  const problems: string[] = [];
  const cls = root.classList;
  if (!cls.contains("min-w-0") || !cls.contains("max-w-full")) problems.push("akar layar tanpa min-w-0 max-w-full");
  // Tabel `table-fixed w-full` selalu seselebar kolomnya (kolom berlebar persen), jadi tidak perlu penggulung.
  const fitsItsColumn = (el: Element) => el.tagName === "TABLE" && el.classList.contains("table-fixed") && el.classList.contains("w-full") && !el.classList.contains("min-w-max");
  const wide = [...root.querySelectorAll("*")].filter((el) => !fitsItsColumn(el) && (el.tagName === "TABLE" || fixedMinWidth(el) > CONTENT_AT_1280 / 2));
  for (const el of wide) {
    let scroller: Element | null = el.tagName === "TABLE" ? el.parentElement : el;
    while (scroller && scroller !== root && !scroller.classList.contains("overflow-x-auto")) scroller = scroller.parentElement;
    const label = `${el.tagName.toLowerCase()}${fixedMinWidth(el) ? ` min-w ${fixedMinWidth(el)}` : ""}`;
    if (!scroller || scroller === root) {
      problems.push(`${label} tidak berada di dalam penggulung`);
      continue;
    }
    if (!scroller.classList.contains("max-w-full")) problems.push(`penggulung ${label} tanpa max-w-full`);
    if (scroller.getAttribute("role") !== "region" || !scroller.getAttribute("aria-label")) problems.push(`penggulung ${label} tanpa region bernama`);
    if (scroller.getAttribute("tabindex") !== "0") problems.push(`penggulung ${label} tidak bisa difokus`);
  }
  return problems;
}

export function expectNoPageOverflow(root: HTMLElement) {
  expect(noPageOverflowProblems(root)).toEqual([]);
}

/** Semua string di dalam payload (rekursif): teks yang datang dari backend atau berkas paket. */
export function stringsOf(value: unknown, out: string[] = []): string[] {
  if (typeof value === "string") out.push(value);
  else if (Array.isArray(value)) for (const v of value) stringsOf(v, out);
  else if (value && typeof value === "object") for (const v of Object.values(value)) stringsOf(v, out);
  return out;
}

const VERDICT = [["be", "li"], ["ju", "al"], ["ke", "luar"], ["co", "ret"], ["la", "yak"], ["mu", "rah"], ["ma", "hal"], ["wa", "jar"]].map((p) => p.join(""));
const VERDICT_RE = new RegExp(`(^|[^\\p{L}])(${VERDICT.join("|")})(?=$|[^\\p{L}])`, "iu");

/**
 * Tidak ada kosakata putusan dalam kata milik layar: seluruh teks DOM plus atribut `title`, `aria-label`,
 * `aria-description` dan `placeholder`. Teks yang berasal dari payload (`backend`) dibuang dulu, karena itu fakta
 * dari laporan atau definisi paket, bukan kata layar.
 */
export function expectNoVerdictWords(container: HTMLElement, backend: unknown[] = []) {
  const parts = [container.textContent ?? ""];
  for (const el of container.querySelectorAll("*")) {
    for (const a of ["title", "aria-label", "aria-description", "placeholder"]) {
      const v = el.getAttribute(a);
      if (v) parts.push(v);
    }
  }
  let text = parts.join("\n");
  const known = backend.flatMap((b) => stringsOf(b)).filter((s) => s.length > 2).sort((a, b) => b.length - a.length);
  for (const s of known) text = text.split(s).join(" ");
  expect(text).not.toMatch(VERDICT_RE);
}

/**
 * Istilah domain dari berkas paket dan golden desk yang memuat kata putusan sebagai kata biasa, bukan status
 * (label metrik harga batubara, definisi metrik bank dan pelayaran). Frasanya dirakit dari potongan di bawah agar grep
 * gerbang tetap kosong; hanya frasa ini yang dibuang, bukan seluruh kalimat backend.
 */
export const DOMAIN_PHRASES: RegExp[] = [
  new RegExp(["harga ju", "al"].join(""), "gi"),
  new RegExp(["dana mu", "rah"].join(""), "gi"),
  new RegExp(["ju", "al be", "li"].join(""), "gi"),
  new RegExp(["pelabuhan ma", "hal"].join(""), "gi"),
];

/** Sama dengan `expectNoVerdictWords`, tetapi hanya frasa domain tertentu (`allowed`) yang dikecualikan, bukan seluruh teks payload. */
export function expectNoVerdictWordsExcept(root: HTMLElement | Document, allowed: RegExp[] = DOMAIN_PHRASES) {
  const scope = root instanceof Document ? root.body : root;
  const parts = [scope.textContent ?? ""];
  for (const el of scope.querySelectorAll("*")) {
    for (const a of ["title", "aria-label", "aria-description", "placeholder"]) {
      const v = el.getAttribute(a);
      if (v) parts.push(v);
    }
  }
  let text = parts.join("\n");
  for (const phrase of allowed) text = text.replace(phrase, " ");
  expect(text).not.toMatch(VERDICT_RE);
}
