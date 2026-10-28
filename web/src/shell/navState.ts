/**
 * Navigation state (design.md §2.4, D8). `<HashRouter>` has no `<ScrollRestoration>`, so per-tab memory and scroll
 * live here: in memory only, a cold start opens the deep link (or `#/beranda`) with nothing remembered.
 * A "hash" below is the route without `#`: pathname + search, e.g. `/perusahaan/BUMI?from=portofolio`.
 */

export type Tab = "beranda" | "portofolio" | "perusahaan" | "alat";

export const TABS: readonly Tab[] = ["beranda", "portofolio", "perusahaan", "alat"];

export const TAB_ROOTS: Record<Tab, string> = {
  beranda: "/beranda",
  portofolio: "/portofolio",
  perusahaan: "/perusahaan",
  alat: "/alat",
};

/** At most this many saved scroll positions; the oldest is dropped (rule 2). */
export const SCROLL_CAP = 50;

const isTab = (value: string | null | undefined): value is Tab => TABS.includes(value as Tab);

function split(hash: string): { pathname: string; search: string } {
  const q = hash.indexOf("?");
  return q < 0 ? { pathname: hash, search: "" } : { pathname: hash.slice(0, q), search: hash.slice(q) };
}

export const locationHash = (location: { pathname: string; search: string }) => location.pathname + location.search;

/** The route's own tab (design.md §2.1 table), from the first path segment. */
export function routeTab(pathname: string): Tab | null {
  const first = pathname.split("/")[1];
  return isTab(first) ? first : null;
}

/** Rule 3: the `from` value when it names a tab, else the route's own tab. */
export function owner(hash: string): Tab | null {
  const { pathname, search } = split(hash);
  const from = new URLSearchParams(search).get("from");
  return isTab(from) ? from : routeTab(pathname);
}

/** A pushed screen (company page, Tambah, the tools) sits one level below a tab root and shows the back control. */
export function isPushed(pathname: string): boolean {
  const segments = pathname.split("/").filter(Boolean);
  return segments.length > 1 && routeTab(pathname) !== null;
}

export interface NavStore {
  lastHash(tab: Tab): string | undefined;
  /** The tab's own level (its root, with any query) last visited; rule 8 goes back to it. */
  rootHash(tab: Tab): string | undefined;
  /** Owner of the history entry at `idx` (React Router's `history.state.idx`) when this store rendered it. */
  entryOwner(idx: number): Tab | undefined;
  /** Rule 5: the visited hash becomes its owner's `lastHash`; `idx` records whose history entry it is (rule 8). */
  visit(hash: string, idx?: number): void;
  savedScroll(hash: string): number | undefined;
  saveScroll(hash: string, y: number): void;
  forgetScroll(hash: string): void;
  size(): number;
}

export function createNavStore(cap = SCROLL_CAP): NavStore {
  const last: Partial<Record<Tab, string>> = {};
  const root: Partial<Record<Tab, string>> = {};
  // A push at idx n drops the browser's entries above n, and the next render at n + 1 overwrites ours.
  const entries = new Map<number, Tab>();
  // Map keeps insertion order: the first key is the oldest.
  const scroll = new Map<string, number>();
  return {
    lastHash: (tab) => last[tab],
    rootHash: (tab) => root[tab],
    entryOwner: (idx) => entries.get(idx),
    visit(hash, idx) {
      const tab = owner(hash);
      if (idx !== undefined) {
        if (tab) entries.set(idx, tab);
        else entries.delete(idx);
      }
      if (!tab) return;
      last[tab] = hash;
      if (!isPushed(split(hash).pathname)) root[tab] = hash;
    },
    savedScroll: (hash) => scroll.get(hash),
    saveScroll(hash, y) {
      scroll.delete(hash);
      scroll.set(hash, y);
      while (scroll.size > cap) scroll.delete(scroll.keys().next().value as string);
    },
    forgetScroll(hash) {
      scroll.delete(hash);
    },
    size: () => scroll.size,
  };
}

/**
 * Rules 5 and 10: where to scroll after rendering `hash`. The saved `scrollY`, else 0; `null` (leave it to the screen)
 * on the first render of a hash with `?section=`, which scrolls to its block itself.
 */
export function restoreScroll(store: NavStore, hash: string): number | null {
  const saved = store.savedScroll(hash);
  if (saved !== undefined) return saved;
  return new URLSearchParams(split(hash).search).has("section") ? null : 0;
}

/** Rules 6 and 7: a tap on another tab goes to its `lastHash` (root if empty); a tap on the active tab goes to its root. */
export function tabTarget(store: NavStore, tab: Tab, active: Tab | null): string {
  return tab === active ? TAB_ROOTS[tab] : (store.lastHash(tab) ?? TAB_ROOTS[tab]);
}

/**
 * Rules 8 and 9: `-1` when the previous history entry (React Router's `history.state.idx - 1`) was rendered by this
 * app with the same owner tab; else (a cold deep link, or a screen reached again through another tab's memory) the
 * owner tab's stored route, its root if none. The caller replaces the entry with it; scroll comes back via rule 5.
 */
export function backTarget(store: NavStore, tab: Tab, historyState: unknown): -1 | string {
  const idx = (historyState as { idx?: unknown } | null | undefined)?.idx;
  if (typeof idx === "number" && idx > 0 && store.entryOwner(idx - 1) === tab) return -1;
  return store.rootHash(tab) ?? TAB_ROOTS[tab];
}

/** The app's one store. */
export const navStore = createNavStore();
