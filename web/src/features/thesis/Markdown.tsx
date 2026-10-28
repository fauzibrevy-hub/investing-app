import { Fragment, type ReactNode } from "react";

/**
 * `**tebal**`, `*miring*`/`_miring_`, `` `kode` ``. `_` hanya di batas kata, supaya nama seperti `net_debt` tetap utuh.
 * Tidak ada HTML mentah: semua teks lewat React.
 */
const INLINE = /(\*\*[^*]+\*\*|\*[^*\s][^*]*\*|(?<![\p{L}\p{N}])_[^_\s][^_]*_(?![\p{L}\p{N}])|`[^`]+`)/gu;

function inline(text: string): ReactNode[] {
  // `split` dengan satu grup tangkap: indeks ganjil adalah potongan bertanda, genap teks biasa.
  return text.split(INLINE).map((part, i) => {
    if (i % 2 === 0) return <Fragment key={i}>{part}</Fragment>;
    if (part.startsWith("**")) return <strong key={i}>{part.slice(2, -2)}</strong>;
    if (part.startsWith("`")) return <code key={i}>{part.slice(1, -1)}</code>;
    return <em key={i}>{part.slice(1, -1)}</em>;
  });
}

const BULLET = /^\s*[-*+]\s+/;
const NUMBERED = /^\s*\d+[.)]\s+/;
const HEADING = /^#{1,6}\s+/;

/**
 * Markdown vault yang dirender (design.md §3.4, finance-app design.md §3.4): paragraf, daftar `-`/`1.`, judul `#` (sebagai teks
 * tebal, supaya urutan heading halaman tetap), dan penekanan inline. Sengaja kecil: tanpa dependensi baru.
 */
export function Markdown({ text, className }: { text: string; className?: string }) {
  const blocks: ReactNode[] = [];
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) {
      i++;
      continue;
    }
    const key = blocks.length;
    const list = BULLET.test(line) ? BULLET : NUMBERED.test(line) ? NUMBERED : null;
    if (list) {
      const items: string[] = [];
      while (i < lines.length && list.test(lines[i])) items.push(lines[i++].replace(list, ""));
      const Tag = list === BULLET ? "ul" : "ol";
      blocks.push(
        <Tag key={key} className={`m-0 flex flex-col gap-1 pl-5 ${list === BULLET ? "list-disc" : "list-decimal"}`}>
          {items.map((item, j) => (
            <li key={j}>{inline(item)}</li>
          ))}
        </Tag>,
      );
      continue;
    }
    if (HEADING.test(line)) {
      blocks.push(
        <p key={key} className="font-bold">
          {inline(line.replace(HEADING, ""))}
        </p>,
      );
      i++;
      continue;
    }
    const para: string[] = [];
    while (i < lines.length && lines[i].trim() && !BULLET.test(lines[i]) && !NUMBERED.test(lines[i]) && !HEADING.test(lines[i])) para.push(lines[i++]);
    blocks.push(
      <p key={key} className="whitespace-pre-line">
        {inline(para.join("\n"))}
      </p>,
    );
  }
  return <div className={className ?? "flex flex-col gap-2 text-body text-text"}>{blocks}</div>;
}
