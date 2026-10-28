import { describe, expect, it } from "vitest";
import { DEFAULT_LIMITS, fitsReadable, fitView, keepInView, MAX_SCALE, MIN_SCALE, pinchView, READABLE_SCALE, revealBox, shiftToKeep, startView, startViewLeft, toWorld, zoomAround, type View } from "./mindmapView";

const view: View = { scale: 0.8, x: 30, y: -40 };

describe("zoom maths", () => {
  it("zoomAround keeps the world point under the anchor fixed", () => {
    const anchor = { x: 210, y: 130 };
    const before = toWorld(view, anchor);
    const next = zoomAround(view, 2.1, anchor);
    const after = toWorld(next, anchor);
    expect(next.scale).toBe(2.1);
    expect(after.x).toBeCloseTo(before.x, 9);
    expect(after.y).toBeCloseTo(before.y, 9);
  });

  it("clamps the scale to the limits and the anchor still holds at the clamp", () => {
    const anchor = { x: 50, y: 60 };
    const up = zoomAround(view, 99, anchor);
    expect(up.scale).toBe(MAX_SCALE);
    expect(toWorld(up, anchor).x).toBeCloseTo(toWorld(view, anchor).x, 9);
    expect(zoomAround(view, 0.001, anchor).scale).toBe(MIN_SCALE);
  });

  it("a pinch keeps the world point under the fingers' midpoint fixed while the midpoint moves", () => {
    const startMid = { x: 180, y: 220 };
    const mid = { x: 200, y: 190 };
    const world = toWorld(view, startMid);
    const next = pinchView(view, startMid, 100, mid, 220);
    expect(next.scale).toBeCloseTo(0.8 * 2.2, 9);
    const onScreen = { x: world.x * next.scale + next.x, y: world.y * next.scale + next.y };
    expect(onScreen.x).toBeCloseTo(mid.x, 9);
    expect(onScreen.y).toBeCloseTo(mid.y, 9);
  });

  it("a pinch that does not move the midpoint behaves like zoomAround at that midpoint", () => {
    const mid = { x: 120, y: 90 };
    const pinched = pinchView(view, mid, 80, mid, 160);
    const zoomed = zoomAround(view, 1.6, mid);
    expect(pinched.x).toBeCloseTo(zoomed.x, 9);
    expect(pinched.y).toBeCloseTo(zoomed.y, 9);
  });
});

describe("fit and pan limits", () => {
  it("fits a wide map inside the viewport, centred, and never above 1:1", () => {
    const fit = fitView({ width: 2400, height: 1500 }, { width: 390, height: 600 });
    expect(fit.scale).toBeCloseTo((390 - 16) / 2400, 9);
    expect(fit.x).toBeCloseTo((390 - 2400 * fit.scale) / 2, 9);
    expect(fit.y).toBeCloseTo((600 - 1500 * fit.scale) / 2, 9);
    expect(fitView({ width: 200, height: 100 }, { width: 800, height: 600 }).scale).toBe(1);
  });

  it("never zooms below the readable scale: buttons, pinch and wheel all stop where card text is 12 px and chips 44 px", () => {
    expect(MIN_SCALE).toBe(READABLE_SCALE);
    expect(DEFAULT_LIMITS).toEqual({ min: READABLE_SCALE, max: MAX_SCALE });
    const at: View = { scale: 1.5, x: 10, y: 20 };
    expect(zoomAround(at, 0.2, { x: 100, y: 100 }).scale).toBe(READABLE_SCALE);
    expect(pinchView(at, { x: 100, y: 100 }, 200, { x: 100, y: 100 }, 20).scale).toBe(READABLE_SCALE);
    // A map far bigger than the screen does not lower the floor any more (it used to drop to the fit scale, 0.16 here).
    expect(fitView({ width: 2400, height: 1500 }, { width: 390, height: 600 }).scale).toBeLessThan(0.2);
  });

  it("keeps at least the margin of the map on screen", () => {
    const content = { width: 1000, height: 800 };
    const viewport = { width: 400, height: 500 };
    const far = keepInView({ scale: 1, x: -5000, y: 9000 }, content, viewport, 80);
    expect(far.x).toBe(80 - 1000);
    expect(far.y).toBe(500 - 80);
    expect(keepInView(view, content, viewport)).toEqual(view);
  });
});

