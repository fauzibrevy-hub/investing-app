import { appendFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test as base, type BrowserContext, type Page, type TestInfo } from "@playwright/test";
import { axeSeriousCritical, horizontalOverflow, smallTargets, textCollisions } from "../e2e/audits";

/**
 * L4 step 7: the rows of `docs/iphone-checklist.md` that WebKit iPhone 15 emulation can cover, run by Hermes against the
 * LIVE sandbox (real data). Run and env: docs/qa/l4/README-live.md. One test per checklist row; each appends
 * `<row>\tPASS|FAIL|SKIP\t<reason>` to `$LIVE_SHOTS_DIR/results.tsv` and writes `<row>-<step>.png` (viewport) and
 * `<row>-<step>-full.png` (full page) next to it. Checks inside a row are soft, so one run lists every problem of the row.
 *
 * STRICTLY READ-ONLY. The only non-GET request that may leave the browser is the unlock POST (`/api/session`, plus
 * `LIVE_FA_UNLOCK_PATH` for finance-app in M17). Three layers stop anything else and fail the row: `page.route("**\/api/**")`
 * aborts it, a page-side wrapper of fetch/XHR/sendBeacon refuses it before it leaves the page (the route cannot see
 * requests a service worker handles, M2), and a `request` listener on the context flags any write that got through.
 * One exception is noted instead of failed: opening a company page makes the app POST `/api/research/<T>/seen` (a
 * "reviewed" marker). It is stopped like any write, so live data is untouched, and the row carries a note. The spec never
 * clicks a control that writes (Abaikan, Muat draf, Simpan, Tutup klaim, Tambah ...); it only opens and closes sheets.
 * The token is typed once into the unlock screen (before any screenshot) and scrubbed from every message written out.
 */
const TOKEN = process.env.LIVE_TOKEN ?? "";
const FA_URL = (process.env.LIVE_URL_FA ?? "").trim().replace(/\/+$/, "");
const FA_TOKEN = process.env.LIVE_TOKEN_FA ?? "";
const SHOTS = process.env.LIVE_SHOTS_DIR ?? "";
const T_A1 = (process.env.LIVE_TICKER_A1 || "BUMI").toUpperCase();
const T_A2 = (process.env.LIVE_TICKER_A2 || "BRIS").toUpperCase();
/** The company with the largest numbers (M7 claim sheet, M13, M2): BUMI unless the sandbox data says otherwise. */
const T_BIG = (process.env.LIVE_TICKER_BIG || "BUMI").toUpperCase();
const UNLOCK_PATHS = new Set(["/api/session", process.env.LIVE_FA_UNLOCK_PATH?.trim() || "/api/session"]);

const NAV = 'nav[aria-label="Navigasi utama"]';
const FONT_200 = "html{font-size:200% !important}";
const TABS = ["Beranda", "Portofolio", "Perusahaan", "Alat"] as const;

/* ------------------------------------------------------------------------------------------ plumbing */

