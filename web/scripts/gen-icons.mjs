#!/usr/bin/env node
/**
 * Membuat ikon PWA (design.md §4): lingkaran `accent` #1961C4 dengan glyph "I" putih, tanpa gradient.
 * Tanpa dependensi: rasterisasi + encoder PNG memakai zlib bawaan Node.
 * Jalankan: node scripts/gen-icons.mjs  (hasil di public/)
 */
import { writeFileSync } from "node:fs";
import { deflateSync } from "node:zlib";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const OUT = join(dirname(fileURLToPath(import.meta.url)), "..", "public");
const ACCENT = [0x19, 0x61, 0xc4];
const WHITE = [0xff, 0xff, 0xff];
const SS = 4; // supersampling per sumbu (anti-alias)

/**
 * Glyph "I" geometris dalam koordinat unit [0,1] relatif ke kotak glyph.
 * Satu batang di tengah, lebar 0,24 lebar kotak glyph.
 */
function inI(u, v) {
  if (u < 0 || u > 1 || v < 0 || v > 1) return false;
  const t = 0.24;
  return Math.abs(u - 0.5) <= t / 2;
}

/**
 * @param size  ukuran ikon (px)
 * @param opts.background  warna latar penuh (null = transparan)
 * @param opts.circle      gambar lingkaran accent (false = latar accent penuh, untuk maskable)
 * @param opts.glyphScale  tinggi glyph relatif terhadap ukuran ikon
 */
function render(size, { background = null, circle = true, glyphScale = 0.46 }) {
  const px = new Uint8Array(size * size * 4);
  const r = size / 2;
  const gh = size * glyphScale;
  const gw = gh * 0.72;
  // "I" simetris: kotak glyph tepat di tengah.
  const gx = r - gw / 2;
  const gy = r - gh / 2;
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      let acc = [0, 0, 0, 0];
      for (let sy = 0; sy < SS; sy++) {
        for (let sx = 0; sx < SS; sx++) {
          const fx = x + (sx + 0.5) / SS;
          const fy = y + (sy + 0.5) / SS;
          let color = background;
          let alpha = background ? 1 : 0;
          const inCircle = !circle || (fx - r) ** 2 + (fy - r) ** 2 <= r * r;
          if (inCircle) {
            color = ACCENT;
            alpha = 1;
            if (inI((fx - gx) / gw, (fy - gy) / gh)) color = WHITE;
          }
          if (alpha) {
            acc[0] += color[0];
            acc[1] += color[1];
            acc[2] += color[2];
            acc[3] += 1;
          }
        }
      }
      const i = (y * size + x) * 4;
      const n = SS * SS;
      if (acc[3] > 0) {
        px[i] = Math.round(acc[0] / acc[3]);
        px[i + 1] = Math.round(acc[1] / acc[3]);
        px[i + 2] = Math.round(acc[2] / acc[3]);
      }
      px[i + 3] = Math.round((acc[3] / n) * 255);
    }
  }
  return encodePng(size, size, px);
}

const CRC_TABLE = new Uint32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
}
function encodePng(w, h, rgba) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (w * 4 + 1)] = 0; // filter none
    Buffer.from(rgba.buffer, y * w * 4, w * 4).copy(raw, y * (w * 4 + 1) + 1);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

const icons = {
  "icon-192.png": render(192, {}),
  "icon-512.png": render(512, {}),
  // Maskable: latar accent penuh; glyph jauh di dalam safe zone 80% (lingkaran diameter 0,8).
  "icon-maskable-512.png": render(512, { circle: false, glyphScale: 0.36 }),
  // iOS: tanpa transparansi → lingkaran di atas latar putih (bg).
  "apple-touch-icon.png": render(180, { background: WHITE }),
};
for (const [name, png] of Object.entries(icons)) {
  writeFileSync(join(OUT, name), png);
  console.log(`${name} ${png.length} B`);
}
