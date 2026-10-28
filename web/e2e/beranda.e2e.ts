import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { axeSeriousCritical, horizontalOverflow, smallTargets, textCollisions } from "./audits";
import { mockApi, overrideApi } from "./mockApi";

/**
 * Beranda `#/beranda` (design.md §3.1; plan §3.3 e2e L3a): the Makro grid and the inbox sheets on the phone widths
 * 320 and 393 px, ported from finance-app's `macro.e2e.ts` and `overlays.e2e.ts`. Mocks only (mockApi.ts, D15); the
 * synthetic observations are built here from the shape of `e2e/fixtures/macro-observations.json`, not imported from `src/`.
 * Screenshots of the open inbox sheet and of the 200 % root font go to `docs/qa/l3a/`.
 */
const SHOTS = fileURLToPath(new URL("../../docs/qa/l3a/", import.meta.url));
mkdirSync(SHOTS, { recursive: true });

const WIDTHS = [320, 393] as const;
const MACRO = 'section[data-block="makro"]';
const INBOX = 'section[data-block="perlu-perhatian"]';

const obs = (i: number, over: Record<string, unknown> = {}) => ({
  pillar: "Energi",
  observed_at: "2026-10-20",
  fetched_at: "2026-10-20T09:20:51Z",
  freshness: "fresh",
  upstream_ok: true,
  metric_id: `m${i}`,
  name: `Indikator ${i + 1}`,
  unit: "USD/barel",
  value: i + 1,
  history: [],
  ...over,
});
const macroOf = (observations: unknown[]) => ({ as_of: "2026-10-20T09:21:52Z", observations });

async function openBeranda(page: Page) {
  const unmocked = await mockApi(page);
  await page.goto("/#/beranda");
  await page.locator(MACRO).getByRole("list", { name: /Indikator makro/ }).waitFor();
  await page.locator(`${INBOX} [data-event-id]`).first().waitFor();
  await page.waitForLoadState("networkidle");
  await page.evaluate(() => document.fonts.ready);
  return unmocked;
}

async function audit(page: Page) {
  await page.waitForLoadState("networkidle");
  expect(await horizontalOverflow(page), "gulir horizontal halaman").toMatchObject({ overflow: false });
  expect(await textCollisions(page), "tabrakan teks").toEqual([]);
  expect(await smallTargets(page), "sasaran ketuk < 44 px").toEqual([]);
  expect((await axeSeriousCritical(page, { mobile: true })).blocking, "axe serious/critical").toEqual([]);
}