function scrub(text: string): string {
  let out = text.replace(/\u001b\[[0-9;]*m/g, "");
  for (const secret of [TOKEN, FA_TOKEN]) if (secret.length >= 4) out = out.split(secret).join("[token]");
  return out;
}

/** What each layer saw: `observed` by the context's request listener, `stopped` by the route or the page-side wrapper. */
interface Guard {
  observed: Set<string>;
  stopped: Set<string>;
}

/** The app marks a dossier "seen" when it opens (CompanyPage `markSeen`): stopped and noted, not a failure. */
const SEEN_MARKER = /^\/api\/research\/[A-Za-z0-9._-]+\/seen$/;
const sig = (method: string, url: string) => `${method} ${new URL(url, "http://x").pathname}`;
const mutates = (method: string, url: string) => !["GET", "HEAD", "OPTIONS"].includes(method) && !(method === "POST" && UNLOCK_PATHS.has(new URL(url).pathname));

const test = base.extend<{ guard: Guard }>({
  guard: [
    async ({ context }, use) => {
      const guard: Guard = { observed: new Set(), stopped: new Set() };
      context.on("request", (req) => {
        if (mutates(req.method(), req.url())) guard.observed.add(sig(req.method(), req.url()));
      });
      await context.route("**/api/**", async (route) => {
        const req = route.request();
        if (mutates(req.method(), req.url())) {
          guard.stopped.add(sig(req.method(), req.url()));
          await route.abort();
          return;
        }
        await route.continue();
      });
      // Page-side layer: the write never leaves the page, whoever would have carried it (a service worker included).
      await context.exposeFunction("__roStopped", (what: string) => {
        guard.stopped.add(what);
      });
      await context.addInitScript((allowed: string[]) => {
        const w = window as unknown as { __roStopped: (what: string) => void };
        const refuse = (method: string, url: string) => {
          const path = new URL(url, location.href).pathname;
          if (["GET", "HEAD", "OPTIONS"].includes(method) || (method === "POST" && allowed.includes(path))) return false;
          void w.__roStopped(`${method} ${path}`);
          return true;
        };
        const realFetch = window.fetch.bind(window);
        window.fetch = (input: RequestInfo | URL, init?: RequestInit) => {
          const req = input instanceof Request ? input : null;
          const method = (init?.method ?? req?.method ?? "GET").toUpperCase();
          const url = req ? req.url : String(input);
          if (refuse(method, url)) return Promise.reject(new TypeError("blocked by the read-only live spec"));
          return realFetch(input, init);
        };
        const realOpen = XMLHttpRequest.prototype.open;
        XMLHttpRequest.prototype.open = function (this: XMLHttpRequest, method: string, url: string | URL, ...rest: unknown[]) {
          if (refuse(String(method).toUpperCase(), String(url))) throw new TypeError("blocked by the read-only live spec");
          return (realOpen as (...a: unknown[]) => void).call(this, method, url, ...rest);
        };
        if (navigator.sendBeacon) navigator.sendBeacon = (url: string | URL) => (refuse("POST", String(url)) ? false : true);
      }, [...UNLOCK_PATHS]);
      await use(guard);
    },
    { auto: true },
  ],
});

test.afterEach(async ({ guard }, info) => {
  const id = info.annotations.find((a) => a.type === "row")?.description ?? info.title;
  const notes = info.annotations.filter((a) => a.type === "note").map((a) => a.description ?? "");
  const violations: string[] = [];
  const marks: string[] = [];
  for (const s of new Set([...guard.observed, ...guard.stopped])) {
    const stopped = guard.stopped.has(s);
    if (SEEN_MARKER.test(s.split(" ")[1]) && stopped) marks.push(s);
    else violations.push(`${s}${stopped ? " (stopped)" : " (reached the network)"}`);
  }
  if (marks.length) notes.push(`app tried ${marks.join(", ")} (opening a company page marks it seen): stopped, live data untouched`);
  const skipped = info.status === "skipped";
  const failed = !skipped && (info.errors.length > 0 || (info.status !== undefined && info.status !== "passed") || violations.length > 0);
  const verdict = failed ? "FAIL" : skipped ? "SKIP" : "PASS";
  let reason: string;
  if (verdict === "SKIP") reason = info.annotations.find((a) => a.type === "skip")?.description ?? "skipped";
  else if (verdict === "FAIL") {
    const errors = info.errors.map((e) => (e.message ?? "").split("\n").find((l) => l.trim()) ?? "").map((l) => l.replace(/^Error:\s*/, ""));
    reason = [...(violations.length ? [`read-only guard: ${violations.join(", ")}`] : []), ...errors].join(" | ") || `status ${info.status}`;
  } else reason = notes.join("; ") || "ok";
  const oneLine = scrub(reason).replace(/\s+/g, " ").trim().slice(0, 600);
  appendFileSync(join(SHOTS, "results.tsv"), `${id}\t${verdict}\t${oneLine}\n`);
  // A write that got through is a failure even when the body passed.
  if (violations.length && info.errors.length === 0) throw new Error(`read-only guard: ${violations.join(", ")}`);
});

interface Ctx {
  page: Page;
  context: BrowserContext;
  info: TestInfo;
  slug: string;
  note: (text: string) => void;
  shoot: (step: string) => Promise<void>;
}

/**
 * Viewport shot (tab bar where it sits on the phone) and a full-page shot. A full-page capture freezes the fixed tab bar
 * on top of whatever sits at the bottom of the first viewport, so it is hidden for that capture only (as e2e/shots.ts).
 */
async function shoot(page: Page, slug: string, step: string) {
  await page.screenshot({ path: join(SHOTS, `${slug}-${step}.png`), scale: "css" });
  const hide = await page.addStyleTag({ content: `${NAV}{display:none !important}` });
  try {
    await page.screenshot({ path: join(SHOTS, `${slug}-${step}-full.png`), fullPage: true, scale: "css" });
  } finally {
    await hide.evaluate((el) => (el as Element).remove());
  }
}

/** One checklist row = one test. `id` is the row in results.tsv; `title` starts with it. */
function row(id: string, slug: string, title: string, body: (c: Ctx) => Promise<void>) {
  test(title, async ({ page, context }, info) => {
    info.annotations.push({ type: "row", description: id });
    await body({ page, context, info, slug, note: (text) => info.annotations.push({ type: "note", description: text }), shoot: (step) => shoot(page, slug, step) });
  });
}

/* ------------------------------------------------------------------------------------------ page helpers */

const frames = (page: Page, n = 2) =>
  page.evaluate(async (k) => {
    for (let i = 0; i < k; i++) await new Promise((resolve) => requestAnimationFrame(() => resolve(null)));
  }, n);

async function settle(page: Page) {
  await page.waitForLoadState("networkidle", { timeout: 15_000 }).catch(() => undefined);
  await page.evaluate(() => document.fonts.ready.then(() => true));
  await frames(page);
}

/**
 * Unlock screen → Beranda. The token goes in through `evaluate` (not `fill`, whose call log prints the value) and the
 * field is cleared by the app as soon as it submits. Nothing is shot before this returns.
 */
async function unlock(page: Page) {
  if (!TOKEN) throw new Error("LIVE_TOKEN is not set (see docs/qa/l4/README-live.md)");
  await page.goto("/#/beranda");
  const lock = page.getByRole("heading", { name: /Buka kunci/ });
  const home = page.getByRole("heading", { name: "Beranda", exact: true });
  await expect(lock.or(home).first()).toBeVisible({ timeout: 30_000 });
  if (await lock.isVisible()) {
    await page.locator('input[type="password"]').first().evaluate((el, value) => {
      const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
      set.call(el, value);
      el.dispatchEvent(new Event("input", { bubbles: true }));
    }, TOKEN);
    await page.getByRole("button", { name: "Buka kunci" }).click();
    try {
      await expect(home).toBeVisible({ timeout: 30_000 });
    } catch {
      const shown = await page.locator("form").first().innerText().catch(() => "");
      throw new Error(`unlock failed, screen says: ${scrub(shown).replace(/\s+/g, " ").slice(0, 120)}`);
    }
  }
  await expect(page.locator('section[data-block="perlu-perhatian"]')).toBeVisible({ timeout: 30_000 });
  await settle(page);
}

async function openCompany(page: Page, ticker: string, section?: string) {
  await page.goto(`/#/perusahaan/${ticker}${section ? `?section=${section}` : ""}`);
  await expect(page.getByRole("heading", { name: new RegExp(`^${ticker}\\b`) }).first()).toBeVisible({ timeout: 30_000 });
  await settle(page);
}

const block = (page: Page, id: string) => page.locator(`section[data-block="${id}"]`);
const tab = (page: Page, label: string) => page.locator(NAV).getByRole("link", { name: label });
const hashOf = (page: Page) => new URL(page.url()).hash;
const scrollY = (page: Page) => page.evaluate(() => Math.round(window.scrollY));
const maxScroll = (page: Page) => page.evaluate(() => Math.max(0, Math.round(document.documentElement.scrollHeight - window.innerHeight)));
const bodyOverflow = (page: Page) => page.evaluate(() => document.body.style.overflow);

/** A user's scroll (as in e2e/nav.e2e.ts): resolves once the `scroll` event is delivered; clamps to the page. */
async function scrollTo(page: Page, y: number): Promise<number> {
  const to = Math.min(y, await maxScroll(page));
  await page.evaluate(
    (target) =>
      new Promise<void>((resolve) => {
        if (Math.round(window.scrollY) === target) return resolve();
        window.addEventListener("scroll", () => resolve(), { once: true });
        window.scrollTo(0, target);
      }),
    to,
  );
  await expect.poll(() => scrollY(page)).toBe(to);
  return to;
}

/** Pull a collapsed block open (its title is a toggle with `aria-expanded`). */
async function openBlock(page: Page, id: string, title: RegExp) {
  const toggle = block(page, id).getByRole("button", { name: title }).first();
  await expect(toggle).toBeVisible({ timeout: 30_000 });
  if ((await toggle.getAttribute("aria-expanded")) !== "true") await toggle.click();
  await block(page, id).scrollIntoViewIfNeeded();
  await settle(page);
}

/* ------------------------------------------------------------------------------------------ audits */

/**
 * Text nodes with a digit that the screen cuts at the left or right edge: outside the viewport, outside a clipping
 * ancestor's box, clipped by their own box (ellipsis or `overflow` with wider content), or left of a horizontal scroller
 * that has not been scrolled. Inside a horizontal scroller the content may run past the right edge (it scrolls); the
 * scroller itself must be inside the viewport. Covers SVG `<text>` (chart axes). Skips visually hidden text and the
 * pannable structure-map canvas, whose nodes are cut at its edge by design.
 */
async function digitsCut(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const vw = document.documentElement.clientWidth;
    const out = new Set<string>();
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const text = (node.textContent ?? "").replace(/\s+/g, " ").trim();
      const el = node.parentElement;
      if (!el || !/\d/.test(text) || el.closest("[hidden]") || el.closest('[aria-label^="Kanvas peta"]')) continue;
      const own = getComputedStyle(el);
      if (own.display === "none" || own.visibility === "hidden" || own.opacity === "0") continue;
      const label = `${el.tagName.toLowerCase()} "${text.slice(0, 28)}"`;
      // Visually hidden text (`sr-only`: a 1 px clipped box) is not on screen, whatever its width.
      let hidden = false;
      for (let p: HTMLElement | null = el; p && p !== document.body; p = p.parentElement) {
        const b = p.getBoundingClientRect();
        if ((b.width <= 1.5 || b.height <= 1.5) && /(hidden|clip)/.test(getComputedStyle(p).overflowX + getComputedStyle(p).overflowY)) hidden = true;
      }
      if (hidden) continue;
      const range = document.createRange();
      range.selectNodeContents(node);
      for (const r of Array.from(range.getClientRects())) {
        if (r.width <= 1 || r.height <= 1) continue;
        let scrolled = false;
        for (let p: HTMLElement | null = el; p && p !== document.body; p = p.parentElement) {
          const ox = getComputedStyle(p).overflowX;
          if (ox === "visible") continue;
          const b = p.getBoundingClientRect();
          if ((ox === "auto" || ox === "scroll") && p.scrollWidth > p.clientWidth + 1) {
            if (b.left < -0.5 || b.right > vw + 0.5) out.add(`${label}: its scroller leaves the screen (${Math.round(b.left)}..${Math.round(b.right)})`);
            else if (p.scrollLeft === 0 && r.left < b.left - 0.5) out.add(`${label}: cut at the scroller's left edge (${Math.round(r.left)} < ${Math.round(b.left)})`);
            scrolled = true;
            break;
          }
          if (r.left < b.left - 0.5 || r.right > b.right + 0.5) out.add(`${label}: cut by a clipping box (${Math.round(r.left)}..${Math.round(r.right)} in ${Math.round(b.left)}..${Math.round(b.right)})`);
        }
        if (!scrolled && (r.left < -0.5 || r.right > vw + 0.5)) out.add(`${label}: off screen (${Math.round(r.left)}..${Math.round(r.right)} of ${vw})`);
      }
      const clips = /(hidden|clip)/.test(own.overflowX) || own.textOverflow === "ellipsis";
      if (clips && typeof el.scrollWidth === "number" && el.scrollWidth > el.clientWidth + 1) out.add(`${label}: truncated (${el.scrollWidth} > ${el.clientWidth})`);
    }
    return Array.from(out).slice(0, 8);
  });
}

