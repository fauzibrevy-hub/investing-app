import { useEffect, useRef, type RefObject } from "react";

/** Elemen yang bisa menerima fokus lewat Tab; yang berada di dalam `[hidden]` dibuang saat jebakan menghitung ujungnya. */
export const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export interface ModalEntry {
  /** Dialog itu sendiri: menerima fokus masuk, dan Tab berputar di dalamnya. */
  panel: HTMLElement;
  /** Bagian yang tetap interaktif saat latar `inert`: dialog beserta lapisan gelapnya (latar yang `inert` tidak menerima ketukan). */
  scope: HTMLElement;
  /** Dipanggil oleh Esc bila dialog ini yang teratas; `null` = Esc tidak ditangani di sini. */
  onEscape: (() => void) | null;
  /** Latar di luar `scope` dibuat `inert` selama dialog ini yang teratas. */
  inertBackground: boolean;
  lockScroll: boolean;
}

/**
 * Tumpukan dialog modal di seluruh aplikasi (urutan dibuka). Hanya yang teratas yang menerima Esc, Tab, dan fokus:
 * sheet yang dibuka dari layar penuh menutup lebih dulu, lalu layar penuhnya. Modul ini memegang satu pendengar
 * `keydown` selama tumpukan tidak kosong.
 */
const stack: ModalEntry[] = [];
/** Elemen yang kita beri `inert` (bukan yang sudah `inert` dari sumber lain), supaya hanya itu yang dilepas. */
const inerted = new Set<Element>();
let scrollLocks = 0;
let savedOverflow = "";

const SKIP_INERT = new Set(["SCRIPT", "STYLE", "TEMPLATE", "LINK", "META"]);

/** Latar = saudara dari `scope` dan dari tiap leluhurnya sampai `<body>`; dialog yang dirender di tempat (bukan portal) tetap hidup. */
function syncInert(): void {
  for (const el of inerted) el.removeAttribute("inert");
  inerted.clear();
  // Dialog tanpa latar inert (panel fakta tanpa lapisan gelap) di atas sheet tidak boleh membuka latar sheet itu lagi:
  // yang menentukan latar adalah dialog teratas yang meng-inert, dan dialog di atasnya tetap hidup.
  const ownerIndex = stack.map((e) => e.inertBackground).lastIndexOf(true);
  if (ownerIndex < 0) return;
  const owner = stack[ownerIndex];
  const above = stack.slice(ownerIndex + 1);
  let node: HTMLElement = owner.scope;
  for (;;) {
    const parent = node.parentElement;
    if (!parent) break;
    for (const sibling of Array.from(parent.children)) {
      if (sibling === node || SKIP_INERT.has(sibling.tagName) || sibling.hasAttribute("inert")) continue;
      if (above.some((e) => sibling.contains(e.scope))) continue;
      sibling.setAttribute("inert", "");
      inerted.add(sibling);
    }
    if (parent === document.body) break;
    node = parent;
  }
}

function focusables(panel: HTMLElement): HTMLElement[] {
  return Array.from(panel.querySelectorAll<HTMLElement>(FOCUSABLE)).filter((el) => !el.closest("[hidden]") && el.checkVisibility?.() !== false);
}

function onKeyDown(e: KeyboardEvent): void {
  const top = stack[stack.length - 1];
  if (!top) return;
  if (e.key === "Escape") {
    if (!top.onEscape || e.defaultPrevented) return;
    e.preventDefault();
    top.onEscape();
    return;
  }
  if (e.key !== "Tab") return;
  const { panel } = top;
  const items = focusables(panel);
  if (items.length === 0) {
    e.preventDefault();
    panel.focus();
    return;
  }
  const first = items[0];
  const last = items[items.length - 1];
  const current = document.activeElement;
  if (e.shiftKey && (current === first || current === panel || !panel.contains(current))) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && (current === last || !panel.contains(current))) {
    e.preventDefault();
    first.focus();
  }
}

/** Mendaftarkan dialog ke tumpukan; mengembalikan fungsi pelepas (aman dipanggil dua kali) yang mengabarkan apakah dialog itu yang teratas. */
export function pushModal(entry: ModalEntry): () => boolean {
  stack.push(entry);
  if (stack.length === 1) window.addEventListener("keydown", onKeyDown);
  if (entry.lockScroll) {
    if (scrollLocks === 0) {
      savedOverflow = document.body.style.overflow;
      document.body.style.overflow = "hidden";
    }
    scrollLocks += 1;
  }
  syncInert();
  return () => {
    const index = stack.indexOf(entry);
    if (index < 0) return false;
    const wasTop = index === stack.length - 1;
    stack.splice(index, 1);
    if (stack.length === 0) window.removeEventListener("keydown", onKeyDown);
    if (entry.lockScroll) {
      scrollLocks -= 1;
      if (scrollLocks === 0) document.body.style.overflow = savedOverflow;
    }
    syncInert();
    return wasTop;
  };
}

export interface ModalOptions {
  /** Pemicu yang menerima fokus kembali; tanpa itu elemen yang aktif tepat sebelum dialog dibuka. */
  returnFocusTo?: HTMLElement | null;
  /** Bagian yang tetap interaktif saat latar `inert`; bawaannya `panel`. `Sheet` memberi pembungkus yang memuat lapisan gelap. */
  scope?: RefObject<HTMLElement | null>;
  /** Esc menutup dialog ini bila ia yang teratas (bawaan `true`). */
  handleEscape?: boolean;
  /** Kunci gulir `<body>` selama dialog terbuka (sheet dan layar penuh; bawaan `false`). */
  lockScroll?: boolean;
}

/**
 * Dialog modal sungguhan: fokus masuk ke `panel` saat `active`, Tab dan Shift+Tab berputar di dalamnya, Esc menutup dialog
 * teratas saja, semua yang di luar dialog `inert` (pembaca layar dan papan ketik tidak bisa menjangkau latar), dan fokus
 * kembali ke pemicu saat ditutup. `onClose` boleh berganti identitas tiap render tanpa memasang ulang apa pun.
 */
export function useModal(panel: RefObject<HTMLElement | null>, active: boolean, onClose: () => void, options: ModalOptions = {}): void {
  const { scope, handleEscape = true, lockScroll = false } = options;
  const close = useRef(onClose);
  close.current = onClose;
  const returnTo = useRef(options.returnFocusTo);
  returnTo.current = options.returnFocusTo;

  // Elemen yang aktif sebelum dialog muncul dicatat saat render: efek baru jalan sesudah fokus pindah ke dalam dialog.
  const opener = useRef<HTMLElement | null>(null);
  const wasActive = useRef(false);
  if (active && !wasActive.current) opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  wasActive.current = active;

  useEffect(() => {
    const el = panel.current;
    if (!active || !el) return;
    const release = pushModal({
      panel: el,
      scope: scope?.current ?? el,
      onEscape: handleEscape ? () => close.current() : null,
      inertBackground: true,
      lockScroll,
    });
    el.focus();
    return () => {
      const wasTop = release();
      // Dialog yang tertutup di bawah dialog lain (mis. Back menutup layar penuh di bawah sheet) tidak boleh merebut fokus dialog di atasnya.
      const focus = document.activeElement;
      if (!wasTop && focus && focus !== document.body && !el.contains(focus)) return;
      const target = returnTo.current?.isConnected ? returnTo.current : opener.current?.isConnected ? opener.current : null;
      target?.focus?.();
    };
  }, [panel, scope, active, handleEscape, lockScroll]);
}
