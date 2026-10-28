import { renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { placeVertically, useFloatingPosition } from "./floating";

const originalWidth = window.innerWidth;
const originalHeight = window.innerHeight;

function place(rect: { left: number; right: number }, vw: number, align: "start" | "end", panelWidth = 320) {
  Object.defineProperty(window, "innerWidth", { configurable: true, value: vw });
  Object.defineProperty(window, "innerHeight", { configurable: true, value: 852 });
  const anchor = document.createElement("button");
  anchor.getBoundingClientRect = () => ({ ...rect, top: 100, bottom: 144, width: rect.right - rect.left, height: 44, x: rect.left, y: 100, toJSON: () => ({}) });
  const ref = { current: anchor };
  const { result } = renderHook(() => useFloatingPosition(ref, true, align, panelWidth));
  return result.current;
}

describe("useFloatingPosition", () => {
  afterEach(() => {
    Object.defineProperty(window, "innerWidth", { configurable: true, value: originalWidth });
    Object.defineProperty(window, "innerHeight", { configurable: true, value: originalHeight });
  });

  it("keeps an end-aligned panel inside the left edge when its anchor sits in a narrow column at 393", () => {
    const style = place({ left: 190, right: 230 }, 393, "end");
    expect(style.left).toBe(4);
  });

  it("aligns the panel's right edge with the anchor when there is room", () => {
    const style = place({ left: 300, right: 380 }, 1280, "end");
    expect(style.left).toBe(60);
  });

  it("keeps a start-aligned panel inside the right edge", () => {
    const style = place({ left: 300, right: 340 }, 393, "start", 300);
    expect(style.left).toBe(393 - 300 - 4);
  });

  it("shrinks to the viewport when it is narrower than the panel", () => {
    const style = place({ left: 50, right: 90 }, 197, "end");
    expect(style.left).toBe(4);
  });

  it("caps the panel to the room below the anchor and keeps it clear of the bottom nav at 393", () => {
    const style = place({ left: 300, right: 380 }, 393, "end");
    expect(style.top).toBe(148);
    expect(style.maxHeight).toBe(852 - 106 - 148);
  });
});

describe("placeVertically", () => {
  const vh = 852;

  it("opens below when there is more room below the anchor", () => {
    const p = placeVertically({ anchorTop: 100, anchorBottom: 144, viewportHeight: vh, bottomReserve: 0 });
    expect(p).toEqual({ side: "below", top: 148, maxHeight: vh - 8 - 148 });
  });

  it("opens above when there is more room above the anchor", () => {
    const p = placeVertically({ anchorTop: 700, anchorBottom: 744, viewportHeight: vh, bottomReserve: 0 });
    expect(p).toEqual({ side: "above", bottom: vh - 696, maxHeight: 696 - 8 });
  });

  it("reserves the bottom nav when opening below", () => {
    const p = placeVertically({ anchorTop: 100, anchorBottom: 144, viewportHeight: vh, bottomReserve: 140 });
    expect(p.maxHeight).toBe(vh - 140 - 148);
  });

  it("keeps a panel taller than the viewport inside it, 8px from both edges, scrolling inside", () => {
    for (const anchorTop of [20, 300, 426, 600, 800]) {
      const p = placeVertically({ anchorTop, anchorBottom: anchorTop + 44, viewportHeight: vh, bottomReserve: 0 });
      const top = p.side === "below" ? p.top! : vh - p.bottom! - p.maxHeight;
      const bottom = top + p.maxHeight;
      expect(top).toBeGreaterThanOrEqual(8);
      expect(bottom).toBeLessThanOrEqual(vh - 8);
      expect(p.maxHeight).toBeGreaterThan(0);
    }
  });

  it("stays on screen when the anchor has scrolled out of view", () => {
    const gone = placeVertically({ anchorTop: 2000, anchorBottom: 2044, viewportHeight: vh, bottomReserve: 140 });
    expect(gone.side).toBe("above");
    expect(gone.maxHeight).toBeGreaterThanOrEqual(120);
    const up = placeVertically({ anchorTop: -900, anchorBottom: -856, viewportHeight: vh, bottomReserve: 0 });
    expect(up.side).toBe("below");
    expect(up.top).toBe(8);
  });
});