/** Chart axis labels with digits that leave their own `<svg>` box. */
async function axisLabelsOutside(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    for (const svg of Array.from(document.querySelectorAll("svg"))) {
      const box = svg.getBoundingClientRect();
      if (box.width <= 1) continue;
      for (const t of Array.from(svg.querySelectorAll("text"))) {
        const text = (t.textContent ?? "").trim();
        if (!/\d/.test(text)) continue;
        const r = t.getBoundingClientRect();
        if (r.width <= 0) continue;
        if (r.left < box.left - 0.5 || r.right > box.right + 0.5) out.push(`"${text.slice(0, 20)}" ${Math.round(r.left)}..${Math.round(r.right)} outside chart ${Math.round(box.left)}..${Math.round(box.right)}`);
      }
    }
    return out.slice(0, 8);
  });
}

async function layoutChecks(page: Page, label: string, { axe = true, targets = true }: { axe?: boolean; targets?: boolean } = {}) {
  await settle(page);
  const o = await horizontalOverflow(page);
  expect.soft(o.overflow, `${label}: horizontal overflow (scroll ${o.scroll} > ${o.width}, main +${o.main})`).toBe(false);
  const collisions = await textCollisions(page);
  expect.soft(collisions, `${label}: text collisions: ${collisions.slice(0, 3).map((c) => `${c.a} x ${c.b}`).join("; ")}`).toEqual([]);
  if (targets) {
    const small = await smallTargets(page);
    expect.soft(small, `${label}: targets < 44 px: ${small.slice(0, 4).map((s) => `${s.el} ${s.w}x${s.h}`).join("; ")}`).toEqual([]);
  }
  if (axe) {
    const result = await axeSeriousCritical(page, { mobile: true });
    expect.soft(result.blocking, `${label}: axe serious/critical: ${result.blocking.map((b) => b.id).join(", ")}`).toEqual([]);
  }
  const cut = await digitsCut(page);
  expect.soft(cut, `${label}: digits cut at the edge: ${cut.slice(0, 3).join("; ")}`).toEqual([]);
}

