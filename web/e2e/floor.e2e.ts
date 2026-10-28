import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";
import { axeSeriousCritical, horizontalOverflow, smallTargets } from "./audits";
import { mockApi } from "./mockApi";

/**
 * The automated phone floor (D16, plan §3.2, §4 L2): every route of design.md §2.2 on the 393 px iPhone 15
 * emulation, with no page overflow, tap targets of at least 44 x 44 px and no serious or critical axe finding;
 * one screenshot per route under `docs/qa/<slice>/` (linked in the PR body, D17).
 * L3a owns this table (plan §3.3): each later lane edits only its own lines.
 */
const SLICE = "l2";
const SHOTS = fileURLToPath(new URL(`../../docs/qa/${SLICE}/`, import.meta.url));

/** `name` is the screenshot file; `heading` the screen title the route must show. */
const ROUTES: { route: string; name: string; heading: string }[] = [
  { route: "/beranda", name: "beranda", heading: "Beranda" },
  { route: "/portofolio", name: "portofolio", heading: "Portofolio" },
  { route: "/perusahaan", name: "perusahaan", heading: "Perusahaan" },
  { route: "/perusahaan/tambah", name: "perusahaan-tambah", heading: "Tambah emiten" },
  { route: "/perusahaan/COAL", name: "perusahaan-COAL", heading: "COAL" },
  { route: "/alat", name: "alat", heading: "Alat" },
  { route: "/alat/bandingkan", name: "alat-bandingkan", heading: "Bandingkan" },
  { route: "/alat/kalibrasi", name: "alat-kalibrasi", heading: "Kalibrasi" },
  { route: "/alat/paket", name: "alat-paket", heading: "Paket & aturan" },
];

mkdirSync(SHOTS, { recursive: true });

for (const { route, name, heading } of ROUTES) {
  test(`lantai otomatis ${route}`, async ({ page }) => {
    const unmocked = await mockApi(page);
    await page.goto(`/#${route}`);
    await expect(page.getByRole("heading", { name: heading, exact: true })).toBeVisible();
    await page.waitForLoadState("networkidle");
    await page.evaluate(() => document.fonts.ready);

    expect(await horizontalOverflow(page), "gulir horizontal halaman").toMatchObject({ overflow: false });
    expect(await smallTargets(page), "sasaran ketuk < 44 px").toEqual([]);
    const axe = await axeSeriousCritical(page, { mobile: true });
    expect(axe.blocking, `axe serious/critical (semua: ${JSON.stringify(axe.all)})`).toEqual([]);
    expect(unmocked, "jalur API yang diminta tetapi tidak ditiru").toEqual([]);

    await page.screenshot({ path: `${SHOTS}${name}.png` });
  });
}

test("rute tak dikenal dan `/` menuju #/beranda", async ({ page }) => {
  await mockApi(page);
  for (const path of ["/", "/riset", "/research/COAL", "/idx", "/portfolio/BUMI"]) {
    await page.goto(`/#${path}`);
    await expect(page.getByRole("heading", { name: "Beranda", exact: true })).toBeVisible();
    expect(new URL(page.url()).hash).toBe("#/beranda");
  }
});
