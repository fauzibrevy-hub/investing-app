import "@testing-library/jest-dom/vitest";

// Polyfill API browser yang tidak ada di jsdom.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
window.ResizeObserver ??= ResizeObserverStub as unknown as typeof ResizeObserver;

if (typeof window !== "undefined" && !window.matchMedia) {
  window.matchMedia = (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener() {},
      removeEventListener() {},
      addListener() {},
      removeListener() {},
      dispatchEvent: () => false,
    }) as MediaQueryList;
}

// Node ≥ 25 punya global `localStorage`/`sessionStorage` sendiri (undefined tanpa --localstorage-file)
// yang menutupi milik jsdom. Pasang ulang storage jsdom.
const dom = (globalThis as unknown as { jsdom?: { window: Window } }).jsdom;
if (dom) {
  for (const key of ["localStorage", "sessionStorage"] as const) {
    Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
  }
}

// Bersihkan DOM antar test (globals: true tidak otomatis memanggil cleanup RTL di semua versi).
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";
afterEach(() => cleanup());

// jsdom belum punya PointerEvent; tanpa ini clientX/pointerId hilang di fireEvent.pointer*.
if (typeof window !== "undefined" && !window.PointerEvent) {
  class PointerEventPolyfill extends MouseEvent {
    pointerId: number;
    pointerType: string;
    constructor(type: string, init: PointerEventInit = {}) {
      super(type, init);
      this.pointerId = init.pointerId ?? 0;
      this.pointerType = init.pointerType ?? "mouse";
    }
  }
  window.PointerEvent = PointerEventPolyfill as unknown as typeof PointerEvent;
}
