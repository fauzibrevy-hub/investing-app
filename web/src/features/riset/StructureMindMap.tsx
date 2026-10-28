import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { ChevronDown, ChevronLeft, ChevronRight, ChevronUp, Maximize2, Minus, Palette, Plus, Scan, X } from "lucide-react";
import { cn } from "../../lib/cn";
import { useModal } from "../../lib/modalStack";
import { useSheetTrigger } from "../research/dossier/mobile/useSheetTrigger";
import type { StructureKind, StructureNode } from "../research/types";
import { MindMapNodeSheet } from "./MindMapNodeSheet";
import { expandableIds, isNarrow, layoutMindMap, TOGGLE, type MindMapLayout, type PlacedNode } from "./mindmapLayout";
import {
  clamp,
  DEFAULT_LIMITS,
  distance,
  fitsReadable,
  keepInView,
  midpoint,
  MIN_SCALE,
  panBy,
  pinchView,
  revealBox,
  shiftToKeep,
  startView,
  startViewLeft,
  zoomAround,
  type Box,
  type Point,
  type Size,
  type View,
} from "./mindmapView";
import { RejectedList, type StructureTreeProps } from "./StructureTree";
import { displayText } from "./structureText";

/**
 * Warna per jenis: tujuh matra yang berjauhan (jingga-merah 17°, hijau zaitun 86°, hijau kebiruan 175°, biru 224°,
 * ungu 272°, magenta 335°, dan abu-abu untuk "lainnya"), masing-masing paling sedikit 4,5:1 terhadap putih dan abu-abu
 * kanvas (syarat garis dan lencana: 3:1). Teks jenis selalu ikut tampil, warna bukan satu-satunya penanda.
 */
export const KIND_COLOR: Record<StructureKind, string> = {
  segmen: "#1d4ed8",
  produk: "#4d7c0f",
  pelanggan: "#7e22ce",
  pendorong_harga: "#0f766e",
  biaya: "#c2410c",
  kepemilikan: "#be185d",
  lainnya: "#5a6472",
};

/** Lencana kecil di kartu, 12 px seperti teks terkecil lain di kartu. */
const CARD_BADGE = "inline-flex h-5 shrink-0 items-center rounded-sm bg-surface-sunken px-1.5 text-axis font-bold uppercase tracking-wide text-text-2";

export interface StructureMindMapProps extends StructureTreeProps {
  ticker: string;
  /** Nama perusahaan (judul kecil di kartu pusat), atau `null`. */
  name: string | null;
  /** Deskripsi bisnis: narasi kartu pusat. */
  note: string | null;
  /** Nilai terakhir metrik yang ditautkan, satu baris untuk kartu, atau `null`. */
  metricText: (node: StructureNode) => string | null;
  /** Galat keputusan terakhir; di layar penuh ditampilkan di dalam peta karena pemberitahuan blok tertutup. */
  error?: string | null;
  /**
   * Wilayah status sopan milik induk, yang tetap terpasang saat peta berganti daftar. Buka / tutup cabang, zoom dan pas layar
   * diumumkan lewat sini; di layar penuh sisa halaman `inert`, jadi pengumuman memakai wilayah di dalam overlay.
   */
  announce?: (text: string) => void;
}

/** `GestureEvent` WebKit (tidak ada di lib.dom): `scale` relatif ke awal gerakan, mulai dari 1. */
interface GestureLike extends Event {
  scale: number;
  clientX: number;
  clientY: number;
}

/** Gerak jari di bawah ambang ini tetap ketukan, bukan geser. */
const TAP_SLOP = 8;
const DOUBLE_TAP_MS = 300;
const DOUBLE_TAP_PX = 30;
const KEY_PAN = 48;
const KEY_ZOOM = 1.25;

const TOOL = "inline-flex min-h-11 min-w-11 items-center justify-center gap-1 rounded-sm px-3 text-label font-bold text-accent hover:bg-surface-sunken";

const prefersReducedMotion = () => typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

interface Gesture {
  mode: "pending" | "pan" | "pinch";
  start: Point;
  startView: View;
  startMid: Point;
  startDist: number;
}

/**
 * Struktur bisnis sebagai peta pikiran di kanvas: perusahaan di tengah, cabang ke kiri dan kanan, garis lengkung
 * berwarna menurut jenis, tiap kartu memuat catatan singkatnya. Cubit dua jari memperbesar di sekitar titik tengah jari,
 * seret menggeser, ketuk dua kali di latar memperbesar; di Mac cubit trackpad (ctrl + roda) memperbesar dan roda
 * menggeser. Ketuk kartu membuka sheet. Simpul ditolak tidak digambar; tetap ada di daftar "Ditolak".
 */