async function numberChecks(page: Page, label: string) {
  await settle(page);
  const o = await horizontalOverflow(page);
  expect.soft(o.overflow, `${label}: horizontal overflow (scroll ${o.scroll} > ${o.width}, main +${o.main})`).toBe(false);
  const cut = await digitsCut(page);
  expect.soft(cut, `${label}: digits cut at the edge: ${cut.slice(0, 3).join("; ")}`).toEqual([]);
  const axis = await axisLabelsOutside(page);
  expect.soft(axis, `${label}: y-axis labels outside the chart: ${axis.slice(0, 3).join("; ")}`).toEqual([]);
}

/** Bottom bar at a large root font: four tabs on screen, each ≥ 44 px, none under another and none covered. */
async function navChecks(page: Page, label: string) {
  const tabs = await page.locator(`${NAV} a`).evaluateAll((links) => {
    const vw = document.documentElement.clientWidth;
    const vh = window.innerHeight;
    return links.map((a) => {
      const r = a.getBoundingClientRect();
      const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      return { name: (a.textContent ?? "").trim() || a.getAttribute("aria-label") || "?", l: r.left, t: r.top, r: r.right, b: r.bottom, w: r.width, h: r.height, onScreen: r.left >= -0.5 && r.right <= vw + 0.5 && r.top >= -0.5 && r.bottom <= vh + 0.5, covered: !(hit && a.contains(hit)) };
    });
  });
  expect.soft(tabs.map((t) => t.name), `${label}: bottom bar tabs`).toEqual([...TABS]);
  for (const t of tabs) {
    expect.soft(t.onScreen, `${label}: tab ${t.name} off screen (${Math.round(t.l)}..${Math.round(t.r)}, ${Math.round(t.t)}..${Math.round(t.b)})`).toBe(true);
    expect.soft(t.covered, `${label}: tab ${t.name} covered by another element`).toBe(false);
    expect.soft(t.w >= 43.5 && t.h >= 43.5, `${label}: tab ${t.name} smaller than 44 px (${Math.round(t.w)}x${Math.round(t.h)})`).toBe(true);
  }
  for (let i = 0; i < tabs.length; i++)
    for (let j = i + 1; j < tabs.length; j++) {
      const overlap = Math.min(tabs[i].r, tabs[j].r) - Math.max(tabs[i].l, tabs[j].l) > 1 && Math.min(tabs[i].b, tabs[j].b) - Math.max(tabs[i].t, tabs[j].t) > 1;
      expect.soft(overlap, `${label}: tabs ${tabs[i].name} and ${tabs[j].name} overlap`).toBe(false);
    }
}

/** Safe-area inset at the bottom as the page resolves it (0 in desktop-hosted emulation; the real value is a device check). */
const safeBottom = (page: Page) =>
  page.evaluate(() => {
    const probe = document.createElement("div");
    probe.style.cssText = "position:fixed;visibility:hidden;padding-bottom:env(safe-area-inset-bottom)";
    document.body.appendChild(probe);
    const px = parseFloat(getComputedStyle(probe).paddingBottom) || 0;
    probe.remove();
    return px;
  });

/* ------------------------------------------------------------------------------------------ A1, A2 */

