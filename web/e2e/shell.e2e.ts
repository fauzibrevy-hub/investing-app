import { expect, test, type Page } from "@playwright/test";
import { horizontalOverflow } from "./audits";
import { mockApi } from "./mockApi";

/**
 * Bottom bar (design.md §2.1, §5; plan §4 L2): four links, each at least 44 x 44 px, no FAB ("Catat transaksi"),
 * and every tab still on screen at 200 % root font or 200 % zoom. WebKit macOS with iPhone 15 emulation.
 */
const NAV = 'nav[aria-label="Navigasi utama"]';
const FAB = 'button[aria-label="Catat transaksi"]';
const LABELS = ["Beranda", "Portofolio", "Perusahaan", "Alat"];

async function ready(page: Page, hash: string) {
  await page.goto(`/#${hash}`);
  await page.getByRole("heading", { level: 1 }).first().waitFor();
  await page.waitForLoadState("networkidle");
  await page.evaluate(() => document.fonts.ready);
}

async function bar(page: Page) {
  return page.evaluate((nav) => {
    const vw = document.documentElement.clientWidth;
    const links = Array.from(document.querySelectorAll<HTMLElement>(`${nav} a`)).map((a) => {
      const r = a.getBoundingClientRect();
      return { name: a.getAttribute("aria-label") ?? a.textContent?.trim() ?? "", left: r.left, right: r.right, width: r.width, height: r.height };
    });
    return { vw, links, outside: links.filter((l) => l.left < -0.5 || l.right > vw + 0.5).map((l) => l.name) };
  }, NAV);
}

test.describe("bar bawah", () => {
  test("empat tautan, masing-masing >= 44 x 44 px, tanpa FAB", async ({ page }) => {
    await mockApi(page);
    await ready(page, "/beranda");
    const links = page.locator(`${NAV} a`);
    await expect(links).toHaveCount(4);
    const r = await bar(page);
    expect(r.links.map((l) => l.name)).toEqual(LABELS);
    for (const l of r.links) {
      expect(l.width, `lebar ${l.name}`).toBeGreaterThanOrEqual(44);
      expect(l.height, `tinggi ${l.name}`).toBeGreaterThanOrEqual(44);
    }
    await expect(page.locator(FAB)).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Catat transaksi" })).toHaveCount(0);
    expect(await horizontalOverflow(page)).toMatchObject({ overflow: false });
  });

  test("tab aktif ditandai aria-current di keempat tab akar", async ({ page }) => {
    await mockApi(page);
    for (const [hash, label] of [["/beranda", "Beranda"], ["/portofolio", "Portofolio"], ["/perusahaan", "Perusahaan"], ["/alat", "Alat"]]) {
      await ready(page, hash);
      await expect(page.locator(`${NAV} a[aria-current="page"]`)).toHaveText(label);
      await expect(page.locator(`${NAV} a[aria-current="page"]`)).toHaveCount(1);
    }
  });

  test("font akar 200% pada 393 px: tab tetap >= 44 x 44 px dan di dalam layar", async ({ page }) => {
    await page.setViewportSize({ width: 393, height: 659 });
    await mockApi(page);
    await page.addInitScript(() => document.addEventListener("DOMContentLoaded", () => { const s = document.createElement("style"); s.textContent = "html{font-size:200%}"; document.head.appendChild(s); }));
    await ready(page, "/beranda");
    const r = await bar(page);
    expect(r.links).toHaveLength(4);
    expect(r.outside, `tab di luar layar (${JSON.stringify(r.links)})`).toEqual([]);
    for (const l of r.links) {
      expect(l.width, `lebar ${l.name}`).toBeGreaterThanOrEqual(44);
      expect(l.height, `tinggi ${l.name}`).toBeGreaterThanOrEqual(44);
    }
    await expect(page.locator(FAB)).toHaveCount(0);
  });

  for (const [label, viewport, css] of [
    ["font akar 200% pada 320 px", { width: 320, height: 568 }, "html{font-size:200%}"],
    ["zoom 200% (viewport CSS 197 px)", { width: 197, height: 426 }, ""],
    ["zoom 200% dengan font akar 200% (viewport 197 px)", { width: 197, height: 426 }, "html{font-size:200%}"],
  ] as const) {
    test(`semua tab di layar: ${label}`, async ({ page }) => {
      await page.setViewportSize(viewport);
      await mockApi(page);
      if (css) await page.addInitScript((c) => document.addEventListener("DOMContentLoaded", () => { const s = document.createElement("style"); s.textContent = c; document.head.appendChild(s); }), css);
      await ready(page, "/beranda");
      const r = await bar(page);
      expect(r.links).toHaveLength(4);
      expect(r.outside, `tab di luar layar (${JSON.stringify(r.links)})`).toEqual([]);
      for (const l of r.links) expect(l.width, `lebar tab ${l.name}`).toBeGreaterThan(20);
      // Setiap tab tetap punya nama yang bisa dibaca pembaca layar, juga bila labelnya disembunyikan (< 300 px).
      for (const name of LABELS) await expect(page.locator(NAV).getByRole("link", { name })).toHaveCount(1);
      await expect(page.locator(FAB)).toHaveCount(0);
    });
  }
});
