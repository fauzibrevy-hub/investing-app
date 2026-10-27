import { describe, expect, it } from "vitest";
import { backTarget, createNavStore, isPushed, locationHash, owner, restoreScroll, routeTab, TAB_ROOTS, tabTarget, TABS } from "./navState";

describe("routeTab", () => {
  it("maps the first path segment to its tab (design.md §2.1)", () => {
    expect(routeTab("/beranda")).toBe("beranda");
    expect(routeTab("/portofolio")).toBe("portofolio");
    expect(routeTab("/perusahaan")).toBe("perusahaan");
    expect(routeTab("/perusahaan/tambah")).toBe("perusahaan");
    expect(routeTab("/perusahaan/BUMI")).toBe("perusahaan");
    expect(routeTab("/alat/kalibrasi")).toBe("alat");
  });

  it("returns null for routes outside the four tabs", () => {
    expect(routeTab("/")).toBeNull();
    expect(routeTab("/riset/BUMI")).toBeNull();
    expect(routeTab("/home")).toBeNull();
  });
});

describe("owner (rule 3)", () => {
  it("is the from value when it names a tab", () => {
    expect(owner("/perusahaan/BUMI?from=portofolio")).toBe("portofolio");
    expect(owner("/perusahaan/BUMI?section=klaim&from=beranda")).toBe("beranda");
    expect(owner("/alat/bandingkan?ticker=BUMI&from=perusahaan")).toBe("perusahaan");
  });

  it("falls back to the route's own tab when from is absent or not a tab (rule 9)", () => {
    expect(owner("/perusahaan/BUMI")).toBe("perusahaan");
    expect(owner("/perusahaan/BUMI?from=riset")).toBe("perusahaan");
    expect(owner("/alat/kalibrasi")).toBe("alat");
    expect(owner("/beranda")).toBe("beranda");
  });

  it("is null for an unknown route", () => {
    expect(owner("/nope")).toBeNull();
  });
});

describe("locationHash and isPushed", () => {
  it("joins pathname and search", () => {
    expect(locationHash({ pathname: "/perusahaan/BUMI", search: "?from=portofolio" })).toBe("/perusahaan/BUMI?from=portofolio");
    expect(locationHash({ pathname: "/beranda", search: "" })).toBe("/beranda");
  });

  it("treats the company page, Tambah and the three tools as pushed, tab roots as not", () => {
    for (const p of ["/perusahaan/tambah", "/perusahaan/BUMI", "/alat/bandingkan", "/alat/kalibrasi", "/alat/paket"]) expect(isPushed(p)).toBe(true);
    for (const p of ["/beranda", "/portofolio", "/perusahaan", "/alat", "/alat/"]) expect(isPushed(p)).toBe(false);
  });
});

describe("store: visit writes the owner's lastHash (rules 2, 3, 5)", () => {
  it("starts empty", () => {
    const store = createNavStore();
    for (const tab of TABS) expect(store.lastHash(tab)).toBeUndefined();
  });

  it("a pushed screen opened from Portofolio is Portofolio's memory, Perusahaan's is untouched", () => {
    const store = createNavStore();
    store.visit("/perusahaan?x=1");
    store.visit("/perusahaan/BUMI?from=portofolio");
    expect(store.lastHash("portofolio")).toBe("/perusahaan/BUMI?from=portofolio");
    expect(store.lastHash("perusahaan")).toBe("/perusahaan?x=1");
  });

  it("ignores hashes outside the four tabs", () => {
    const store = createNavStore();
    store.visit("/nope");
    for (const tab of TABS) expect(store.lastHash(tab)).toBeUndefined();
  });
});

describe("store: scroll save and restore (rules 5, 7, 10)", () => {
  it("restores the saved scrollY, 0 when nothing is saved", () => {
    const store = createNavStore();
    store.saveScroll("/perusahaan", 640);
    expect(restoreScroll(store, "/perusahaan")).toBe(640);
    expect(restoreScroll(store, "/beranda")).toBe(0);
  });

  it("overwrites an earlier value for the same hash", () => {
    const store = createNavStore();
    store.saveScroll("/beranda", 10);
    store.saveScroll("/beranda", 20);
    expect(store.savedScroll("/beranda")).toBe(20);
  });

  it("forgetScroll drops the saved value", () => {
    const store = createNavStore();
    store.saveScroll("/alat", 300);
    store.forgetScroll("/alat");
    expect(store.savedScroll("/alat")).toBeUndefined();
    expect(restoreScroll(store, "/alat")).toBe(0);
  });

  it("?section= wins on the first render of that hash, a saved scrollY wins afterwards", () => {
    const store = createNavStore();
    const hash = "/perusahaan/BUMI?section=klaim";
    expect(restoreScroll(store, hash)).toBeNull();
    store.saveScroll(hash, 900);
    expect(restoreScroll(store, hash)).toBe(900);
  });

  it("keeps at most 50 entries and drops the oldest", () => {
    const store = createNavStore();
    for (let i = 0; i < 51; i++) store.saveScroll(`/perusahaan/T${i}`, i);
    expect(store.size()).toBe(50);
    expect(store.savedScroll("/perusahaan/T0")).toBeUndefined();
    expect(store.savedScroll("/perusahaan/T1")).toBe(1);
    expect(store.savedScroll("/perusahaan/T50")).toBe(50);
  });

  it("re-saving a hash makes it the newest, so it outlives older ones", () => {
    const store = createNavStore(3);
    store.saveScroll("/a", 1);
    store.saveScroll("/b", 2);
    store.saveScroll("/a", 3);
    store.saveScroll("/c", 4);
    store.saveScroll("/d", 5);
    expect(store.savedScroll("/b")).toBeUndefined();
    expect(store.savedScroll("/a")).toBe(3);
  });
});

describe("tabTarget (rules 6, 7)", () => {
  it("another tab goes to its lastHash, or its root when empty", () => {
    const store = createNavStore();
    expect(tabTarget(store, "perusahaan", "beranda")).toBe(TAB_ROOTS.perusahaan);
    store.visit("/perusahaan/BUMI");
    expect(tabTarget(store, "perusahaan", "beranda")).toBe("/perusahaan/BUMI");
  });

  it("the active tab goes to its root", () => {
    const store = createNavStore();
    store.visit("/perusahaan/BUMI");
    expect(tabTarget(store, "perusahaan", "perusahaan")).toBe("/perusahaan");
  });
});

describe("backTarget (rules 8, 9)", () => {
  it("goes back one entry when the previous history entry is in this app", () => {
    expect(backTarget("portofolio", { idx: 3 })).toBe(-1);
  });

  it("goes to the owner tab's root on a cold deep link", () => {
    expect(backTarget("perusahaan", { idx: 0 })).toBe("/perusahaan");
    expect(backTarget("alat", null)).toBe("/alat");
    expect(backTarget("beranda", undefined)).toBe("/beranda");
    expect(backTarget("alat", { idx: "2" })).toBe("/alat");
  });
});