for (const [id, ticker] of [["A1", T_A1], ["A2", T_A2]] as const) {
  row(`${id} ${ticker}`, `${id}-${ticker}`, `${id} ${ticker}: company page, structure map, Keuangan, Harga`, async (c) => {
    const { page } = c;
    await unlock(page);
    await openCompany(page, ticker);
    await layoutChecks(page, "top");
    await c.shoot("top");

    const struktur = block(page, "struktur");
    await struktur.scrollIntoViewIfNeeded();
    await expect(struktur).toBeVisible();
    await layoutChecks(page, "structure map");
    await c.shoot("struktur");
    const asList = struktur.getByRole("button", { name: /Lihat sebagai daftar/ });
    if (await asList.count()) {
      await asList.click();
      await settle(page);
      await layoutChecks(page, "structure list", { axe: false });
      await c.shoot("struktur-daftar");
      await struktur.getByRole("button", { name: /Lihat sebagai peta/ }).click();
      await settle(page);
    } else c.note("structure: no list toggle (empty map)");

    await openBlock(page, "keuangan", /^Keuangan/);
    await layoutChecks(page, "Keuangan");
    await c.shoot("keuangan");

    await openBlock(page, "harga", /^Harga/);
    await layoutChecks(page, "Harga");
    await c.shoot("harga");
    c.note(`${ticker}: top, structure map, Keuangan, Harga audited`);
  });
}

/* ------------------------------------------------------------------------------------------ M6 */

row("M6", "M6", "M6 Beranda: sheets lock the page behind them, close restores it, last button clear of the bottom", async (c) => {
  const { page } = c;
  await unlock(page);
  const max = await maxScroll(page);
  const start = await scrollTo(page, 200);
  if (max < 8) c.note("page is not scrollable: lock check is trivial");
  const bodyBefore = await bodyOverflow(page);
  const inset = await safeBottom(page);

  const sheets: [string, () => Promise<void>, "x" | "backdrop"][] = [];
  const item = page.locator('section[data-block="perlu-perhatian"] [data-event-id] button').first();
  if (await item.count()) sheets.push(["inbox", () => item.click(), "x"]);
  const macro = page.locator('section[data-block="makro"] ul li button').first();
  if (await macro.count()) sheets.push(["makro", () => macro.click(), "backdrop"]);
  expect(sheets.length, "no inbox item or Makro card on the live Beranda to open").toBeGreaterThan(0);
  c.note("Beranda has no filter sheet (filters are the chip row, see M8): inbox item and Makro sheets stand in");

  for (const [name, open, closeBy] of sheets) {
    await scrollTo(page, start);
    await open();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await settle(page);
    // Measured once the sheet is open: Playwright scrolls a target into view before it taps it.
    const before = await page.evaluate(() => document.scrollingElement!.scrollTop);
    expect.soft(await bodyOverflow(page), `${name}: body scroll lock while the sheet is open`).toBe("hidden");

    const box = (await dialog.boundingBox())!;
    // Playwright's mobile WebKit has neither wheel nor touch drag: PageDown, a trusted scroll key, stands in for a finger.
    for (let i = 0; i < 3; i++) await page.keyboard.press("PageDown");
    await frames(page, 4);
    const after = await page.evaluate(() => document.scrollingElement!.scrollTop);
    expect.soft(Math.abs(after - before), `${name}: page scrolled behind the sheet (scrollTop ${before} -> ${after})`).toBeLessThanOrEqual(1);

    const lowest = await dialog.evaluate((d) => {
      const vis = Array.from(d.querySelectorAll<HTMLElement>("button, a[href]")).filter((e) => e.getBoundingClientRect().height > 1 && getComputedStyle(e).visibility !== "hidden");
      return { bottom: Math.max(...vis.map((e) => e.getBoundingClientRect().bottom)), vh: window.innerHeight, n: vis.length };
    });
    expect.soft(lowest.bottom, `${name}: last button bottom ${Math.round(lowest.bottom)} is under the bottom edge (viewport ${lowest.vh}, safe inset ${inset})`).toBeLessThanOrEqual(lowest.vh - inset + 0.5);
    await c.shoot(`${name}-sheet`);

    if (closeBy === "x") await dialog.getByRole("button", { name: "Tutup", exact: true }).first().click();
    else await page.mouse.click(196, Math.max(4, box.y / 2));
    await expect(dialog).toBeHidden();
    await settle(page);
    expect.soft(await bodyOverflow(page), `${name}: body scroll not restored after closing by ${closeBy === "x" ? "×" : "the backdrop"}`).toBe(bodyBefore);
    expect.soft(Math.abs((await page.evaluate(() => document.scrollingElement!.scrollTop)) - before), `${name}: page moved when the sheet closed`).toBeLessThanOrEqual(1);
  }

  // Control: the same key does scroll the page once no sheet is open (else the checks above prove nothing).
  await scrollTo(page, 0);
  const resting = await page.evaluate(() => document.scrollingElement!.scrollTop);
  for (let i = 0; i < 2; i++) await page.keyboard.press("PageDown");
  await frames(page, 4);
  const moved = (await page.evaluate(() => document.scrollingElement!.scrollTop)) !== resting;
  if (max >= 8) expect.soft(moved, "control: PageDown does not scroll the page even with no sheet open, so the lock checks are inconclusive").toBe(true);
  c.note(`sheets checked: ${sheets.map(([n]) => n).join(", ")}; PageDown stands in for a finger (device check M6 remains)`);
});

/* ------------------------------------------------------------------------------------------ M7 */

