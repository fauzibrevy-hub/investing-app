import { mkdirSync, readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { axeSeriousCritical, horizontalOverflow, smallTargets, textCollisions } from "./audits";
import { mockApi, overrideApi } from "./mockApi";
import { fullPageShot, viewportShot } from "./shots";

/**
 * Portofolio `#/portofolio` (design.md §3.2; plan.md §3.3 e2e L3c, §4 row L3):
 * - Position Sheet numbers vs `e2e/fixtures/stock-portfolio.json`.
 * - Eye toggle hides every Rp value (`Rp•••` / `•••`) and persists across reload (`localStorage`).
 * - **Buka halaman perusahaan** → `#/perusahaan/:T?from=portofolio` with "‹ Portofolio" back label (`BackHeader`).
 * - Donut hues: distinct colours per slice (not one hue family).
 * - Screenshots via `e2e/shots.ts` to `docs/qa/l3c/` (Portofolio, eye on, position Sheet open, 200 %), < 1 MB each.
 */
const SHOTS = fileURLToPath(new URL("../../docs/qa/l3c/", import.meta.url));
const FIXTURE_PATH = fileURLToPath(new URL("./fixtures/stock-portfolio.json", import.meta.url));
mkdirSync(SHOTS, { recursive: true });

interface FixturePosition {
  no: number;
  category: string;
  ticker: string;
  company_name: string;
  shares: number;
  lots: number;
  avg_price: number;
  current_price: number;
  cost: number;
  valuation: number;
  pl: number;
  pl_pct: number;
  val_alloc_pct: number;
  target_alloc_pct: number;
  drift_pct: number;
}
interface FixturePortfolio {
  summary: {
    current_valuation: number;
    total_cost: number;
    total_pl: number;
    total_pl_pct: number;
    cash_balance: number;
    updated_at?: string;
  };
  positions: FixturePosition[];
  available: boolean;
}

const FIXTURE: FixturePortfolio = JSON.parse(readFileSync(FIXTURE_PATH, "utf8"));

const idInt = (n: number) => Math.round(Math.abs(n)).toLocaleString("id-ID");
const idRp = (n: number, signed = false) => {
  const s = n < 0 ? "−" : signed && n > 0 ? "+" : "";
  return `${s}Rp${idInt(n)}`;
};
const idPct = (n: number, signed = false) => {
  const s = n < 0 ? "−" : signed && n > 0 ? "+" : "";
  const body = Math.abs(n).toFixed(1).replace(/\.0$/, "").replace(".", ",");
  return `${s}${body}%`;
};
const idPp = (n: number) => `${n < 0 ? "−" : n > 0 ? "+" : ""}${Math.abs(n).toFixed(1).replace(/\.0$/, "").replace(".", ",")} pp`;

function rgbToHue(rgbStr: string): { r: number; g: number; b: number; h: number } {
  const m = rgbStr.match(/(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/);
  if (!m) throw new Error(`unexpected rgb string: ${rgbStr}`);
  const r = Number(m[1]) / 255;
  const g = Number(m[2]) / 255;
  const b = Number(m[3]) / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const d = max - min;
  let h = 0;
  if (d > 0) {
    if (max === r) h = ((g - b) / d + (g < b ? 6 : 0)) * 60;
    else if (max === g) h = ((b - r) / d + 2) * 60;
    else h = ((r - g) / d + 4) * 60;
  }
  return { r: Number(m[1]), g: Number(m[2]), b: Number(m[3]), h };
}

const hueDistance = (a: number, b: number) => 180 - Math.abs((Math.abs(a - b) % 360) - 180);

async function openPortofolio(page: Page) {
  const unmocked = await mockApi(page);
  await page.goto("/#/portofolio");
  await expect(page.getByRole("heading", { level: 1, name: "Portofolio", exact: true })).toBeVisible();
  await expect(page.getByRole("list", { name: "Legenda komposisi" })).toBeVisible();
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

/** Baris daftar Posisi (bukan baris legenda Komposisi, yang juga tombol): subjudulnya memuat "lot". */
const positionRow = (page: Page, ticker: string) => page.getByRole("button", { name: new RegExp(`^${ticker}\\s.*\\blot\\b`) });

/**
 * Elemen yang teks langsungnya memuat angka dan terpotong (elipsis atau `overflow` tersembunyi dengan isi lebih lebar
 * dari kotaknya). Nominal tidak pernah dipotong (finance-app design.md §7): label boleh, angka tidak.
 */
async function truncatedNumbers(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    for (const el of Array.from(document.body.querySelectorAll<HTMLElement>("*"))) {
      const own = Array.from(el.childNodes).filter((n) => n.nodeType === 3).map((n) => n.textContent ?? "").join("");
      if (!/\d/.test(own) || el.closest("[aria-hidden='true']")) continue;
      const cs = getComputedStyle(el);
      if (cs.display === "none" || cs.visibility === "hidden") continue;
      const clips = cs.textOverflow === "ellipsis" || ["hidden", "clip"].includes(cs.overflowX);
      if (clips && el.scrollWidth > el.clientWidth + 1) out.push(`${el.tagName.toLowerCase()} "${own.trim().slice(0, 40)}" (${el.scrollWidth} > ${el.clientWidth})`);
    }
    return out;
  });
}

/** Setiap kemunculan `Rp` yang diikuti digit di halaman (harus 0 saat nominal disembunyikan). */
async function visibleRpDigits(page: Page): Promise<string[]> {
  return page.evaluate(() => (document.body.innerText ?? "").match(/Rp\s*[\d.,]+/g) ?? []);
}

test.describe("Portofolio (#/portofolio)", () => {
  test("Sheet posisi menampilkan seluruh 11 angka sesuai e2e/fixtures/stock-portfolio.json", async ({ page }) => {
    const unmocked = await openPortofolio(page);
    expect(unmocked).toEqual([]);

    for (const pos of FIXTURE.positions) {
      await positionRow(page, pos.ticker).click();
      const dialog = page.getByRole("dialog", { name: pos.ticker });
      await expect(dialog).toBeVisible();
      await page.waitForTimeout(300);
      await expect(dialog.getByText(pos.company_name, { exact: true })).toBeVisible();
      await expect(dialog.getByText(pos.category, { exact: true })).toBeVisible();

      const rows = await dialog.evaluate((el) => {
        const out: Record<string, string> = {};
        for (const p of Array.from(el.querySelectorAll("div.flex.items-baseline.justify-between"))) {
          const spans = p.querySelectorAll("span");
          if (spans.length >= 2) {
            out[spans[0].textContent?.trim() ?? ""] = spans[spans.length - 1].textContent?.trim() ?? "";
          }
        }
        return out;
      });

      expect(rows).toEqual({
        Lot: idInt(pos.lots),
        Lembar: idInt(pos.shares),
        "Harga rata-rata": idRp(pos.avg_price),
        "Harga close": idRp(pos.current_price),
        Modal: idRp(pos.cost),
        "Nilai pasar": idRp(pos.valuation),
        "P/L": idRp(pos.pl, true),
        "P/L %": idPct(pos.pl_pct, true),
        Bobot: idPct(pos.val_alloc_pct),
        Target: idPct(pos.target_alloc_pct),
        Drift: idPp(pos.drift_pct),
      });

      await audit(page);
      await dialog.getByRole("button", { name: "Tutup" }).click();
      await expect(dialog).toBeHidden();
    }
  });

  test("tombol mata menyembunyikan setiap nilai Rp (termasuk di dalam Sheet) dan bertahan setelah muat ulang", async ({ page }) => {
    await openPortofolio(page);
    expect((await visibleRpDigits(page)).length).toBeGreaterThan(0);

    // Satuan ada di judul blok: baris Posisi menampilkan angka tanpa `Rp` (hidden: `•••`).
    const posisi = page.locator("section", { has: page.getByRole("heading", { name: "Posisi (Rp)" }) });
    expect(await posisi.innerText()).not.toMatch(/Rp\s*[\d•]/);

    const eye = page.getByRole("button", { name: "Sembunyikan nominal" });
    await expect(eye).toHaveAttribute("aria-pressed", "false");
    expect((await eye.boundingBox())!.height).toBeGreaterThanOrEqual(44);
    await eye.click();

    const eyeOn = page.getByRole("button", { name: "Tampilkan nominal" });
    await expect(eyeOn).toHaveAttribute("aria-pressed", "true");
    expect(await visibleRpDigits(page)).toEqual([]);
    await expect(page.getByRole("region", { name: "Nilai portofolio" })).toContainText("Rp••• · +1,5%");

    // Buka Sheet posisi saat mata aktif: semua nilai Rp tetap tersembunyi, persen tetap tampil.
    await positionRow(page, "COAL").click();
    const dialog = page.getByRole("dialog", { name: "COAL" });
    await expect(dialog).toBeVisible();
    expect(await visibleRpDigits(page)).toEqual([]);
    await expect(dialog).toContainText("+3,6%");
    await dialog.getByRole("button", { name: "Tutup" }).click();
    await expect(dialog).toBeHidden();

    // Muat ulang halaman: pilihan tetap tersimpan di localStorage dan semua Rp tetap tersembunyi.
    await page.reload();
    await expect(page.getByRole("heading", { level: 1, name: "Portofolio", exact: true })).toBeVisible();
    await expect(page.getByRole("list", { name: "Legenda komposisi" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Tampilkan nominal" })).toHaveAttribute("aria-pressed", "true");
    expect(await visibleRpDigits(page)).toEqual([]);
  });

  test("Buka halaman perusahaan membuka rute perusahaan dengan label kembali ‹ Portofolio dan kembali ke Portofolio", async ({ page }) => {
    await openPortofolio(page);
    await positionRow(page, "COAL").click();
    const dialog = page.getByRole("dialog", { name: "COAL" });
    await expect(dialog).toBeVisible();

    // Tautan lama (Dossier tesis, Riset, Matriks XBRL) tidak ada; hanya Buka halaman perusahaan.
    await expect(dialog.getByRole("link")).toHaveCount(1);
    const link = dialog.getByRole("link", { name: "Buka halaman perusahaan" });
    expect((await link.boundingBox())!.height).toBeGreaterThanOrEqual(44);
    await link.click();

    await expect.poll(() => new URL(page.url()).hash).toBe("#/perusahaan/COAL?from=portofolio");
    const back = page.getByRole("link", { name: "Kembali ke Portofolio" });
    await expect(back).toBeVisible();
    await expect(back).toHaveText("Portofolio");
    expect((await back.boundingBox())!.height).toBeGreaterThanOrEqual(44);
    await expect(page.locator('nav[aria-label="Navigasi utama"] a[aria-current="page"]')).toHaveText("Portofolio");

    await back.click();
    await expect.poll(() => new URL(page.url()).hash).toBe("#/portofolio");
    await expect(page.getByRole("heading", { level: 1, name: "Portofolio", exact: true })).toBeVisible();
  });

  test("warna donut: tiap irisan punya hue berbeda pada 2, 4, 5 dan 6 emiten (tidak pernah dua biru sebelum hue lain habis)", async ({ page }) => {
    await openPortofolio(page);

    // Palet 8 token (design.md §1.3) hanya punya 4 hue yang saling ≥ 30°; sesudahnya jarak terbaik yang mungkin 25° (5) dan
    // 17,9° (6), lihat `hues.test.ts`. Batas per jumlah emiten dipin di sini.
    const minGap: Record<number, number> = { 2: 30, 4: 30, 5: 25, 6: 17 };
    const checkDistinctHues = async (expectedCount: number) => {
      const slices = await page.evaluate(() => {
        const arcs = Array.from(document.querySelectorAll<SVGPathElement>("svg path[data-arc]"));
        const swatches = Array.from(document.querySelectorAll<HTMLElement>('[ aria-label="Legenda komposisi" ] [data-swatch]'));
        return {
          arcStrokes: arcs.map((a) => getComputedStyle(a).stroke),
          swatchBgs: swatches.map((s) => getComputedStyle(s).backgroundColor),
          swatchCats: swatches.map((s) => s.getAttribute("data-swatch")),
        };
      });
      expect(slices.arcStrokes).toHaveLength(expectedCount);
      expect(slices.swatchBgs).toHaveLength(expectedCount);
      expect(slices.arcStrokes).toEqual(slices.swatchBgs);
      expect(new Set(slices.arcStrokes).size, "setiap irisan warna unik").toBe(expectedCount);
      expect(new Set(slices.swatchCats).size, "setiap swatch token kategori unik").toBe(expectedCount);

      const hues = slices.arcStrokes.map((c) => rgbToHue(c).h);
      expect(hues.filter((h) => h > 190 && h < 250).length, "paling banyak satu biru").toBeLessThanOrEqual(1);
      for (let i = 0; i < hues.length; i++) {
        for (let j = i + 1; j < hues.length; j++) {
          expect(
            hueDistance(hues[i], hues[j]),
            `${expectedCount} emiten: jarak hue irisan ${i} (${slices.arcStrokes[i]}, ${hues[i].toFixed(1)}°) vs ${j} (${slices.arcStrokes[j]}, ${hues[j].toFixed(1)}°)`,
          ).toBeGreaterThanOrEqual(minGap[expectedCount]);
        }
      }
    };

    await checkDistinctHues(FIXTURE.positions.length);

    for (const count of [4, 5, 6]) {
      const positions = ["AAAA", "BBBB", "CCCC", "DDDD", "EEEE", "FFFF"].slice(0, count).map((ticker, i) => ({
        ...FIXTURE.positions[0],
        no: i + 1,
        ticker,
        full_code: `IDX:${ticker}`,
        company_name: `${ticker} Tbk`,
        valuation: (count - i) * 10_000_000,
        val_alloc_pct: (count - i) * 5,
      }));
      await overrideApi(page, "/api/finance/stock-portfolio", { body: { ...FIXTURE, positions } });
      await page.reload();
      await expect(page.getByRole("list", { name: "Legenda komposisi" }).getByRole("listitem")).toHaveCount(count);
      await checkDistinctHues(count);
    }
  });
});

test.describe("angka tidak terpotong (design.md §6)", () => {
  for (const { width, font } of [
    { width: 393, font: 200 },
    { width: 320, font: 100 },
    { width: 320, font: 200 },
  ]) {
    test(`lebar ${width} px, font akar ${font}%: subjudul Posisi utuh dan tidak ada angka yang terpotong`, async ({ page }) => {
      await page.setViewportSize({ width, height: 852 });
      await openPortofolio(page);
      if (font !== 100) {
        await page.addStyleTag({ content: `html{font-size:${font}% !important}` });
        await page.waitForTimeout(300);
      }
      for (const pos of FIXTURE.positions) {
        const subtitle = positionRow(page, pos.ticker).locator("span", { hasText: /lot · avg/ }).last();
        await expect(subtitle).toHaveText(`${idInt(pos.lots)} lot · avg ${idInt(pos.avg_price)}`);
        const fits = await subtitle.evaluate((el) => el.scrollWidth <= el.clientWidth + 1 && getComputedStyle(el).textOverflow !== "ellipsis");
        expect(fits, `subjudul ${pos.ticker} terpotong`).toBe(true);
      }
      expect(await truncatedNumbers(page), "angka terpotong").toEqual([]);
      expect(await horizontalOverflow(page), "gulir horizontal halaman").toMatchObject({ overflow: false });
    });
  }
});

test.describe("tangkapan layar docs/qa/l3c/", () => {
  test.use({ viewport: { width: 393, height: 852 } });

  const assertUnder1MB = (filePath: string) => {
    const size = statSync(filePath).size;
    expect(size, `${filePath} harus > 0 byte`).toBeGreaterThan(0);
    expect(size, `${filePath} harus < 1 MB`).toBeLessThan(1_000_000);
  };

  test("portofolio, mata aktif, Sheet posisi terbuka, dan font akar 200%", async ({ page }) => {
    await openPortofolio(page);

    // 1. Portofolio penuh (melalui shots.ts)
    const pPortofolio = `${SHOTS}portofolio.png`;
    await fullPageShot(page, pPortofolio);
    assertUnder1MB(pPortofolio);

    // 2. Mata aktif (nominal disembunyikan)
    await page.getByRole("button", { name: "Sembunyikan nominal" }).click();
    await expect(page.getByRole("button", { name: "Tampilkan nominal" })).toHaveAttribute("aria-pressed", "true");
    expect(await visibleRpDigits(page)).toEqual([]);
    await audit(page);
    const pEye = `${SHOTS}portofolio-eye-on.png`;
    await fullPageShot(page, pEye);
    assertUnder1MB(pEye);

    // Kembalikan mata ke normal lalu buka Sheet posisi
    await page.getByRole("button", { name: "Tampilkan nominal" }).click();
    await expect(page.getByRole("button", { name: "Sembunyikan nominal" })).toHaveAttribute("aria-pressed", "false");

    // 3. Sheet posisi terbuka
    await positionRow(page, "COAL").click();
    const dialog = page.getByRole("dialog", { name: "COAL" });
    await expect(dialog).toBeVisible();
    await page.waitForTimeout(300);
    await audit(page);
    const pSheet = `${SHOTS}portofolio-position-sheet.png`;
    await viewportShot(page, pSheet);
    assertUnder1MB(pSheet);

    await dialog.getByRole("button", { name: "Tutup" }).click();
    await expect(dialog).toBeHidden();

    // 4. Font akar 200 %
    await page.addStyleTag({ content: "html{font-size:200% !important}" });
    await page.waitForTimeout(300);
    await audit(page);
    const p200 = `${SHOTS}portofolio-font200.png`;
    await fullPageShot(page, p200);
    assertUnder1MB(p200);
  });
});
