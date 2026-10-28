/**
 * Teks simpul struktur saat dirender (N5): angka ditulis id-ID dan satuan `Mt` selalu satu ejaan. Teks tersimpan tidak
 * diubah; draf Claude Code menulis `1,175,412,847` dan `(MT)`, sedangkan tabel memakai `1.175.412.847` dan `Mt`.
 */

// 1,175,412,847 atau 1,175,412.5: kelompok ribuan koma, boleh berpecahan titik.
const EN_GROUPED = /(?<![\d.,])\d{1,3}(?:,\d{3})+(?:\.\d+)?(?![\d]|[.,]\d)/g;
// 2.5 atau 0.125: desimal titik. Tiga angka pecahan tanpa nol di depan ambigu dengan ribuan id-ID, jadi dibiarkan.
const EN_DECIMAL = /(?<![\d.,])(?:0\.\d+|[1-9]\d{0,2}\.\d{1,2})(?![\d]|[.,]\d)/g;
const UNIT_PAREN = /\((?:MT|mt)\)/g;
const UNIT_AFTER_NUMBER = /(\d|\bribu|\bjuta|\bmiliar|\btriliun)(\s+)(?:MT|mt)\b/g;

const swap = (s: string) => s.replace(/[,.]/g, (c) => (c === "," ? "." : ","));

export function displayText(text: string): string {
  return text
    .replace(EN_GROUPED, swap)
    .replace(EN_DECIMAL, swap)
    .replace(UNIT_PAREN, "(Mt)")
    .replace(UNIT_AFTER_NUMBER, "$1$2Mt");
}
