/** Matematika kanvas peta: titik dunia `p` tampil di layar pada `p * scale + (x, y)`. Semua fungsi murni. */

/**
 * Skala terkecil di mana teks kartu (12 px di kanvas) masih terbaca dan tombol cabang (44 px) masih terketuk. Ini juga
 * batas bawah semua zoom: tombol, pas layar, cubit, roda. Peta yang tidak muat pada skala ini digeser, tidak diperkecil.
 */
export const READABLE_SCALE = 1;
export const MIN_SCALE = READABLE_SCALE;
export const MAX_SCALE = 3;

export interface View {
  scale: number;
  x: number;
  y: number;
}

export interface Point {
  x: number;
  y: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface ScaleLimits {
  min: number;
  max: number;
}

export const DEFAULT_LIMITS: ScaleLimits = { min: MIN_SCALE, max: MAX_SCALE };

export const clamp = (v: number, lo: number, hi: number): number => Math.min(hi, Math.max(lo, v));

export const toWorld = (view: View, p: Point): Point => ({ x: (p.x - view.x) / view.scale, y: (p.y - view.y) / view.scale });

/** Skala baru di sekitar `anchor` (koordinat layar): titik dunia di bawah `anchor` tidak bergeser. */
export function zoomAround(view: View, nextScale: number, anchor: Point, limits: ScaleLimits = DEFAULT_LIMITS): View {
  const scale = clamp(nextScale, limits.min, limits.max);
  const k = scale / view.scale;
  return { scale, x: anchor.x - (anchor.x - view.x) * k, y: anchor.y - (anchor.y - view.y) * k };
}

/**
 * Cubit dua jari: skala awal dikali rasio jarak jari, dan titik dunia yang semula di bawah titik tengah awal ikut
 * titik tengah yang sekarang (cubit sekaligus menggeser).
 */
export function pinchView(start: View, startMid: Point, startDist: number, mid: Point, dist: number, limits: ScaleLimits = DEFAULT_LIMITS): View {
  const scale = clamp(start.scale * (startDist > 0 ? dist / startDist : 1), limits.min, limits.max);
  const world = toWorld(start, startMid);
  return { scale, x: mid.x - world.x * scale, y: mid.y - world.y * scale };
}

export const panBy = (view: View, dx: number, dy: number): View => ({ ...view, x: view.x + dx, y: view.y + dy });

/** Seluruh peta muat di viewport (tidak diperbesar melebihi 1:1) dan di tengah. */
export function fitView(content: Size, viewport: Size, pad = 8): View {
  const scale = Math.min(1, (viewport.width - 2 * pad) / content.width, (viewport.height - 2 * pad) / content.height);
  return { scale, x: (viewport.width - content.width * scale) / 2, y: (viewport.height - content.height * scale) / 2 };
}

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * Tampilan awal: seluruh peta bila muat dengan skala terbaca; bila tidak, skala terbaca dengan `focus` (titik dunia,
 * kartu perusahaan) di tengah viewport dan sisanya tinggal digeser.
 */
export function startView(content: Size, viewport: Size, focus: Point, pad = 8): View {
  const fit = fitView(content, viewport, pad);
  if (fit.scale >= READABLE_SCALE) return fit;
  return { scale: READABLE_SCALE, x: viewport.width / 2 - focus.x * READABLE_SCALE, y: viewport.height / 2 - focus.y * READABLE_SCALE };
}

/**
 * Tampilan awal peta satu sisi (kanvas sempit): selalu skala terbaca, kartu perusahaan di tepi kiri (peta digeser
 * hanya bila lebih lebar dari viewport), dan secara vertikal peta di tengah atau, bila lebih tinggi, dimulai dari atas
 * tumpukan supaya tidak ada kartu di atas area terlihat.
 */
export function startViewLeft(content: Size, viewport: Size): View {
  const s = READABLE_SCALE;
  return {
    scale: s,
    x: content.width * s <= viewport.width ? (viewport.width - content.width * s) / 2 : 0,
    y: content.height * s <= viewport.height ? (viewport.height - content.height * s) / 2 : 0,
  };
}

/** Tata letak berubah: geser tampilan supaya titik dunia `from` (lama) dan `to` (baru) jatuh di titik layar yang sama. */
export const shiftToKeep = (view: View, from: Point, to: Point): View => ({ ...view, x: view.x + (from.x - to.x) * view.scale, y: view.y + (from.y - to.y) * view.scale });

/**
 * Geser minimal supaya `box` (dunia) terlihat penuh dengan `margin`; di sumbu yang boxnya lebih besar dari viewport,
 * `focus` yang dipusatkan, jadi kartu yang diketuk tetap terlihat. Dengan `atStart` (kanvas satu sisi, anak di bawah
 * induk), sumbu yang kebesaran menaruh tepi kiri / atas box di margin: kartu yang diketuk di pojok kiri atas, anak-anaknya
 * terlihat sebanyak yang muat.
 */
export function revealBox(view: View, box: Box, viewport: Size, focus: Point, margin = 12, atStart = false): View {
  const axis = (pos: number, start: number, size: number, port: number, mid: number, atStart = false): number => {
    if (size * view.scale > port - 2 * margin) return atStart ? margin - start * view.scale : port / 2 - mid * view.scale;
    return clamp(pos, margin - start * view.scale, port - margin - (start + size) * view.scale);
  };
  return { scale: view.scale, x: axis(view.x, box.x, box.w, viewport.width, focus.x, atStart), y: axis(view.y, box.y, box.h, viewport.height, focus.y, atStart) };
}

/**
 * Seluruh peta (dengan bantalan `pad` di tiap sisi) muat di viewport pada skala terbaca; bila tidak, pas layar memaskan
 * sebagian dan sisanya digeser. Peta satu sisi sudah membawa bantalannya sendiri, jadi dipanggil dengan `pad` 0.
 */
export const fitsReadable = (content: Size, viewport: Size, pad = 8): boolean =>
  content.width * READABLE_SCALE + 2 * pad <= viewport.width && content.height * READABLE_SCALE + 2 * pad <= viewport.height;

/** Geser tidak boleh membuang peta keluar viewport: paling sedikit `margin` piksel peta tetap terlihat. */
export function keepInView(view: View, content: Size, viewport: Size, margin = 80): View {
  const w = content.width * view.scale;
  const h = content.height * view.scale;
  return {
    scale: view.scale,
    x: clamp(view.x, margin - w, viewport.width - margin),
    y: clamp(view.y, margin - h, viewport.height - margin),
  };
}

export const distance = (a: Point, b: Point): number => Math.hypot(a.x - b.x, a.y - b.y);
export const midpoint = (a: Point, b: Point): Point => ({ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 });