row("M7", "M7", `M7 ${T_BIG}: tall claim sheet keeps header and × on screen and scrolls inside`, async (c) => {
  const { page } = c;
  await unlock(page);
  await openCompany(page, T_BIG, "klaim");
  const claims = page.locator('section[data-block="klaim"] ul[aria-label="Daftar klaim"] li button');
  const count = await claims.count();
  expect(count, `no claims on ${T_BIG} to open`).toBeGreaterThan(0);
  const lengths = await claims.evaluateAll((els) => els.map((e) => (e.textContent ?? "").length));
  const longest = lengths.indexOf(Math.max(...lengths));
  const full = page.viewportSize()!;

  const runAt = async (label: string, height: number): Promise<boolean> => {
    await page.setViewportSize({ width: full.width, height });
    await frames(page);
    await claims.nth(longest).scrollIntoViewIfNeeded();
    await claims.nth(longest).click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await settle(page);
    const pageTop = await page.evaluate(() => document.scrollingElement!.scrollTop);
    const geo = await dialog.evaluate((d) => {
      const rect = (e: Element | null) => (e ? e.getBoundingClientRect() : null);
      const h = rect(d.querySelector("h2"));
      const x = rect(d.querySelector('button[aria-label="Tutup"]'));
      const scroller = d.querySelector<HTMLElement>(".overflow-y-auto");
      const before = h?.top ?? 0;
      let scrolled = 0;
      if (scroller && scroller.scrollHeight > scroller.clientHeight + 1) {
        scroller.scrollTop = scroller.scrollHeight;
        scrolled = scroller.scrollTop;
      }
      return {
        vw: document.documentElement.clientWidth,
        vh: window.innerHeight,
        dialogTop: d.getBoundingClientRect().top,
        h,
        x,
        scrollable: scroller ? scroller.scrollHeight - scroller.clientHeight : 0,
        scrolled,
        headerMoved: Math.abs((rect(d.querySelector("h2"))?.top ?? 0) - before),
      };
    });
    const inside = (r: DOMRect | null) => r !== null && r.top >= -0.5 && r.bottom <= geo.vh + 0.5 && r.left >= -0.5 && r.right <= geo.vw + 0.5;
    expect.soft(geo.dialogTop >= -0.5, `${label} (${height} px): sheet top ${Math.round(geo.dialogTop)} is above the screen`).toBe(true);
    expect.soft(inside(geo.h as DOMRect | null), `${label} (${height} px): sheet title not inside the viewport`).toBe(true);
    expect.soft(inside(geo.x as DOMRect | null), `${label} (${height} px): × not inside the viewport`).toBe(true);
    if (geo.scrollable > 1) {
      expect.soft(geo.scrolled, `${label} (${height} px): content did not scroll inside the sheet`).toBeGreaterThan(0);
      expect.soft(geo.headerMoved, `${label} (${height} px): header moved while the content scrolled`).toBeLessThanOrEqual(1);
    } else c.note(`${label} (${height} px): content fits, nothing to scroll`);
    expect.soft(Math.abs((await page.evaluate(() => document.scrollingElement!.scrollTop)) - pageTop), `${label}: page scrolled behind the sheet`).toBeLessThanOrEqual(1);
    await c.shoot(`claim-${label}`);
    await dialog.getByRole("button", { name: "Tutup", exact: true }).first().click();
    await expect(dialog).toBeHidden();
    return geo.scrollable > 1;
  };
  let scrolled = false;
  for (const [label, height] of [["normal", full.height], ["toolbar", 560], ["short", 420]] as const) scrolled = (await runAt(label, height)) || scrolled;
  // Short claims fit even in a short window; a 300 px window makes any sheet tall, so the "scrolls inside" path is exercised.
  if (!scrolled) scrolled = await runAt("tiny", 300);
  await page.setViewportSize(full);
  c.note(`longest of ${count} claim(s), viewports ${full.height}, 560, 420${scrolled ? "" : ", 300"} px high; content ${scrolled ? "scrolled inside the sheet" : "never overflowed"}`);
});

/* ------------------------------------------------------------------------------------------ M8 */

row("M8", "M8", "M8 Beranda: the filter chips scroll sideways and the page does not", async (c) => {
  const { page } = c;
  await unlock(page);
  const chips = page.getByRole("group", { name: "Filter kotak masuk" });
  await expect(chips).toBeVisible();
  const hashBefore = hashOf(page);
  const dims = await chips.evaluate((el) => ({ sw: el.scrollWidth, cw: el.clientWidth, left: el.scrollLeft }));
  await c.shoot("before");
  if (dims.sw <= dims.cw + 1) c.note(`chips fit the row (${dims.sw} <= ${dims.cw}): nothing to scroll at this width`);
  else {
    // No wheel or touch drag in Playwright's mobile WebKit: focusing the last chip makes the browser scroll the row to it
    // (a real scroll of the row inside the page). The finger swipe and the iOS edge-swipe "back" stay device checks (M8).
    await chips.getByRole("button").last().focus();
    await frames(page, 4);
    const moved = await chips.evaluate((el) => el.scrollLeft);
    expect.soft(moved, "chip row did not scroll horizontally").toBeGreaterThan(0);
    c.note(`chip row scrolled ${Math.round(moved)} px of ${dims.sw - dims.cw}`);
  }
  const page_ = await page.evaluate(() => ({ x: window.scrollX, el: document.scrollingElement!.scrollLeft }));
  expect.soft(page_.x + page_.el, `page scrolled sideways (scrollX ${page_.x}, scrollingElement.scrollLeft ${page_.el})`).toBe(0);
  expect.soft(hashOf(page), "route changed during the swipe").toBe(hashBefore);
  expect.soft((await horizontalOverflow(page)).overflow, "page has horizontal overflow").toBe(false);
  await c.shoot("after");
});

/* ------------------------------------------------------------------------------------------ M10 (200 %) */