describe("start view, anchoring and reveal", () => {
  const viewport = { width: 393, height: 560 };

  it("opens at the readable scale around the focus when the map is wider than the screen, never below it", () => {
    const content = { width: 560, height: 420 };
    expect(fitView(content, viewport).scale).toBeLessThan(READABLE_SCALE);
    const focus = { x: 280, y: 210 };
    const start = startView(content, viewport, focus);
    expect(start.scale).toBeGreaterThanOrEqual(READABLE_SCALE);
    expect(start.scale).toBe(READABLE_SCALE);
    // The focus point sits in the middle of the viewport.
    expect(focus.x * start.scale + start.x).toBeCloseTo(viewport.width / 2, 9);
    expect(focus.y * start.scale + start.y).toBeCloseTo(viewport.height / 2, 9);
  });

  it("a map that already fits at the readable scale is simply fitted and centred", () => {
    const content = { width: 300, height: 200 };
    expect(startView(content, viewport, { x: 0, y: 0 })).toEqual(fitView(content, viewport));
  });

  it("startViewLeft opens at the readable scale with the map at the left edge, never panning a map that fits sideways", () => {
    const fits = startViewLeft({ width: 361, height: 400 }, { width: 361, height: 560 });
    expect(fits).toEqual({ scale: READABLE_SCALE, x: 0, y: 80 });
    // Taller than the viewport: the view starts at the top of the stack (no card above the visible area), x at the left edge.
    const tall = startViewLeft({ width: 361, height: 900 }, { width: 361, height: 560 });
    expect(tall).toEqual({ scale: READABLE_SCALE, x: 0, y: 0 });
    // Wider than the viewport: still starts at the left edge.
    expect(startViewLeft({ width: 500, height: 300 }, { width: 361, height: 560 }).x).toBe(0);
  });

  it("fitsReadable says whether the whole map fits at the readable scale, so Pas layar can say the rest is panned", () => {
    expect(fitsReadable({ width: 361, height: 400 }, { width: 361, height: 560 }, 0)).toBe(true);
    expect(fitsReadable({ width: 361, height: 2600 }, { width: 361, height: 560 }, 0)).toBe(false);
    expect(fitsReadable({ width: 900, height: 400 }, { width: 800, height: 560 })).toBe(false);
    expect(fitsReadable({ width: 300, height: 200 }, { width: 800, height: 560 })).toBe(true);
  });

  it("revealBox with atStart puts the left / top edge of a box that is bigger than the screen at the margin", () => {
    const at: View = { scale: 1, x: 0, y: 0 };
    const wide = revealBox(at, { x: 140, y: 60, w: 400, h: 80 }, viewport, { x: 200, y: 100 }, 12, true);
    expect(wide.x).toBe(12 - 140);
    expect(revealBox(at, { x: 140, y: 60, w: 400, h: 80 }, viewport, { x: 200, y: 100 }).x).toBe(viewport.width / 2 - 200);
    // A tall opened branch on the phone (parent card on top, children below): the parent goes to the top margin, not the middle.
    const tall = revealBox(at, { x: 146, y: 700, w: 200, h: 900 }, viewport, { x: 246, y: 740 }, 12, true);
    expect(tall.y).toBe(12 - 700);
    expect(revealBox(at, { x: 146, y: 700, w: 200, h: 900 }, viewport, { x: 246, y: 740 }).y).toBe(viewport.height / 2 - 740);
  });

  it("shiftToKeep puts the old and the new position of a card on the same screen point", () => {
    const from = { x: 120, y: 340 };
    const to = { x: 400, y: 90 };
    const onScreen = (p: { x: number; y: number }, v: View) => ({ x: p.x * v.scale + v.x, y: p.y * v.scale + v.y });
    const next = shiftToKeep(view, from, to);
    expect(next.scale).toBe(view.scale);
    expect(onScreen(to, next).x).toBeCloseTo(onScreen(from, view).x, 9);
    expect(onScreen(to, next).y).toBeCloseTo(onScreen(from, view).y, 9);
  });

  it("revealBox moves the view only as far as needed to show the whole box inside the margin", () => {
    const at: View = { scale: 1, x: 0, y: 0 };
    const inside = revealBox(at, { x: 50, y: 60, w: 100, h: 80 }, viewport, { x: 100, y: 100 });
    expect(inside).toEqual(at);
    const right = revealBox(at, { x: 300, y: 60, w: 200, h: 80 }, viewport, { x: 400, y: 100 }, 12);
    expect(right.x).toBe(viewport.width - 12 - 500);
    expect(right.y).toBe(0);
    const below = revealBox(at, { x: 50, y: 500, w: 100, h: 200 }, viewport, { x: 100, y: 600 }, 12);
    expect(below.y).toBe(viewport.height - 12 - 700);
  });

  it("revealBox centres the focus on an axis where the box is bigger than the screen", () => {
    const at: View = { scale: 1, x: 0, y: 0 };
    const tall = revealBox(at, { x: 50, y: 0, w: 100, h: 900 }, viewport, { x: 100, y: 430 });
    expect(tall.y).toBe(viewport.height / 2 - 430);
    expect(tall.x).toBe(0);
  });
});
