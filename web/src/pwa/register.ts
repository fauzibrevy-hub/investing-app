/**
 * Registrasi service worker + alur "Versi baru tersedia · Muat ulang" (design.md §10).
 * Versi baru menunggu (waiting) sampai semua tab ditutup, atau sampai pengguna mengetuk "Muat ulang".
 */
type Listener = (available: boolean) => void;

let waiting: ServiceWorker | null = null;
let reloadRequested = false;
const listeners = new Set<Listener>();

function setWaiting(worker: ServiceWorker | null) {
  waiting = worker;
  listeners.forEach((l) => l(worker !== null));
}

export function onUpdateAvailable(listener: Listener): () => void {
  listeners.add(listener);
  listener(waiting !== null);
  return () => listeners.delete(listener);
}

/** Aktifkan versi baru lalu muat ulang halaman. */
export function applyUpdate(): void {
  if (!waiting) return;
  reloadRequested = true;
  waiting.postMessage({ type: "SKIP_WAITING" });
}

export function registerServiceWorker(): void {
  if (!("serviceWorker" in navigator)) return;

  navigator.serviceWorker.addEventListener("controllerchange", () => {
    if (reloadRequested) window.location.reload();
  });

  window.addEventListener("load", async () => {
    try {
      const reg = await navigator.serviceWorker.register("/sw.js", { scope: "/" });
      if (reg.waiting && navigator.serviceWorker.controller) setWaiting(reg.waiting);
      reg.addEventListener("updatefound", () => {
        const worker = reg.installing;
        worker?.addEventListener("statechange", () => {
          // Ada controller lama = ini pembaruan, bukan instalasi pertama.
          if (worker.state === "installed" && navigator.serviceWorker.controller) setWaiting(worker);
        });
      });
    } catch {
      // Tanpa SW app tetap jalan (mis. origin http non-localhost); tidak ada yang perlu ditampilkan.
    }
  });
}