export function StructureMindMap(props: StructureMindMapProps) {
  const { nodes, ticker, name, note, metricText } = props;
  const uid = useId();
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set());
  const [size, setSize] = useState<Size>({ width: 0, height: 0 });
  // Mode peta mengikuti lebar kanvas yang terukur (bukan user agent): sempit = satu sisi, lebar = dua sisi.
  const canvasWidth = canvasWidthOf(size);
  const narrow = canvasWidth !== null;
  const [legendOpen, setLegendOpen] = useState(false);
  const layout = useMemo(() => layoutMindMap(nodes, expanded, canvasWidth), [nodes, expanded, canvasWidth]);
  const expandable = useMemo(() => expandableIds(nodes), [nodes]);
  const allOpen = expandable.length > 0 && expandable.every((id) => expanded.has(id));
  const rejected = useMemo(() => nodes.filter((n) => n.state === "rejected"), [nodes]);
  const kinds = useMemo(() => {
    const seen = new Map<StructureKind, string>();
    // Semua simpul yang tidak ditolak, bukan hanya yang tergambar: keterangan tidak berubah tinggi saat cabang dibuka.
    for (const n of nodes) if (n.state !== "rejected" && !seen.has(n.kind)) seen.set(n.kind, n.kind_label);
    return [...seen];
  }, [nodes]);
  const placedById = useMemo(() => new Map(layout.nodes.map((n) => [n.id, n])), [layout]);

  const viewportRef = useRef<HTMLDivElement>(null);
  const [view, setViewState] = useState<View>({ scale: 1, x: 0, y: 0 });
  const [animate, setAnimate] = useState(false);
  const [full, setFull] = useState(false);
  const fullPushed = useRef(false);
  const overlayRef = useRef<HTMLDivElement>(null);
  /** Tombol "Layar penuh" yang membukanya: fokus kembali ke sana, juga di Safari yang tidak memfokuskan tombol yang diketuk. */
  const fullOpener = useRef<HTMLElement | null>(null);
  /**
   * Satu baris status sopan untuk pembaca layar: buka / tutup cabang, zoom, pas layar. Teks sama berturut-turut tetap diumumkan lagi.
   * Di luar layar penuh pengumuman dikirim ke wilayah induk (`props.announce`); wilayah di dalam overlay hanya untuk layar penuh.
   */
  const [status, setStatus] = useState({ text: "", n: 0 });
  const announce = (text: string) => {
    if (props.announce && !full) props.announce(text);
    else setStatus((s) => ({ text, n: s.n + 1 }));
  };
  const selected = useSheetTrigger<string>();
  const opener = useRef<HTMLElement | null>(null);

  const measured = size.width > 0 && size.height > 0;
  /**
   * Pas layar = tampilan awal peta yang tergambar sekarang: seluruhnya bila muat pada skala terbaca, kalau tidak skala
   * terbaca dari awal pohon (kanvas sempit: pojok kiri atas; lebar: kartu pusat di tengah) dan sisanya digeser. Tidak
   * pernah di bawah `READABLE_SCALE`: teks kartu >= 12 px dan tombol cabang >= 44 px di layar.
   */
  const target = useMemo(() => (measured ? homeView(layout, size) : null), [layout, size, measured]);
  const limits = DEFAULT_LIMITS;

  // Gerakan dan roda membaca nilai terbaru lewat ref supaya pendengarnya tidak dipasang ulang tiap gerak.
  const viewRef = useRef(view);
  const latest = useRef({ layout, size, limits });
  latest.current = { layout, size, limits };
  /** Selama `true`, tampilan mengikuti pas-layar saat ukuran kanvas atau peta berubah; geser atau zoom manual mematikannya. */
  const autoFit = useRef(true);

  const commit = useCallback((next: View, animated = false) => {
    viewRef.current = next;
    setAnimate(animated && !prefersReducedMotion());
    setViewState(next);
  }, []);

  useLayoutEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const measure = () => {
      const r = el.getBoundingClientRect();
      setSize((prev) => (prev.width === r.width && prev.height === r.height ? prev : { width: r.width, height: r.height }));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (autoFit.current && target) commit(target);
  }, [target, commit]);

  const frame = useRef(0);
  useEffect(() => () => cancelAnimationFrame(frame.current), []);

  const clampView = (v: View): View => keepInView(v, latest.current.layout, latest.current.size);

  const zoomBy = (factor: number) => {
    const { size: s, limits: l } = latest.current;
    autoFit.current = false;
    const next = clampView(zoomAround(viewRef.current, viewRef.current.scale * factor, { x: s.width / 2, y: s.height / 2 }, l));
    commit(next, true);
    announce(`Zoom ${Math.round(next.scale * 100)}%`);
  };
  const fitToScreen = () => {
    // Satu bingkai tertunda dari buka / tutup cabang tidak boleh menimpa pas layar yang diketuk tepat sesudahnya.
    cancelAnimationFrame(frame.current);
    autoFit.current = true;
    if (target) commit(target, true);
    const { layout: l, size: s } = latest.current;
    announce(fitsReadable(l, s, isNarrow(s.width) ? 0 : 8) ? "Pas layar" : "Pas layar, geser untuk melihat sisanya");
  };

  /**
   * Buka atau tutup cabang. Tata letak dihitung ulang dari simpul yang tergambar; kartu `anchor` (atau kartu pusat)
   * tetap di titik layar yang sama, lalu tampilan bergeser (animasi, atau langsung bila gerak dikurangi) agar kartu itu
   * beserta anak-anaknya terlihat. `home` membingkai awal pohon (tampilan awal peta baru), juga setelah "Buka semua".
   */
  const reflow = (next: ReadonlySet<string>, anchor: string | null, mode: "reveal" | "stay" | "home") => {
    const { layout: before, size: s } = latest.current;
    const after = layoutMindMap(nodes, next, canvasWidthOf(s));
    setExpanded(next);
    announce(`${after.nodes.length} kartu tampil`);
    autoFit.current = false;
    if (!s.width || !s.height) return;
    const was = anchor ? before.nodes.find((n) => n.id === anchor) : before.centre;
    const now = anchor ? after.nodes.find((n) => n.id === anchor) : after.centre;
    if (!was || !now) return;
    const held = shiftToKeep(viewRef.current, centreOf(was), centreOf(now));
    let final = held;
    if (mode === "home") final = homeView(after, s);
    else if (mode === "reveal") final = keepInView(revealBox(held, boxAround(now, anchor, after), s, centreOf(now), 12, isNarrow(s.width)), after, s);
    cancelAnimationFrame(frame.current);
    if (prefersReducedMotion()) {
      commit(final);
      if (mode === "home") autoFit.current = true;
      return;
    }
    commit(held);
    frame.current = requestAnimationFrame(() => {
      commit(final, true);
      if (mode === "home") autoFit.current = true;
    });
  };

  const toggle = (id: string) => {
    const next = new Set(expanded);
    const opening = !next.delete(id);
    if (opening) next.add(id);
    reflow(next, id, opening ? "reveal" : "stay");
  };
  const toggleAll = () => reflow(allOpen ? new Set() : new Set(expandable), null, "home");

  // ---- gerakan ----
  const pointers = useRef(new Map<number, Point>());
  const gesture = useRef<Gesture | null>(null);
  const suppressClick = useRef(false);
  const lastTap = useRef<{ t: number; p: Point } | null>(null);
  /** Input terakhir adalah penunjuk (sentuh, klik): fokus yang menyusul bukan dari papan ketik, jadi tidak menggeser peta. */
  const pointerInput = useRef(false);
  useEffect(() => {
    const onKey = () => {
      pointerInput.current = false;
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const local = (e: { clientX: number; clientY: number }): Point => {
    const r = viewportRef.current?.getBoundingClientRect();
    return { x: e.clientX - (r?.left ?? 0), y: e.clientY - (r?.top ?? 0) };
  };

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    pointerInput.current = true;
    if (e.pointerType === "mouse" && e.button !== 0) return;
    suppressClick.current = false;
    setAnimate(false);
    const p = local(e);
    pointers.current.set(e.pointerId, p);
    if (pointers.current.size === 1) {
      gesture.current = { mode: "pending", start: p, startView: viewRef.current, startMid: p, startDist: 0 };
    } else if (pointers.current.size === 2) {
      const [a, b] = [...pointers.current.values()];
      gesture.current = { mode: "pinch", start: p, startView: viewRef.current, startMid: midpoint(a, b), startDist: distance(a, b) };
      suppressClick.current = true;
    }
  };

  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const g = gesture.current;
    if (!g || !pointers.current.has(e.pointerId)) return;
    const p = local(e);
    pointers.current.set(e.pointerId, p);
    if (g.mode === "pending") {
      if (distance(p, g.start) < TAP_SLOP) return;
      g.mode = "pan";
      suppressClick.current = true;
      // Tangkap penunjuk hanya setelah geser dimulai: menangkap sejak pointerdown mengalihkan ketukan dari kartu.
      try {
        e.currentTarget.setPointerCapture(e.pointerId);
      } catch {
        /* jsdom dan penunjuk yang sudah lepas tidak punya capture */
      }
    }
    if (g.mode === "pan") {
      autoFit.current = false;
      commit(clampView(panBy(g.startView, p.x - g.start.x, p.y - g.start.y)));
    } else if (g.mode === "pinch" && pointers.current.size >= 2) {
      const [a, b] = [...pointers.current.values()];
      autoFit.current = false;
      commit(clampView(pinchView(g.startView, g.startMid, g.startDist, midpoint(a, b), distance(a, b), latest.current.limits)));
    }
  };

  const onPointerEnd = (e: PointerEvent<HTMLDivElement>) => {
    const g = gesture.current;
    if (!pointers.current.delete(e.pointerId) || !g) return;
    const p = local(e);
    if (pointers.current.size === 0) {
      gesture.current = null;
      if (g.mode === "pending" && e.type === "pointerup") doubleTap(e, p);
    } else if (g.mode === "pinch" && pointers.current.size === 1) {
      // Satu jari diangkat: jari yang tersisa melanjutkan sebagai geser, tanpa lompatan.
      const [rest] = [...pointers.current.values()];
      gesture.current = { mode: "pan", start: rest, startView: viewRef.current, startMid: rest, startDist: 0 };
    }
  };

  /** Ketuk dua kali di latar memperbesar 2×; di kartu tidak, karena ketukan pertama sudah membuka sheet. */
  const doubleTap = (e: PointerEvent<HTMLDivElement>, p: Point) => {
    if ((e.target as HTMLElement).closest("[data-mm-node], [data-mm-toggle]")) {
      lastTap.current = null;
      return;
    }
    const prev = lastTap.current;
    if (prev && e.timeStamp - prev.t < DOUBLE_TAP_MS && distance(prev.p, p) < DOUBLE_TAP_PX) {
      lastTap.current = null;
      autoFit.current = false;
      commit(clampView(zoomAround(viewRef.current, viewRef.current.scale * 2, p, latest.current.limits)), true);
    } else {
      lastTap.current = { t: e.timeStamp, p };
    }
  };

  useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      setAnimate(false);
      autoFit.current = false;
      const r = el.getBoundingClientRect();
      const p = { x: e.clientX - r.left, y: e.clientY - r.top };
      const unit = e.deltaMode === 1 ? 16 : 1;
      const v = viewRef.current;
      const next = e.ctrlKey
        ? zoomAround(v, v.scale * Math.exp(-clamp(e.deltaY * unit, -60, 60) * 0.01), p, latest.current.limits)
        : panBy(v, -e.deltaX * unit, -e.deltaY * unit);
      commit(keepInView(next, latest.current.layout, latest.current.size));
    };
    // `passive: false` agar ctrl + roda tidak memperbesar halaman dan roda biasa tidak menggulir halaman di atas kanvas.
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [commit]);

  // Safari desktop: cubit di trackpad tidak mengirim ctrl + roda, melainkan `gesturestart` / `gesturechange` dengan
  // `scale` relatif ke awal gerakan. Titik di bawah kursor saat mulai ikut kursor, sama seperti cubit dua jari.
  useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    let startView: View | null = null;
    let startAt: Point = { x: 0, y: 0 };
    const at = (e: Event): Point => {
      const g = e as GestureLike;
      const r = el.getBoundingClientRect();
      return Number.isFinite(g.clientX) && Number.isFinite(g.clientY) ? { x: g.clientX - r.left, y: g.clientY - r.top } : { x: r.width / 2, y: r.height / 2 };
    };
    const onStart = (e: Event) => {
      e.preventDefault();
      // Safari iOS mengirim gesture dan penunjuk untuk cubit yang sama; penunjuk sudah menanganinya.
      startView = pointers.current.size > 0 ? null : viewRef.current;
      startAt = at(e);
      setAnimate(false);
    };
    const onChange = (e: Event) => {
      e.preventDefault();
      const scale = (e as GestureLike).scale;
      if (!startView || !Number.isFinite(scale)) return;
      autoFit.current = false;
      const { layout: l, size: s, limits: lim } = latest.current;
      commit(keepInView(pinchView(startView, startAt, 1, at(e), scale, lim), l, s));
    };
    const onEnd = (e: Event) => {
      e.preventDefault();
      startView = null;
    };
    el.addEventListener("gesturestart", onStart);
    el.addEventListener("gesturechange", onChange);
    el.addEventListener("gestureend", onEnd);
    return () => {
      el.removeEventListener("gesturestart", onStart);
      el.removeEventListener("gesturechange", onChange);
      el.removeEventListener("gestureend", onEnd);
    };
  }, [commit]);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const pan = (dx: number, dy: number) => {
      autoFit.current = false;
      commit(clampView(panBy(viewRef.current, dx, dy)));
    };
    if (e.key === "+" || e.key === "=") zoomBy(KEY_ZOOM);
    else if (e.key === "-" || e.key === "_") zoomBy(1 / KEY_ZOOM);
    else if (e.key === "0") fitToScreen();
    else if (e.key === "ArrowLeft") pan(KEY_PAN, 0);
    else if (e.key === "ArrowRight") pan(-KEY_PAN, 0);
    else if (e.key === "ArrowUp") pan(0, KEY_PAN);
    else if (e.key === "ArrowDown") pan(0, -KEY_PAN);
    else return;
    e.preventDefault();
  };

  /**
   * Fokus papan ketik ke kartu di luar layar: geser kanvas ke kartu itu, dan batalkan gulir bawaan peramban. Fokus dari
   * penunjuk (Chrome memfokuskan tombol saat diketuk) tidak menggeser: ketukan di kartu yang terlihat sebagian harus
   * membuka kartunya, bukan memindahkan peta dari bawah jari.
   */
  const onFocusNode = (n: PlacedNode) => {
    const vp = viewportRef.current;
    if (vp) {
      vp.scrollLeft = 0;
      vp.scrollTop = 0;
    }
    if (pointerInput.current) return;
    const { size: s } = latest.current;
    const v = viewRef.current;
    if (!s.width) return;
    const left = n.x * v.scale + v.x;
    const top = n.y * v.scale + v.y;
    if (left >= 0 && top >= 0 && left + n.w * v.scale <= s.width && top + n.h * v.scale <= s.height) return;
    autoFit.current = false;
    commit(clampView({ scale: v.scale, x: s.width / 2 - (n.x + n.w / 2) * v.scale, y: s.height / 2 - (n.y + n.h / 2) * v.scale }));
  };

  // ---- layar penuh ----
  const enterFull = (opener: HTMLElement) => {
    fullOpener.current = opener;
    history.pushState({ ...(history.state as object | null), mindmap: true }, "");
    fullPushed.current = true;
    setFull(true);
  };
  const exitFull = useCallback(() => {
    if (fullPushed.current) {
      fullPushed.current = false;
      history.back();
    }
    setFull(false);
  }, []);

  useEffect(() => {
    if (!full) return;
    // Gestur kembali peramban menutup layar penuh, bukan meninggalkan halaman.
    const onPop = () => {
      fullPushed.current = false;
      setFull(false);
    };
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, [full]);

  // Layar penuh adalah dialog modal: fokus masuk dan terkurung, latar (nav, halaman) `inert`, kunci gulir, dan Esc menutupnya
  // hanya bila tidak ada sheet di atasnya (tumpukan modal: sheet yang dibuka dari sini menutup lebih dulu).
  useModal(overlayRef, full, exitFull, { returnFocusTo: fullOpener.current, lockScroll: true });

  /** Perkecil tidak bisa lebih kecil dari skala terbaca: tombolnya tetap fokus-able dan 44 px, hanya diredupkan. */
  const atMin = view.scale <= MIN_SCALE + 1e-9;
  const selectedNode = selected.value ? placedById.get(selected.value) : undefined;
  const closeSheet = selected.close;
  const afterSheet = (run: () => void) => {
    closeSheet();
    run();
  };

  return (
    <div className="flex min-w-0 flex-col gap-3">
      {/*
        Di ponsel nav bawah (tetap) dan FAB "Catat transaksi" yang menonjol 26 px di atasnya menutupi tepi bawah layar.
        Tinggi peta dan ruang di bawahnya menyisakan nav + area aman + tonjolan FAB + 8 px, jadi keterangan warna di
        dasar peta dan kartu di kanvas tidak tertutup FAB, baik saat peta digulir ke puncak layar maupun ke ujungnya.
      */}
      <div className="pb-[calc(var(--nav-height)+var(--nav-bottom)+34px)] lg:pb-0">
        <div
          ref={overlayRef}
          data-mindmap=""
          data-fullscreen={full ? "" : undefined}
          role={full ? "dialog" : undefined}
          aria-modal={full ? true : undefined}
          aria-label={full ? "Peta struktur bisnis, layar penuh" : undefined}
          tabIndex={full ? -1 : undefined}
          className={cn(
            "flex flex-col overflow-hidden border border-border-hairline bg-surface-sunken",
            full ? "fixed inset-0 z-30 rounded-none pb-[env(safe-area-inset-bottom)] pt-[env(safe-area-inset-top)] outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent" : "relative h-[max(420px,calc(100svh-var(--nav-height)-var(--nav-bottom)-50px))] rounded-md lg:h-[560px]",
          )}
        >
          {/* Satu baris di ponsel: tombol ikon 44 px dengan nama dan tooltip; kata hanya di kanvas lebar. */}
          <div className={cn("flex shrink-0 items-center border-b border-border-hairline bg-bg", narrow ? "gap-x-0 px-1" : "flex-wrap gap-x-1 px-2")}>
            <button type="button" aria-label="Perbesar" title="Perbesar" onClick={() => zoomBy(KEY_ZOOM)} className={TOOL}>
              <Plus aria-hidden="true" size={20} strokeWidth={2} />
            </button>
            <button type="button" aria-label="Perkecil" title="Perkecil" aria-disabled={atMin || undefined} onClick={() => zoomBy(1 / KEY_ZOOM)} className={cn(TOOL, atMin && "opacity-40")}>
              <Minus aria-hidden="true" size={20} strokeWidth={2} />
            </button>
            <button type="button" aria-label="Pas layar" title="Pas layar" onClick={fitToScreen} className={TOOL}>
              <Scan aria-hidden="true" size={20} strokeWidth={2} />
              {narrow ? null : "Pas layar"}
            </button>
            {expandable.length ? (
              <button type="button" onClick={toggleAll} className={cn(TOOL, "whitespace-nowrap")}>
                {allOpen ? "Tutup semua" : "Buka semua"}
              </button>
            ) : null}
            <span aria-hidden="true" className="num ml-auto hidden px-2 text-caption text-text-2 sm:inline">{`${Math.round(view.scale * 100)}%`}</span>
            {narrow ? (
              <button type="button" aria-label="Warna" title="Warna" aria-expanded={legendOpen} aria-controls={`${uid}-legend`} onClick={() => setLegendOpen((o) => !o)} className={cn(TOOL, "ml-auto", legendOpen && "bg-surface-sunken")}>
                <Palette aria-hidden="true" size={20} strokeWidth={2} />
              </button>
            ) : null}
            {full ? (
              <button type="button" aria-label="Tutup" title="Tutup" onClick={exitFull} className={cn(TOOL, !narrow && "ml-auto sm:ml-0")}>
                <X aria-hidden="true" size={18} strokeWidth={2} />
                {narrow ? null : "Tutup"}
              </button>
            ) : (
              <button type="button" aria-label="Layar penuh" title="Layar penuh" onClick={(e) => enterFull(e.currentTarget)} className={cn(TOOL, !narrow && "ml-auto sm:ml-0")}>
                <Maximize2 aria-hidden="true" size={18} strokeWidth={2} />
                {narrow ? null : "Layar penuh"}
              </button>
            )}
          </div>

          <div
            ref={viewportRef}
            data-mindmap-viewport=""
            role="group"
            aria-label="Kanvas peta struktur bisnis. Cubit untuk memperbesar, seret untuk menggeser. Tombol panah menggeser, plus dan minus memperbesar, nol pas layar."
            tabIndex={0}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerEnd}
            onPointerCancel={onPointerEnd}
            onClickCapture={(e) => {
              // Setelah geser atau cubit, jangan sampai lepas jari di atas kartu terbaca sebagai ketukan.
              if (suppressClick.current) {
                e.preventDefault();
                e.stopPropagation();
                suppressClick.current = false;
              }
            }}
            onKeyDown={onKeyDown}
            className="relative min-h-0 flex-1 touch-none select-none overflow-hidden outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent"
          >
            <div
              data-mindmap-layer=""
              style={{
                position: "absolute",
                left: 0,
                top: 0,
                width: layout.width,
                height: layout.height,
                transformOrigin: "0 0",
                transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})`,
                transition: animate ? "transform 180ms ease-out" : "none",
                willChange: "transform",
              }}
            >
              <svg width={layout.width} height={layout.height} className="absolute left-0 top-0" aria-hidden="true">
                {layout.edges.map((edge) => (
                  <path
                    key={edge.id}
                    d={edge.d}
                    fill="none"
                    stroke={KIND_COLOR[edge.kind]}
                    strokeWidth={3}
                    strokeLinecap="round"
                    strokeDasharray={placedById.get(edge.id)?.node.state === "proposed" ? "7 6" : undefined}
                  />
                ))}
              </svg>

              <div
                data-mindmap-centre=""
                style={{ left: layout.centre.x, top: layout.centre.y, width: layout.centre.w, height: layout.centre.h }}
                className="absolute flex flex-col justify-center gap-0.5 overflow-hidden rounded-lg bg-surface-inverse px-3 py-2.5 text-text-on-accent shadow-float"
              >
                <span className="text-stat">{ticker}</span>
                {name ? <span className="line-clamp-2 break-words text-label font-bold">{name}</span> : null}
                {note ? <span className="line-clamp-3 break-words text-caption opacity-80">{displayText(note)}</span> : null}
              </div>

              {layout.nodes.map((n) => (
                <NodeCard key={n.id} n={n} uid={uid} compact={narrow} metric={metricText(n.node)} onFocus={onFocusNode} onOpen={(el) => { opener.current = el; selected.open(n.id, el); }} onToggle={toggle} />
              ))}
            </div>
          </div>

          {full && props.error ? (
            <p role="alert" className="shrink-0 border-t border-border-hairline bg-bg px-3 py-2 text-caption text-status-over">
              {props.error}
            </p>
          ) : null}

          {/* Di ponsel keterangan warna tertutup (tombol "Warna" di toolbar) dan terbuka sebagai lapisan di atas kanvas, jadi kanvas tidak menyusut. */}
          {!narrow || legendOpen ? (
            <ul
              id={`${uid}-legend`}
              aria-label="Keterangan warna"
              className={cn(
                "flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-border-hairline bg-bg px-3 py-2 text-caption text-text-2",
                narrow ? "absolute inset-x-0 bottom-0 z-10 shadow-float" : "shrink-0",
              )}
            >
              {kinds.map(([kind, label]) => (
                <li key={kind} className="inline-flex items-center gap-1.5">
                  <span aria-hidden="true" className="inline-block h-1 w-5 rounded-pill" style={{ background: KIND_COLOR[kind] }} />
                  {label}
                </li>
              ))}
              <li className="inline-flex items-center gap-1.5">
                <svg aria-hidden="true" width="20" height="4" className="shrink-0">
                  <line x1="0" y1="2" x2="20" y2="2" stroke="currentColor" strokeWidth="2" strokeDasharray="4 3" />
                </svg>
                Garis putus: usulan
              </li>
            </ul>
          ) : null}

          {/* Pengumuman layar penuh. Selalu dirender (wilayah status yang baru muncul tidak diumumkan) dan di dalam bingkai ini, supaya tetap terjangkau saat layar penuh membuat sisa halaman inert; di luar layar penuh wilayah milik induk yang dipakai. */}
          <div role="status" aria-live="polite" aria-atomic="true" data-mindmap-status="" className="sr-only">
            {status.n % 2 === 1 ? `${status.text} ` : status.text}
          </div>
        </div>
      </div>

      <RejectedList rejected={rejected} busyId={props.busyId} onAccept={props.onAccept} />

      {selectedNode ? (
        <MindMapNodeSheet
          node={selectedNode.node}
          depth={selectedNode.depth}
          maxDepth={props.maxDepth}
          color={KIND_COLOR[selectedNode.node.kind]}
          metric={props.renderMetric(selectedNode.node)}
          onAccept={props.onAccept}
          onReject={props.onReject}
          onEdit={(node) => afterSheet(() => props.onEdit(node, opener.current ?? document.body))}
          onAddChild={(node) => afterSheet(() => props.onAddChild(node, opener.current ?? document.body))}
          onClose={closeSheet}
        />
      ) : null}
    </div>
  );
}

const centreOf = (r: Box): Point => ({ x: r.x + r.w / 2, y: r.y + r.h / 2 });

/** Lebar yang dipakai tata letak: lebar kanvas bila sempit (peta satu sisi), selain itu `null` (dua sisi, atau belum terukur). */
const canvasWidthOf = (s: Size): number | null => (isNarrow(s.width) ? Math.round(s.width) : null);

/** Tampilan awal: kanvas sempit = kartu perusahaan di tepi kiri; lebar = kartu perusahaan di tengah. */
const homeView = (l: MindMapLayout, s: Size): View => (isNarrow(s.width) ? startViewLeft(l, s) : startView(l, s, centreOf(l.centre)));

/** Kartu `card` (simpul `id`) dan anak-anaknya yang tergambar di `layout`: yang harus terlihat setelah `id` dibuka. */
const boxAround = (card: Box, id: string | null, layout: MindMapLayout): Box => {
  const rects: Box[] = [card, ...layout.nodes.filter((c) => c.parent === id)];
  const x = Math.min(...rects.map((r) => r.x));
  const y = Math.min(...rects.map((r) => r.y));
  return { x, y, w: Math.max(...rects.map((r) => r.x + r.w)) - x, h: Math.max(...rects.map((r) => r.y + r.h)) - y };
};

interface NodeCardProps {
  n: PlacedNode;
  uid: string;
  /** Kanvas sempit: kartu 80 px, label dua baris, satu baris di bawahnya (nilai metrik, atau catatan bila tak ada); anak tampil di bawah induk. */
  compact: boolean;
  metric: string | null;
  onFocus: (n: PlacedNode) => void;
  onOpen: (el: HTMLElement) => void;
  onToggle: (id: string) => void;
}

/** Kartu simpul (membuka sheet) dan, bila punya anak, tombol buka / tutup cabang di sisi luarnya (saudara, bukan di dalam tombol kartu). */
function NodeCard({ n, uid, compact, metric, onFocus, onOpen, onToggle }: NodeCardProps) {
  const { node } = n;
  const proposed = node.state === "proposed";
  const left = n.side === "left";
  const color = KIND_COLOR[node.kind];
  const id = (part: string) => `${uid}-${n.id}-${part}`;
  // Kartu ringkas memuat satu baris di bawah label: nilai metrik bila ada, kalau tidak catatan. Sisanya ada di sheet.
  const showNote = Boolean(node.note) && !(compact && metric);
  // Nama kartu = teks yang terlihat, urutan terlihat: jenis, PEMILIK, USULAN, lalu label (WCAG 2.5.3). Label tidak dibaca dua kali; catatan dan nilai metrik jadi deskripsi.
  const named = [id("kindlabel"), node.owner ? id("owner") : null, proposed ? id("proposed") : null, id("label")].filter(Boolean).join(" ");
  const described = [showNote ? id("note") : null, metric ? id("metric") : null].filter(Boolean).join(" ") || undefined;
  // Panah menunjuk ke arah anak saat tertutup (menjauh dari pusat; di kanvas sempit ke bawah), dan ke arah induk saat terbuka.
  const Chevron = compact ? (n.expanded ? ChevronUp : ChevronDown) : left === n.expanded ? ChevronRight : ChevronLeft;
  return (
    <>
      <button
        type="button"
        data-node={node.id}
        data-state={node.state}
        data-mm-node=""
        aria-labelledby={named}
        aria-describedby={described}
        onClick={(e) => onOpen(e.currentTarget)}
        onFocus={() => onFocus(n)}
        style={{ left: n.x, top: n.y, width: n.w, height: n.h, borderColor: color, borderStyle: proposed ? "dashed" : "solid" }}
        className={cn(
          "absolute flex flex-col overflow-hidden rounded-md border-2 bg-bg px-2 shadow-float outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2",
          compact ? "gap-0 py-1" : "gap-0.5 py-1.5",
          left ? "items-end text-right" : "text-left",
        )}
      >
        <span className={cn("flex w-full min-w-0 shrink-0 items-center gap-1.5 text-axis font-semibold text-text-2", compact && "h-4", left && "flex-row-reverse")}>
          <span aria-hidden="true" className="inline-block h-2 w-2 shrink-0 rounded-full" style={{ background: color }} />
          <span id={id("kindlabel")} className="min-w-0 truncate">{node.kind_label}</span>
          {node.owner ? <span id={id("owner")} className={CARD_BADGE}>PEMILIK</span> : null}
          {proposed ? <span id={id("proposed")} className={CARD_BADGE}>USULAN</span> : null}
        </span>
        <span id={id("label")} className={cn("line-clamp-2 w-full shrink-0 break-words text-label font-bold text-text", compact && "leading-[17px]")}>
          {displayText(node.label)}
        </span>
        {showNote ? (
          <span id={id("note")} className={cn("w-full break-words text-caption text-text-2", compact ? "line-clamp-1 leading-4" : "line-clamp-2")}>
            {displayText(node.note ?? "")}
          </span>
        ) : null}
        {metric ? (
          <span id={id("metric")} className={cn("num w-full shrink-0 truncate text-caption font-semibold text-text", compact ? "leading-4" : "mt-auto")}>
            {metric}
          </span>
        ) : null}
      </button>
      {n.childCount > 0 ? (
        <button
          type="button"
          data-mm-toggle={node.id}
          aria-expanded={n.expanded}
          aria-label={`+${n.childCount} cabang di ${displayText(node.label)}`}
          onClick={() => onToggle(n.id)}
          onFocus={() => onFocus(n)}
          style={{ left: left ? n.x - 4 - TOGGLE : n.x + n.w + 4, top: n.y + (n.h - TOGGLE) / 2, width: TOGGLE, height: TOGGLE, borderColor: color, color }}
          className="absolute flex min-h-11 min-w-11 flex-col items-center justify-center rounded-md border-2 bg-bg text-caption font-bold leading-none shadow-float outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2"
        >
          {n.expanded ? null : <span aria-hidden="true" className="num">{`+${n.childCount}`}</span>}
          <Chevron aria-hidden="true" size={18} strokeWidth={2.5} />
        </button>
      ) : null}
    </>
  );
}
