import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { axeSeriousCritical, horizontalOverflow, smallTargets, textCollisions } from "./audits";
import { mockApi } from "./mockApi";
import { fullPageShot } from "./shots";

/**
 * The automated phone floor (D16, plan §3.2, §4 L2): every route of design.md §2.2 on the 393 px iPhone 15
 * emulation, with no page overflow, tap targets of at least 44 x 44 px and no serious or critical axe finding;
 * one screenshot per route under `docs/qa/<slice>/` (linked in the PR body, D17; full page, without the fixed tab bar). The routes with a real screen also
 * pass the text-collision audit and the whole floor again at a 200 % root font (design.md §6).
 * L3a owns this table (plan §3.3): each later lane edits only its own lines. `shot` is on for the routes whose screen
 * is real in this slice; the others are still placeholders (L3b, L3c) and take their screenshot in their own slice.
 */
const SLICE = "l3a";
const SHOTS = fileURLToPath(new URL(`../../docs/qa/${SLICE}/`, import.meta.url));

/** The largest root font size (design.md §6): rem doubles and the layout must reflow with no overflow or collision. */
const FONT_200 = "html{font-size:200% !important}";

/** One floor audit: page overflow, tap targets, axe and, on routes with a real screen, text collisions. */
async function floor(page: Page, { collisions }: { collisions: boolean }) {
  expect(await horizontalOverflow(page), "gulir horizontal halaman").toMatchObject({ overflow: false });
  if (collisions) expect(await textCollisions(page), "tabrakan teks").toEqual([]);
  expect(await smallTargets(page), "sasaran ketuk < 44 px").toEqual([]);
  const axe = await axeSeriousCritical(page, { mobile: true });
  expect(axe.blocking, `axe serious/critical (semua: ${JSON.stringify(axe.all)})`).toEqual([]);
}

/** `name` is the screenshot file; `heading` the screen title the route must show. */
const ROUTES: { route: string; name: string; heading: string; shot: boolean | string }[] = [
  { route: "/beranda", name: "beranda", heading: "Beranda", shot: true },
  { route: "/portofolio", name: "portofolio", heading: "Portofolio", shot: "l3c" },
  { route: "/perusahaan", name: "perusahaan", heading: "Perusahaan", shot: false },
  { route: "/perusahaan/tambah", name: "perusahaan-tambah", heading: "Tambah emiten", shot: false },
  { route: "/perusahaan/COAL", name: "perusahaan-COAL", heading: "COAL", shot: false },
  { route: "/alat", name: "alat", heading: "Alat", shot: true },
  { route: "/alat/bandingkan", name: "alat-bandingkan", heading: "Bandingkan", shot: true },
  { route: "/alat/kalibrasi", name: "alat-kalibrasi", heading: "Kalibrasi", shot: true },
  { route: "/alat/paket", name: "alat-paket", heading: "Paket & aturan", shot: true },
];

mkdirSync(SHOTS, { recursive: true });
for (const { route, name, heading, shot } of ROUTES) {
  test(`lantai otomatis ${route}`, async ({ page }) => {
    const unmocked = await mockApi(page);
    await page.goto(`/#${route}`);
    await expect(page.getByRole("heading", { name: heading, exact: true })).toBeVisible();
    await page.waitForLoadState("networkidle");
    await page.evaluate(() => document.fonts.ready);

    // `shot` = a real screen in this slice: text collisions are checked too, and the floor repeats at 200 % root font.
    await floor(page, { collisions: Boolean(shot) });
    expect(unmocked, "jalur API yang diminta tetapi tidak ditiru").toEqual([]);

    if (!shot) return;
    const dir = typeof shot === "string" ? fileURLToPath(new URL(`../../docs/qa/${shot}/`, import.meta.url)) : SHOTS;
    mkdirSync(dir, { recursive: true });
    await fullPageShot(page, `${dir}${name}.png`);
    await page.addStyleTag({ content: FONT_200 });
    await page.waitForTimeout(300);
    await page.waitForLoadState("networkidle");
    await floor(page, { collisions: true });
  });
}

test("rute tak dikenal dan `/` menuju #/beranda", async ({ page }) => {
  await mockApi(page);
  for (const path of ["/", "/riset", "/research/COAL", "/idx", "/portfolio/BUMI", "/macro"]) {
    await page.goto(`/#${path}`);
    await expect(page.getByRole("heading", { name: "Beranda", exact: true })).toBeVisible();
    expect(new URL(page.url()).hash).toBe("#/beranda");
  }
});
