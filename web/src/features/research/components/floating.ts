import { useLayoutEffect, useState, type CSSProperties, type RefObject } from "react";

const GAP = 4;
/** Jarak minimal panel ke tepi layar (px). */
const MARGIN = 8;
/** Tinggi minimal panel bila tombolnya sudah tergulir lewat tepi layar (px). */
const MIN_HEIGHT = 120;
/** Bilah bawah tetap di setiap lebar jendela (D13, design.md §2.1; tanpa FAB): `--nav-height` 56 px + 8 px jarak bawah. */
const NAV_RESERVE = 64;

export interface VerticalPlacement {
  side: "below" | "above";
  /** Tepi atas panel (`side: below`). */
  top?: number;
  /** Jarak tepi bawah panel ke bawah layar (`side: above`). */
  bottom?: number;
  maxHeight: number;
}

/**
 * Letak tegak panel: dibuka di sisi yang lebih lapang, tak pernah lebih dekat dari 8 px ke tepi layar, tingginya dibatasi
 * ruang di sisi itu (panel yang lebih tinggi menggulir di dalam). `bottomReserve` = bilah bawah tetap + area aman.
 */
export function placeVertically({ anchorTop, anchorBottom, viewportHeight, bottomReserve }: { anchorTop: number; anchorBottom: number; viewportHeight: number; bottomReserve: number }): VerticalPlacement {
  const floor = viewportHeight - Math.max(bottomReserve, MARGIN);
  const below = floor - anchorBottom - GAP;
  const above = anchorTop - GAP - MARGIN;
  if (below >= above) {
    const top = Math.max(MARGIN, Math.min(anchorBottom + GAP, floor - MIN_HEIGHT));
    return { side: "below", top, maxHeight: floor - top };
  }
  const edge = Math.max(MARGIN + MIN_HEIGHT, Math.min(anchorTop - GAP, floor));
  return { side: "above", bottom: viewportHeight - edge, maxHeight: edge - MARGIN };
}

/** Area aman bawah (px) lewat elemen sementara; 0 bila tak ada (jsdom, layar tanpa takik). */
function safeAreaBottom(): number {
  const probe = document.createElement("div");
  probe.style.cssText = "position:fixed;visibility:hidden;padding-bottom:env(safe-area-inset-bottom)";
  document.body.appendChild(probe);
  const px = parseFloat(getComputedStyle(probe).paddingBottom) || 0;
  probe.remove();
  return px;
}

/**
 * Posisi `fixed` untuk panel yang menempel pada tombolnya. Tabel paket dan keuangan menggulir mendatar
 * (`overflow-x-auto`, yang juga memotong arah tegak), jadi panel `absolute` di dalam sel terpotong di baris bawah.
 * Panel dibuka di sisi yang lebih lapang (`placeVertically`), tingginya dibatasi ruang di sisi itu dan tak masuk ke bawah
 * bilah navigasi; ikut bergeser saat halaman menggulir.
 * Tepi kiri dan kanan panel selalu di dalam layar: tombol di kolom sempit tidak mendorong panel ke luar layar. `panelWidth`
 * adalah lebar panel (px, tanpa batas layar); pemanggil memberi panelnya `w-[min(<lebar>px,calc(100vw-8px))]`.
 */
export function useFloatingPosition(anchor: RefObject<HTMLElement>, open: boolean, align: "start" | "end", panelWidth: number): CSSProperties {
  const [style, setStyle] = useState<CSSProperties>({});
  useLayoutEffect(() => {
    if (!open) return;
    const place = () => {
      const el = anchor.current;
      if (!el) return;
      const r = el.getBoundingClientRect();
      const vw = window.innerWidth;
      const vh = window.innerHeight;
      const bottomReserve = NAV_RESERVE + safeAreaBottom();
      const v = placeVertically({ anchorTop: r.top, anchorBottom: r.bottom, viewportHeight: vh, bottomReserve });
      const vertical: CSSProperties = { ...(v.side === "below" ? { top: v.top } : { bottom: v.bottom }), maxHeight: v.maxHeight };
      const width = Math.min(panelWidth, vw - 2 * GAP);
      const wanted = align === "end" ? r.right - width : r.left;
      const left = Math.max(GAP, Math.min(wanted, vw - width - GAP));
      setStyle({ position: "fixed", ...vertical, left });
    };
    place();
    window.addEventListener("scroll", place, { capture: true, passive: true });
    window.addEventListener("resize", place);
    return () => {
      window.removeEventListener("scroll", place, { capture: true });
      window.removeEventListener("resize", place);
    };
  }, [anchor, open, align, panelWidth]);
  return style;
}