row("M10 (200 %)", "M10-200", "M10 (200 %): Beranda, a company page and every tab keep four reachable tabs", async (c) => {
  const { page } = c;
  await unlock(page);
  await page.addStyleTag({ content: FONT_200 });
  await frames(page, 4);

  const audit = async (label: string, step: string) => {
    await settle(page);
    await navChecks(page, label);
    await layoutChecks(page, label, { axe: false });
    await c.shoot(step);
  };
  await audit("Beranda", "beranda");
  await page.goto(`/#/perusahaan/${T_BIG}`);
  await expect(page.getByRole("heading", { name: new RegExp(`^${T_BIG}\\b`) }).first()).toBeVisible({ timeout: 30_000 });
  await audit(`company ${T_BIG}`, `company-${T_BIG}`);
  // Real taps: Playwright refuses the click when the tab is off screen or covered.
  for (const label of TABS) {
    await tab(page, label).click();
    await expect(page.locator(`${NAV} a[aria-current="page"]`)).toHaveText(label);
    await audit(`tab ${label}`, `tab-${label}`);
  }
  c.note("root font 200 %; Beranda, company page, four tabs; axe not run here (covered by A1/A2/M13 at 100 %)");
});

/* ------------------------------------------------------------------------------------------ M13 */

row("M13", "M13", `M13 ${T_BIG}: Ringkasan and Harga show no cut digit and y-axis labels stay inside`, async (c) => {
  const { page } = c;
  await unlock(page);
  await openCompany(page, T_BIG);
  await numberChecks(page, "Ringkasan");
  await c.shoot("ringkasan");
  await openBlock(page, "harga", /^Harga/);
  await numberChecks(page, "Harga");
  await c.shoot("harga");
  const svgs = await page.locator('section[data-block="harga"] svg').count();
  c.note(`${T_BIG}: ${svgs} chart(s) in Harga`);
});

/* ------------------------------------------------------------------------------------------ M15 */