for (const width of WIDTHS) {
  test.describe(`Beranda at ${width}px`, () => {
    test.use({ viewport: { width, height: 852 } });

    test("Makro cards: two columns, 44 px tall, names clamp at two lines, numbers stay one line, nothing overflows", async ({ page }) => {
      const long = "Indikator makro dengan nama yang sangat panjang sekali dan tidak muat satu baris di layar sempit";
      await mockApi(page);
      await overrideApi(page, "/api/macro/observations", {
        body: macroOf([obs(0, { name: long, value: 80.5 }), obs(1, { name: "Suku bunga acuan", unit: "percent", value: 4.75 }), obs(2, { name: "Kurs rupiah", unit: "IDR/USD", value: 15250 }), obs(3, { name: "Inflasi inti", unit: "percent_yoy", value: null })]),
      });
      await page.goto("/#/beranda");
      const list = page.locator(MACRO).getByRole("list", { name: /Indikator makro/ });
      await list.waitFor();
      await page.waitForLoadState("networkidle");

      const cards = await list.getByRole("listitem").evaluateAll((items) =>
        items.map((li) => {
          const b = li.querySelector("button")!.getBoundingClientRect();
          const [name, line2] = Array.from(li.querySelector("button")!.children) as HTMLElement[];
          const value = line2.firstElementChild as HTMLElement;
          const lh = (e: HTMLElement) => parseFloat(getComputedStyle(e).lineHeight);
          return { left: b.left, h: b.height, nameH: name.getBoundingClientRect().height, nameLine: lh(name), nameScrollH: name.scrollHeight, nameClientH: name.clientHeight, valueH: value.getBoundingClientRect().height, valueLine: lh(value), valueTabular: getComputedStyle(value).fontVariantNumeric };
        }),
      );
      expect(cards).toHaveLength(4);
      for (const c of cards) {
        expect(c.h, "tinggi kartu ≥ 44 px").toBeGreaterThanOrEqual(44);
        expect(c.nameH, "nama lebih dari dua baris").toBeLessThanOrEqual(2 * c.nameLine + 1);
        expect(c.valueH, "nilai membungkus").toBeLessThanOrEqual(c.valueLine + 1);
        expect(c.valueTabular).toContain("tabular-nums");
      }
      expect(new Set(cards.map((c) => Math.round(c.left))).size, "dua kolom").toBe(2);
      // Nama panjang dipotong pada baris kedua, bukan mendorong angka keluar.
      expect(cards[0].nameScrollH).toBeGreaterThan(cards[0].nameClientH);
      expect(await horizontalOverflow(page), "gulir horizontal halaman").toMatchObject({ overflow: false });
    });

    test("worst-case values (large, negative, with a unit) neither collide nor widen the page", async ({ page }) => {
      await mockApi(page);
      const worst = obs(0, { metric_id: "worst", name: "Inflasi tahunan dengan nama yang panjang", unit: "percent_yoy", value: -1234.56, history: [{ date: "2026-09-18", value: 1234.56 }] });
      await overrideApi(page, "/api/macro/observations", { body: macroOf([worst, obs(1), obs(2)]) });
      await page.goto("/#/beranda");
      const card = page.locator(MACRO).getByRole("button", { name: /Inflasi tahunan/ });
      await expect(card).toContainText("% yoy");
      await expect(card).toContainText("−1.234,56");
      await expect(card).toContainText("persen tahun ke tahun");
      await page.waitForLoadState("networkidle");
      const box = await card.boundingBox();
      const parts = await card.evaluate((b) => Array.from(b.querySelectorAll("[data-line] > *, [data-line]")).map((c) => { const r = c.getBoundingClientRect(); return { right: r.right, scrollW: (c as HTMLElement).scrollWidth, clientW: (c as HTMLElement).clientWidth }; }));
      for (const p of parts) {
        expect(p.right, "keluar dari kartu").toBeLessThanOrEqual(box!.x + box!.width + 0.5);
        expect(p.scrollW, "nilai terpotong").toBeLessThanOrEqual(p.clientW + 1);
      }
      expect(await textCollisions(page), "tabrakan teks").toEqual([]);
      expect(await horizontalOverflow(page)).toMatchObject({ overflow: false });
    });

    test("page audits: no overflow, collisions, small targets, axe", async ({ page }) => {
      const unmocked = await openBeranda(page);
      await audit(page);
      expect(unmocked, "jalur API yang diminta tetapi tidak ditiru").toEqual([]);
    });

    test("a Makro card opens the provenance sheet without widening the page", async ({ page }) => {
      await openBeranda(page);
      await page.locator(MACRO).getByRole("button", { name: /Minyak mentah/ }).click();
      const dialog = page.getByRole("dialog", { name: "Minyak mentah kontrak terdekat" });
      await dialog.waitFor();
      await page.waitForTimeout(300);
      expect(await horizontalOverflow(page)).toMatchObject({ overflow: false });
      const box = await dialog.evaluate((e) => {
        const r = e.getBoundingClientRect();
        return { left: r.left, right: r.right, scrollW: e.scrollWidth, clientW: e.clientWidth };
      });
      expect(box.left).toBeGreaterThanOrEqual(-0.5);
      expect(box.right).toBeLessThanOrEqual(width + 0.5);
      expect(box.scrollW).toBeLessThanOrEqual(box.clientW + 1);
      expect(await smallTargets(page), "sasaran ketuk < 44 px").toEqual([]);
      expect((await axeSeriousCritical(page, { mobile: true })).blocking, "axe serious/critical").toEqual([]);
      await page.keyboard.press("Escape");
      await expect(dialog).toBeHidden();
    });

    test("more than six indicators fold behind a 44 px toggle", async ({ page }) => {
      await mockApi(page);
      await overrideApi(page, "/api/macro/observations", { body: macroOf(Array.from({ length: 9 }, (_, i) => obs(i))) });
      await page.goto("/#/beranda");
      const list = page.locator(MACRO).getByRole("list", { name: /Indikator makro/ });
      await list.waitFor();
      await expect(list.getByRole("listitem")).toHaveCount(6);
      const toggle = page.locator(MACRO).getByRole("button", { name: "Tampilkan semua indikator (9)" });
      expect((await toggle.boundingBox())!.height).toBeGreaterThanOrEqual(44);
      await toggle.click();
      await expect(list.getByRole("listitem")).toHaveCount(9);
      await expect(toggle).toHaveAttribute("aria-expanded", "true");
      expect(await horizontalOverflow(page)).toMatchObject({ overflow: false });
    });

    test("a Makro error shows the error state with retry; the inbox and calendar stay", async ({ page }) => {
      await mockApi(page);
      let fail = true;
      await page.route("**/api/macro/observations", (route) =>
        fail ? route.fulfill({ status: 500, contentType: "application/json", body: JSON.stringify({ detail: "boom" }) }) : route.fallback(),
      );
      await page.goto("/#/beranda");
      await expect(page.locator(MACRO).getByText("Gagal memuat data makro")).toBeVisible();
      await expect(page.locator(INBOX)).toBeVisible();
      await expect(page.locator('section[data-block="kalender"]')).toBeVisible();
      expect(await horizontalOverflow(page)).toMatchObject({ overflow: false });
      fail = false;
      await page.locator(MACRO).getByRole("button", { name: "Coba lagi" }).click();
      await expect(page.locator(MACRO).getByRole("list", { name: /Indikator makro/ })).toBeVisible();
    });

    test("inbox: five rows, then Lihat semua; a filter chip narrows the list", async ({ page }) => {
      await openBeranda(page);
      const rows = page.locator(`${INBOX} [data-event-id]`);
      await expect(rows).toHaveCount(5);
      const more = page.locator(INBOX).getByRole("button", { name: /^Lihat semua \(/ });
      expect((await more.boundingBox())!.height).toBeGreaterThanOrEqual(44);
      await more.click();
      expect(await rows.count()).toBeGreaterThan(5);
      expect(await horizontalOverflow(page)).toMatchObject({ overflow: false });
      const chip = page.getByRole("group", { name: "Filter kotak masuk" }).getByRole("button", { name: /^Klaim/ });
      await chip.click();
      await expect(chip).toHaveAttribute("aria-pressed", "true");
      await expect(rows).toHaveCount(1);
    });

    test("inbox: an item opens its sheet; the company page opens from Beranda", async ({ page }) => {
      await openBeranda(page);
      await page.locator(`${INBOX} [data-event-id] button`).first().click();
      const dialog = page.getByRole("dialog");
      await expect(dialog).toBeVisible();
      await expect(dialog.getByRole("button", { name: "Buka halaman perusahaan" })).toBeVisible();
      await expect(dialog.getByRole("button", { name: "Abaikan" })).toBeVisible();
      await audit(page);
      await dialog.getByRole("button", { name: "Buka halaman perusahaan" }).click();
      await expect.poll(() => new URL(page.url()).hash).toMatch(/^#\/perusahaan\/COAL\?.*from=beranda/);
    });
  });
}

test.describe("screenshots", () => {
  test.use({ viewport: { width: 393, height: 852 } });

  test("Beranda with an inbox sheet open", async ({ page }) => {
    await openBeranda(page);
    await page.locator(`${INBOX} [data-event-id] button`).first().click();
    await expect(page.getByRole("dialog")).toBeVisible();
    await page.waitForTimeout(300);
    await page.screenshot({ path: `${SHOTS}beranda-inbox-sheet.png`, scale: "css" });
  });

  test("Beranda at 200 % root font: reflows with no overflow, collisions, small targets or axe finding", async ({ page }) => {
    await openBeranda(page);
    await page.addStyleTag({ content: "html{font-size:200% !important}" });
    await page.waitForTimeout(300);
    await audit(page);
    await page.screenshot({ path: `${SHOTS}beranda-font200.png`, fullPage: true, scale: "css" });
  });
});

test("#/macro lands on #/beranda and the tab bar has no Makro entry", async ({ page }) => {
  await mockApi(page);
  await page.goto("/#/macro");
  await expect(page.getByRole("heading", { name: "Beranda", exact: true })).toBeVisible();
  expect(new URL(page.url()).hash).toBe("#/beranda");
  await expect(page.locator('nav[aria-label="Navigasi utama"]').getByRole("link", { name: "Makro" })).toHaveCount(0);
});