row("M15", "M15", "M15 tab memory: Perusahaan, scroll, company, Portofolio, Perusahaan returns to the same page and scroll", async (c) => {
  const { page } = c;
  await unlock(page);
  await tab(page, "Perusahaan").click();
  await expect(page.getByRole("heading", { name: "Perusahaan", exact: true })).toBeVisible();
  await settle(page);
  const listY = await scrollTo(page, 300);
  const link = page.locator("li[data-company] a").first();
  await expect(link).toBeVisible({ timeout: 30_000 });
  await link.click();
  await expect.poll(() => hashOf(page)).toMatch(/^#\/perusahaan\/[A-Z0-9]+/);
  const companyHash = hashOf(page);
  const ticker = companyHash.replace(/^#\/perusahaan\//, "").replace(/[?].*$/, "");
  await expect(page.getByRole("heading", { name: new RegExp(`^${ticker}\\b`) }).first()).toBeVisible({ timeout: 30_000 });
  await settle(page);
  const wanted = await scrollTo(page, 600);
  await c.shoot("company-scrolled");

  await tab(page, "Portofolio").click();
  await expect.poll(() => hashOf(page)).toBe("#/portofolio");
  await settle(page);
  await tab(page, "Perusahaan").click();
  await expect.poll(() => hashOf(page), "Perusahaan tab opens another route than the company page").toBe(companyHash);
  await settle(page);
  await expect.poll(async () => Math.abs((await scrollY(page)) - wanted), { message: `scroll not restored (wanted ${wanted})`, timeout: 10_000 }).toBeLessThanOrEqual(8);
  await c.shoot("returned");
  c.note(`${ticker}: back at scroll ${await scrollY(page)} (left at ${wanted}${wanted < 40 ? ", page too short to scroll: restore not exercised" : ""}; list was scrolled to ${listY})`);
});

/* ------------------------------------------------------------------------------------------ M16 */

row("M16", "M16", "M16 Portofolio: position sheet, Buka halaman perusahaan, back to the list", async (c) => {
  const { page } = c;
  await unlock(page);
  await tab(page, "Portofolio").click();
  await expect(page.getByRole("heading", { level: 1, name: "Portofolio", exact: true })).toBeVisible();
  await settle(page);
  await c.shoot("list");
  const first = page.locator("button:not([aria-pressed])").filter({ hasText: /\blot\b/ }).first();
  await expect(first, "no position row on the live Portofolio").toBeVisible({ timeout: 30_000 });
  await first.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await settle(page);
  const title = ((await dialog.getByRole("heading").first().innerText()) || "").trim();
  const text = await dialog.innerText();
  const numbers = text.match(/\d[\d.,]*/g) ?? [];
  expect.soft(numbers.length, `position sheet shows ${numbers.length} number(s)`).toBeGreaterThanOrEqual(4);
  for (const label of ["Lot", "Nilai pasar", "P/L"]) expect.soft(text.includes(label), `position sheet lacks "${label}"`).toBe(true);
  await c.shoot("sheet");

  await dialog.getByRole("link", { name: "Buka halaman perusahaan" }).click();
  await expect.poll(() => hashOf(page)).toBe(`#/perusahaan/${title}?from=portofolio`);
  await expect(page.locator(`${NAV} a[aria-current="page"]`)).toHaveText("Portofolio");
  const back = page.getByRole("link", { name: "Kembali ke Portofolio" });
  await expect(back).toBeVisible({ timeout: 30_000 });
  await settle(page);
  await c.shoot("company");
  await back.click();
  await expect.poll(() => hashOf(page)).toBe("#/portofolio");
  await expect(page.getByRole("heading", { level: 1, name: "Portofolio", exact: true })).toBeVisible();
  c.note(`${title}: ${numbers.length} numbers in the sheet; opened ?from=portofolio with the Portofolio tab active; back returned to the list`);
});

/* ------------------------------------------------------------------------------------------ M2 (emulated) */

test.describe("M2 (emulated)", () => {
  // The only row that lets the service worker run (it is the thing under test); it reads and goes offline, nothing else.
  test.use({ serviceWorkers: "allow" });

  row("M2 (emulated)", "M2", "M2 (emulated): offline reload shows the cached shell or a clear error, never a white screen", async (c) => {
    const { page, context } = c;
    await unlock(page);
    for (const hash of ["/beranda", `/perusahaan/${T_BIG}`]) {
      await page.goto(`/#${hash}`);
      await settle(page);
    }
    const sw = await page.evaluate(async () => {
      if (!("serviceWorker" in navigator)) return "no serviceWorker API";
      const ready = await Promise.race([navigator.serviceWorker.ready.then(() => "ready"), new Promise<string>((resolve) => setTimeout(() => resolve("not ready after 10 s"), 10_000))]);
      return ready;
    });
    c.note(`service worker: ${sw}`);
    // One more online pass so the worker has the shell and the GETs of both screens.
    for (const hash of ["/beranda", `/perusahaan/${T_BIG}`]) {
      await page.goto(`/#${hash}`);
      await page.reload();
      await settle(page);
    }

    await context.setOffline(true);
    try {
      for (const [label, hash] of [["Beranda", "/beranda"], ["company", `/perusahaan/${T_BIG}`]] as const) {
        await page.goto(`/#${hash}`).catch(() => undefined);
        let navFailed = "";
        try {
          await page.reload({ waitUntil: "domcontentloaded", timeout: 20_000 });
        } catch (e) {
          navFailed = scrub(String((e as Error).message).split("\n")[0]).slice(0, 80);
        }
        if (navFailed) {
          expect.soft(false, `${label}: offline reload ended in the browser's own error page (${navFailed}); no cached app shell`).toBe(true);
          continue;
        }
        await page.waitForTimeout(3_000);
        const shown = await page.evaluate(() => (document.body?.innerText ?? "").replace(/\s+/g, " ").trim());
        expect.soft(shown.length, `${label}: offline reload left an empty body (white screen)`).toBeGreaterThan(20);
        const navVisible = await page.locator(NAV).isVisible();
        expect.soft(navVisible, `${label}: bottom bar gone offline`).toBe(true);
        const banner = /Offline/.test(shown);
        c.note(`${label} offline: ${shown.length} chars, bar ${navVisible ? "shown" : "gone"}, offline banner ${banner ? "shown" : "absent"}`);
        await page.screenshot({ path: join(SHOTS, `M2-${label}-offline.png`), scale: "css" }).catch(() => undefined);
      }
    } finally {
      await context.setOffline(false);
    }
  });
});

/* ------------------------------------------------------------------------------------------ M17 (cookies) */

row("M17 (cookies)", "M17", "M17 (cookies): unlocking finance-app and investing-app in one browser never locks the other", async (c) => {
  test.skip(!FA_URL || !FA_TOKEN, "LIVE_URL_FA and LIVE_TOKEN_FA are not both set");
  const { page, context } = c;
  const investing = new URL(process.env.LIVE_URL!).origin;
  const finance = new URL(FA_URL).origin;
  const cookieNames = async (origin: string) => (await context.cookies(origin)).map((k) => k.name).sort();
  const locked = async () => (await page.locator('input[type="password"]').filter({ visible: true }).count()) > 0;
  const session = (origin: string) => page.evaluate(async (o) => (await fetch(`${o}/api/session`, { credentials: "include" }).then((r) => r.status).catch(() => 0)), origin);

  await unlock(page);

  await page.goto(`${finance}/`);
  await page.waitForLoadState("domcontentloaded");
  await settle(page);
  if (await locked()) {
    const field = page.locator('input[type="password"]').first();
    await field.evaluate((el, value) => {
      const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
      set.call(el, value);
      el.dispatchEvent(new Event("input", { bubbles: true }));
    }, FA_TOKEN);
    await field.press("Enter");
    await expect(page.locator('input[type="password"]').filter({ visible: true }), "finance-app still shows its unlock field after the token").toHaveCount(0, { timeout: 30_000 });
  }
  await settle(page);

  // Reload each app in turn: the second unlock must not have locked the first.
  await page.goto(`${finance}/`);
  await page.reload();
  await settle(page);
  expect.soft(await locked(), "finance-app locked after reload").toBe(false);
  expect.soft(await session(finance), "finance-app GET /api/session").toBeLessThan(400);
  await c.shoot("finance");

  await page.goto(`${investing}/#/beranda`);
  await page.reload();
  await settle(page);
  expect.soft(await page.getByRole("heading", { name: /Buka kunci/ }).count(), "investing-app locked after reload").toBe(0);
  await expect(page.getByRole("heading", { name: "Beranda", exact: true })).toBeVisible({ timeout: 30_000 });
  expect.soft(await session(investing), "investing-app GET /api/session").toBe(200);
  await c.shoot("investing");

  await page.goto(`${finance}/`);
  await page.reload();
  await settle(page);
  expect.soft(await locked(), "finance-app locked after investing-app was used").toBe(false);

  const names = { investing: await cookieNames(investing), finance: await cookieNames(finance) };
  expect.soft(names.investing, "investing_session cookie name").toContain("investing_session");
  expect.soft(new Set(names.investing).size, "duplicate cookie names at the investing origin").toBe(names.investing.length);
  c.note(`cookie names at ${investing}: [${names.investing.join(", ")}]; at ${finance}: [${names.finance.join(", ")}] (names only)`);
});
