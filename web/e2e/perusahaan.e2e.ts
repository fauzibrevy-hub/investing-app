import { mkdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { axeSeriousCritical, horizontalOverflow, smallTargets, textCollisions } from "./audits";
import { mockApi, overrideApi } from "./mockApi";
import { fullPageShot } from "./shots";

/**
 * Perusahaan `#/perusahaan`, the company page `#/perusahaan/:ticker` and Tambah (plan §3.3 e2e L3b), on the phone
 * widths 320 and 393 px: the sections on COAL (held, full data) and REST (manual), the claim sheet and `FactDrawer`
 * (finance-app `overlays.e2e.ts`; the PDF-badge case is covered by `FactDrawer.test.tsx`, a figure that fetches `/facts/` is not reachable from the phone page with the canon goldens), the long-name and 1e15 cases (finance-app `layout.e2e.ts`), the Tesis block (draft
 * detail, Hapus confirm, editor Sheet with the sticky Simpan), the Draf vault Sheet, the `?ticker=` prefill and the
 * thesis-only page (prd.md O1). Mocks only (mockApi.ts, D15); the 1e15 and long-name answers are built here from the
 * canon goldens, not from `src/`. Screenshots go to `docs/qa/l3b/` (the three route shots are the floor's).
 */
const SHOTS = fileURLToPath(new URL("../../docs/qa/l3b/", import.meta.url));
const CANON = fileURLToPath(new URL("../../tests/fixtures/research/canon/", import.meta.url));
mkdirSync(SHOTS, { recursive: true });
const golden = (name: string): any => JSON.parse(readFileSync(`${CANON}${name}.json`, "utf8"));

const WIDTHS = [320, 393] as const;
const FONT_200 = "html{font-size:200% !important}";

async function ready(page: Page, hash: string, waitFor: () => Promise<unknown>) {
  await page.goto(`/#${hash}`);
  await waitFor();
  await page.waitForLoadState("networkidle");
  await page.evaluate(() => document.fonts.ready);
}
const company = (page: Page, ticker: string, section?: string) =>
  ready(page, `/perusahaan/${ticker}${section ? `?section=${section}` : ""}`, () => page.getByRole("heading", { name: new RegExp(`^${ticker}\\b`) }).first().waitFor());

async function audit(page: Page) {
  await page.waitForLoadState("networkidle");
  expect(await horizontalOverflow(page), "gulir horizontal halaman").toMatchObject({ overflow: false });
  expect(await textCollisions(page), "tabrakan teks").toEqual([]);
  expect(await smallTargets(page), "sasaran ketuk < 44 px").toEqual([]);
  expect((await axeSeriousCritical(page, { mobile: true })).blocking, "axe serious/critical").toEqual([]);
}

/** Elemen `main` yang melewati tepi layar di luar kotak gulir sendiri. */
async function offscreen(page: Page) {
  return page.evaluate(() => {
    const vw = document.documentElement.clientWidth;
    const bad: string[] = [];
    for (const el of Array.from(document.querySelectorAll<HTMLElement>("main *"))) {
      const r = el.getBoundingClientRect();
      if (r.width <= 1 || r.height <= 1 || getComputedStyle(el).visibility === "hidden") continue;
      let p: HTMLElement | null = el.parentElement;
      let scroller = false;
      while (p && p !== document.body) {
        if (/(auto|scroll|hidden)/.test(getComputedStyle(p).overflowX) && p.getBoundingClientRect().right <= vw + 1) scroller = true;
        p = p.parentElement;
      }
      if (!scroller && (r.right > vw + 0.5 || r.left < -0.5)) bad.push(`${el.tagName.toLowerCase()} "${(el.textContent ?? "").trim().slice(0, 30)}" ${Math.round(r.left)}..${Math.round(r.right)}`);
    }
    return bad.slice(0, 8);
  });
}

/** Judul `h2` blok, berurutan sesuai halaman. */
const blockTitles = (page: Page) => page.locator("main section[data-block] > div > h2").allInnerTexts();

const SKIP = new Set(["fy", "q", "age_days", "limit", "offset", "total", "inbox_total", "count", "drivers_plotted", "populated", "expected", "entered", "unknown", "id"]);
/** Angka pecahan dikali 1e15, bilangan bulat dikali `intScale`: nilai rupiah mentah triliunan. */
function huge(o: any, intScale = 1, key = ""): any {
  if (typeof o === "number") return SKIP.has(key) ? o : Number.isInteger(o) ? o * intScale : o * 1e15;
  if (Array.isArray(o)) return o.map((v) => huge(v, intScale, key));
  if (o && typeof o === "object") return Object.fromEntries(Object.entries(o).map(([k, v]) => [k, huge(v, intScale, k)]));
  return o;
}

for (const width of WIDTHS) {
  test.describe(`Perusahaan at ${width}px`, () => {
    test.use({ viewport: { width, height: 852 } });

    test("Perusahaan: Dipegang, Manual, Pembanding, Tesis lain; a row opens the company page", async ({ page }) => {
      const unmocked = await mockApi(page);
      await ready(page, "/perusahaan", () => page.getByRole("heading", { name: "Perusahaan", exact: true }).waitFor());
      await expect(page.getByRole("link", { name: "Tambah", exact: true })).toHaveAttribute("href", /#\/perusahaan\/tambah$/);
      for (const group of [/^Dipegang/, /^Manual/, /^Pembanding/]) await expect(page.getByRole("heading", { name: group }).first()).toBeVisible();
      // Tesis lain: ticker dengan dossier tetapi di luar semesta riset (golden AAAA, BBBB, CCCC) dan draf vault.
      const block = page.locator('section[data-block="tesis-lain"]');
      await expect(block.getByRole("heading", { name: "Tesis lain (3)" })).toBeVisible();
      await block.getByRole("button", { name: /^Tesis lain/ }).click();
      for (const t of ["AAAA", "BBBB", "CCCC"]) await expect(block.locator(`[data-thesis="${t}"]`)).toBeVisible();
      await expect(block.getByRole("button", { name: "Draf vault (2)" })).toBeVisible();
      await audit(page);
      expect(unmocked, "jalur API yang diminta tetapi tidak ditiru").toEqual([]);
      await page.locator('[data-thesis="AAAA"] a').click();
      await expect(page.getByText("AAAA belum ada di daftar riset")).toBeVisible();
    });

    test("Draf vault Sheet: the list, a draft opens in the same Sheet, back returns to the list", async ({ page }) => {
      await mockApi(page);
      await ready(page, "/perusahaan", () => page.getByRole("heading", { name: "Perusahaan", exact: true }).waitFor());
      await page.getByRole("button", { name: /^Tesis lain/ }).click();
      await page.getByRole("button", { name: "Draf vault (2)" }).click();
      const dialog = page.getByRole("dialog");
      await expect(dialog.getByRole("list", { name: "Draf vault" }).getByRole("listitem")).toHaveCount(2);
      await expect(dialog.getByText("Minyak naik")).toBeVisible();
      await audit(page);
      if (width === 393) await page.screenshot({ path: `${SHOTS}perusahaan-draf-vault.png`, scale: "css" });

      await dialog.locator("[data-vault-draft]").first().click();
      await expect(dialog.getByText("Harga minyak naik karena supply.")).toBeVisible();
      await expect(dialog.getByText(/baca saja/)).toBeVisible();
      await expect(dialog.getByRole("button", { name: /Hapus/ }), "draf vault tidak bisa dihapus").toHaveCount(0);
      await audit(page);
      await dialog.getByRole("button", { name: "Kembali ke daftar draf vault" }).click();
      await expect(dialog.getByRole("list", { name: "Draf vault" })).toBeVisible();
    });

    test("Tambah ?ticker=: the code is prefilled and checked", async ({ page }) => {
      const unmocked = await mockApi(page);
      await ready(page, "/perusahaan/tambah?ticker=CTOH", () => page.getByRole("textbox").first().waitFor());
      await expect(page.getByRole("textbox").first()).toHaveValue("CTOH");
      await expect(page.getByText("Contoh Tambang Tbk").first()).toBeVisible();
      await expect(page.getByText("Langkah 1 dari 4: Emiten")).toBeVisible();
      await audit(page);
      expect(unmocked, "jalur API yang diminta tetapi tidak ditiru").toEqual([]);
      if (width === 393) await fullPageShot(page, `${SHOTS}perusahaan-tambah-prefill.png`);
    });

    test("thesis-only page (prd.md O1): header, Tesis open, Tambah ke riset prefills Tambah", async ({ page }) => {
      await mockApi(page);
      await overrideApi(page, "/api/research/AAAA", { status: 404, body: { detail: "AAAA tidak ada di daftar riset" } });
      await ready(page, "/perusahaan/AAAA", () => page.getByText("AAAA belum ada di daftar riset").waitFor());
      await expect(page.getByRole("heading", { name: "AAAA", exact: true })).toBeVisible();
      await expect(page.getByRole("button", { name: "Tesis", exact: true }), "blok Tesis selalu terbuka").toHaveCount(0);
      await expect(page.locator('section[data-block="tesis"]')).toContainText("Review log");
      expect(await blockTitles(page)).toEqual(["Tesis"]);
      await expect(page.getByRole("contentinfo"), "footer menutup halaman hanya-tesis").toHaveText("Bukan saran investasi");
      await audit(page);
      if (width === 393) await fullPageShot(page, `${SHOTS}perusahaan-hanya-tesis.png`);
      await page.getByRole("link", { name: "Tambah ke riset" }).click();
      await expect(page.getByText("Langkah 1 dari 4: Emiten")).toBeVisible();
      expect(new URL(page.url()).hash).toMatch(/^#\/perusahaan\/tambah\?ticker=AAAA/);
      await expect(page.getByRole("textbox").first()).toHaveValue("AAAA");
    });
  });
}

for (const ticker of ["COAL", "REST"] as const) {
  for (const width of WIDTHS) {
    test.describe(`${ticker} company page at ${width}px`, () => {
      test.use({ viewport: { width, height: 852 } });

      test("blocks in order, Tesis between Jurnal and Harga, Struktur / Harga / Keuangan carry data", async ({ page }) => {
        const unmocked = await mockApi(page);
        await company(page, ticker);
        const titles = await blockTitles(page);
        const order = ["Struktur bisnis", "Angka kunci", "Klaim", "Jurnal", "Tesis", "Harga", "Keuangan"].map((t) => titles.findIndex((x) => x.startsWith(t)));
        expect(order, `judul blok: ${JSON.stringify(titles)}`).not.toContain(-1);
        expect(order, "urutan blok").toEqual([...order].sort((a, b) => a - b));
        // Struktur: peta pikiran dengan daftar di baliknya; Harga terbuka dari awal; Keuangan dan Tesis terlipat.
        await expect(page.locator('section[data-block="struktur"] [data-mm-node]').first()).toBeVisible();
        await expect(page.getByRole("img", { name: /harga/i }).first()).toBeVisible();
        await expect(page.getByRole("button", { name: "Keuangan", exact: true })).toHaveAttribute("aria-expanded", "false");
        await expect(page.getByRole("button", { name: "Tesis", exact: true })).toHaveAttribute("aria-expanded", "false");
        await audit(page);
        expect(unmocked, "jalur API yang diminta tetapi tidak ditiru").toEqual([]);
        if (width === 393 && ticker === "REST") await fullPageShot(page, `${SHOTS}perusahaan-REST.png`);
      });

      test("Struktur: as a list and back to the map", async ({ page }) => {
        await mockApi(page);
        await company(page, ticker, "struktur");
        await page.getByRole("button", { name: "Lihat sebagai daftar" }).click();
        await expect(page.getByRole("button", { name: "Lihat sebagai peta" })).toBeVisible();
        await audit(page);
        await page.getByRole("button", { name: "Lihat sebagai peta" }).click();
        await expect(page.locator('section[data-block="struktur"] [data-mm-node]').first()).toBeVisible();
      });

      test("Keuangan (?section=keuangan): the table and its three views, nothing overflows", async ({ page }) => {
        await mockApi(page);
        await company(page, ticker, "keuangan");
        await page.getByRole("table").first().waitFor();
        for (const view of ["Kuartal", "TTM", "Tahunan"]) {
          await page.getByRole("radio", { name: view }).click();
          await page.waitForLoadState("networkidle");
          await expect(page.locator("[aria-busy='true']")).toHaveCount(0);
          expect(await horizontalOverflow(page), `${view}: gulir horizontal halaman`).toMatchObject({ overflow: false });
        }
        await audit(page);
      });

      test("Harga (?section=harga): the chart and statistics", async ({ page }) => {
        await mockApi(page);
        await company(page, ticker, "harga");
        await expect(page.getByRole("img", { name: /harga/i }).first()).toBeVisible();
        await expect(page.getByTestId("price-stats")).toBeVisible();
        await audit(page);
      });
    });
  }
}

/** D13 (design.md §1, §2.1): a Mac window keeps the phone column and the bottom bar, so nothing here may change at 1280 px. */
test.describe("COAL company page at 1280px (D13: the phone layout at every width)", () => {
  test.use({ viewport: { width: 1280, height: 852 } });

  test("Struktur: the mind map keeps its phone height and bar clearance", async ({ page }) => {
    await mockApi(page);
    const geometry = () =>
      page.evaluate(() => {
        const map = document.querySelector<HTMLElement>("[data-mindmap]")!;
        return { height: Math.round(map.getBoundingClientRect().height), clearance: getComputedStyle(map.parentElement!).paddingBottom };
      });
    await page.setViewportSize({ width: 393, height: 852 });
    await company(page, "COAL", "struktur");
    const phone = await geometry();
    await page.setViewportSize({ width: 1280, height: 852 });
    await page.waitForLoadState("networkidle");
    const mac = await geometry();
    expect(mac, "peta pikiran di 1280 px sama dengan di 393 px").toEqual(phone);
    expect(parseFloat(mac.clearance), "ruang bebas di atas bilah bawah").toBeGreaterThan(0);
    await expect(page.locator('nav[aria-label="Navigasi utama"]'), "bilah bawah tetap tampil").toBeVisible();
    await audit(page);
  });

  test("Keuangan: the two Segmented controls stack in the column, units sit under their row label", async ({ page }) => {
    await mockApi(page);
    await company(page, "COAL", "keuangan");
    await page.getByRole("table").first().waitFor();
    const views = (await page.getByRole("radiogroup", { name: "Tampilan laporan" }).boundingBox())!;
    const counts = (await page.getByRole("radiogroup", { name: "Jumlah periode" }).boundingBox())!;
    expect(counts.y, "Jumlah periode di bawah Tampilan laporan").toBeGreaterThanOrEqual(views.y + views.height - 0.5);
    expect(Math.abs(counts.x - views.x), "kedua kontrol rata kiri").toBeLessThanOrEqual(1);
    const stacked = await page.evaluate(() => {
      const unit = document.querySelector<HTMLElement>("table th[scope='row'] > span.block");
      return unit ? getComputedStyle(unit).display : null;
    });
    if (stacked !== null) expect(stacked, "satuan baris di bawah label").toBe("block");
    await audit(page);
  });
});

for (const width of WIDTHS) {
  test.describe(`COAL overlays at ${width}px`, () => {
    test.use({ viewport: { width, height: 852 } });

    test("Klaim: claim sheet", async ({ page }) => {
      await mockApi(page);
      await company(page, "COAL", "klaim");
      await page.locator("button[data-claim-status]").first().click();
      await expect(page.getByRole("dialog")).toBeVisible();
      await audit(page);
    });

    test("Keuangan: FactDrawer opens from a table figure", async ({ page }) => {
      await mockApi(page);
      await company(page, "COAL", "keuangan");
      await page.getByRole("table").first().getByRole("button", { name: /^Asal angka/ }).first().click();
      await expect(page.getByRole("dialog")).toBeVisible();
      await audit(page);
    });

    test("Angka kunci: a key figure opens its origin", async ({ page }) => {
      await mockApi(page);
      await company(page, "COAL", "angka");
      await page.locator('section[data-block="angka"] button[aria-expanded]').first().click();
      await expect(page.locator('section[data-block="angka"] button[aria-expanded="true"]').first()).toBeVisible();
      await audit(page);
    });
  });
}

for (const width of WIDTHS) {
  test.describe(`Tesis block at ${width}px`, () => {
    test.use({ viewport: { width, height: 852 } });

    test("?section=tesis opens the block: read-only dossier, the draft list, no thesis line in the header", async ({ page }) => {
      const unmocked = await mockApi(page);
      await company(page, "COAL", "tesis");
      const block = page.locator('section[data-block="tesis"]');
      await expect(block.getByRole("button", { name: "Tesis", exact: true })).toHaveAttribute("aria-expanded", "true");
      await expect(block).toContainText("Review log");
      await expect(block.getByRole("list", { name: "Draf tesis COAL" }).getByRole("listitem")).toHaveCount(1);
      await expect(block.getByRole("heading", { name: "Draf (1)" })).toBeVisible();
      await expect(page.locator('[data-block="header"]')).not.toContainText("Tesis");
      await audit(page);
      expect(unmocked, "jalur API yang diminta tetapi tidak ditiru").toEqual([]);
      if (width === 393) await page.screenshot({ path: `${SHOTS}perusahaan-COAL-tesis.png`, scale: "css" });
    });

    test("REST has no dossier: an empty state that offers to write a draft", async ({ page }) => {
      await mockApi(page);
      await overrideApi(page, "/api/thesis/drafts", { body: { drafts: [], vault_drafts: [], total: 0 } });
      await company(page, "REST", "tesis");
      await expect(page.locator('section[data-block="tesis"]')).toContainText("Belum ada tesis untuk REST");
      await expect(page.getByRole("button", { name: "Tulis draf tesis" })).toBeVisible();
      await audit(page);
    });

    test("draft detail Sheet and the Hapus confirmation delete the draft", async ({ page }) => {
      await mockApi(page);
      await company(page, "COAL", "tesis");
      await page.getByRole("button", { name: /Batubara tahan harga rendah/ }).click();
      const detail = page.getByRole("dialog", { name: "Draf tesis" });
      await expect(detail.getByText("Biaya tunai per ton naik di atas 45 USD")).toBeVisible();
      await audit(page);
      await detail.getByRole("button", { name: "Hapus draf" }).click();
      const confirm = page.getByRole("dialog", { name: "Hapus draf?" });
      await expect(confirm).toBeVisible();
      await audit(page);
      // Batal tidak menghapus; Hapus mengirim DELETE.
      await confirm.getByRole("button", { name: "Batal" }).click();
      await expect(confirm).toBeHidden();
      await detail.getByRole("button", { name: "Hapus draf" }).click();
      const deleted = page.waitForRequest((r) => r.method() === "DELETE" && new URL(r.url()).pathname === "/api/thesis/drafts/d-coal-1");
      await confirm.getByRole("button", { name: "Hapus", exact: true }).click();
      await deleted;
    });

    test("editor Sheet: full height, Simpan stays at the bottom, empty save names the fields, a filled save POSTs", async ({ page }) => {
      await mockApi(page);
      await company(page, "COAL", "tesis");
      await page.getByRole("button", { name: "Tulis draf tesis" }).click();
      const sheet = page.getByRole("dialog", { name: "Tulis draf tesis" });
      await expect(sheet).toBeVisible();
      const box = (await sheet.boundingBox())!;
      expect(box.height, "Sheet setinggi layar (90%)").toBeGreaterThan(852 * 0.85);
      const save = sheet.getByRole("button", { name: "Simpan draf" });
      const sticky = async (label: string) => {
        const b = (await save.boundingBox())!;
        expect(b.height, `${label}: tinggi Simpan`).toBeGreaterThanOrEqual(44);
        expect(b.y + b.height, `${label}: Simpan di dalam layar`).toBeLessThanOrEqual(852 + 0.5);
        const hit = await page.evaluate(([x, y]) => document.elementFromPoint(x, y)?.closest("button")?.textContent?.trim(), [b.x + b.width / 2, b.y + b.height / 2] as const);
        expect(hit, `${label}: Simpan tertutup`).toBe("Simpan draf");
      };
      await sticky("awal");
      await sheet.getByLabel("Catatan").scrollIntoViewIfNeeded();
      await sticky("setelah gulir");
      await audit(page);
      if (width === 393) await page.screenshot({ path: `${SHOTS}perusahaan-COAL-editor.png`, scale: "css" });

      await save.click();
      await expect(sheet.getByText("Judul tesis wajib diisi")).toBeVisible();
      await expect(sheet.getByText("Thesis statement wajib diisi")).toBeVisible();
      await sheet.getByLabel("Judul", { exact: true }).fill("Tesis contoh");
      await sheet.getByLabel("Thesis statement").fill("Pernyataan contoh sintetis.");
      const posted = page.waitForRequest((r) => r.method() === "POST" && new URL(r.url()).pathname === "/api/thesis/drafts");
      await save.click();
      const body = (await posted).postDataJSON();
      expect(body).toMatchObject({ ticker: "COAL", title: "Tesis contoh", thesis_statement: "Pernyataan contoh sintetis." });
      await expect(sheet).toBeHidden();
    });
  });
}

test.describe("200 % root font", () => {
  test.use({ viewport: { width: 393, height: 852 } });

  test("company page COAL (top and Tesis open) reflows with no overflow or collision", async ({ page }) => {
    await mockApi(page);
    await company(page, "COAL", "tesis");
    await page.addStyleTag({ content: FONT_200 });
    await page.waitForTimeout(300);
    await page.waitForLoadState("networkidle");
    await audit(page);
    await page.evaluate(() => window.scrollTo(0, 0));
    await fullPageShot(page, `${SHOTS}perusahaan-COAL-200.png`);
  });

  for (const [label, hash, ready_] of [
    ["Perusahaan", "/perusahaan", (p: Page) => p.getByRole("heading", { name: "Perusahaan", exact: true }).waitFor()],
    ["thesis-only AAAA", "/perusahaan/AAAA", (p: Page) => p.getByText("AAAA belum ada di daftar riset").waitFor()],
    ["Tambah ?ticker=", "/perusahaan/tambah?ticker=CTOH", (p: Page) => p.getByText("Contoh Tambang Tbk").first().waitFor()],
  ] as const) {
    test(`${label} at 200 %`, async ({ page }) => {
      await mockApi(page);
      await overrideApi(page, "/api/research/AAAA", { status: 404, body: { detail: "AAAA tidak ada di daftar riset" } });
      await ready(page, hash, () => ready_(page));
      await page.addStyleTag({ content: FONT_200 });
      await page.waitForTimeout(300);
      await page.waitForLoadState("networkidle");
      await audit(page);
    });
  }
});

for (const width of WIDTHS) {
  test.describe(`1e15 numbers at ${width}px`, () => {
    test.use({ viewport: { width, height: 852 } });

    async function hugeMocks(page: Page) {
      await mockApi(page);
      await overrideApi(page, "/api/research/COAL", { body: huge(golden("ticker-COAL")) });
      await overrideApi(page, "/api/research/COAL/pack", { body: huge(golden("pack-COAL"), 1e12) });
      await overrideApi(page, "/api/research/COAL/prices", { body: huge(golden("prices-COAL-1y"), 1e12) });
      await overrideApi(page, "/api/research/COAL/financials", { body: huge(golden("financials-COAL-standalone"), 1e6) });
    }

    test("page top: no chip or value runs off the screen", async ({ page }) => {
      await hugeMocks(page);
      await company(page, "COAL");
      expect(await horizontalOverflow(page), "gulir horizontal halaman").toMatchObject({ overflow: false });
      expect(await offscreen(page), "elemen di luar layar").toEqual([]);
    });

    test("Harga: axis labels and statistics are not clipped", async ({ page }) => {
      await hugeMocks(page);
      await company(page, "COAL", "harga");
      expect(await horizontalOverflow(page), "gulir horizontal halaman").toMatchObject({ overflow: false });
      const m = await page.evaluate(() => {
        const vw = document.documentElement.clientWidth;
        const svg = document.querySelector("svg[role=img]")!.getBoundingClientRect();
        const labels = Array.from(document.querySelectorAll<SVGTextElement>("svg[role=img] text")).map((t) => {
          const r = t.getBoundingClientRect();
          return { text: t.textContent, left: r.left, right: r.right, over: r.left < svg.left - 0.5 || r.right > svg.right + 0.5 };
        });
        const stats = Array.from(document.querySelectorAll<HTMLElement>("[data-testid=price-stats] .num")).map((el) => ({ text: el.textContent, clipped: el.scrollWidth > el.clientWidth + 1 }));
        return { vw, labels, stats };
      });
      expect(m.labels.length, "label sumbu ada").toBeGreaterThan(0);
      for (const l of m.labels) {
        expect(l.over, `label sumbu "${l.text}" keluar dari kotak grafik`).toBe(false);
        expect(l.left, `label sumbu "${l.text}" terpotong di kiri`).toBeGreaterThanOrEqual(0);
        expect(l.right, `label sumbu "${l.text}" terpotong di kanan`).toBeLessThanOrEqual(m.vw);
      }
      expect(m.stats.filter((s) => s.clipped), "angka statistik terpotong").toEqual([]);
    });

    test("Keuangan: the table scrolls inside its box, the page does not", async ({ page }) => {
      await hugeMocks(page);
      await company(page, "COAL", "keuangan");
      await page.getByRole("table").first().waitFor();
      expect(await horizontalOverflow(page), "gulir horizontal halaman").toMatchObject({ overflow: false });
    });
  });

  for (const scenario of ["long-names", "long-spaces"] as const) {
    test.describe(`${scenario}: long company names at ${width}px`, () => {
      test.use({ viewport: { width, height: 852 } });

      /** 240 karakter tanpa spasi ("long-names") atau frasa berspasi yang diulang ("long-spaces"). */
      async function longMocks(page: Page) {
        const noSpace = "PT" + "Batubara".repeat(30) + "Tbk";
        const spaced = "PT Batubara Nusantara Sejahtera Makmur Abadi Jaya Bersama Sentosa Tambang Energi Indonesia Raya ".repeat(3).trim();
        const long = scenario === "long-names" ? noSpace : spaced;
        await mockApi(page);
        const desk = golden("desk");
        desk.coverage = desk.coverage.map((c: any, i: number) => ({ ...c, business_note: i % 2 ? spaced : long, pack: c.pack ? { ...c.pack, label: long.slice(0, 120) } : c.pack }));
        await overrideApi(page, "/api/research/desk", { body: desk });
        const t = golden("ticker-COAL");
        t.name = long;
        t.business_note = spaced;
        t.pack.label = long.slice(0, 100);
        t.changes = t.changes.map((c: any) => ({ ...c, text: long }));
        await overrideApi(page, "/api/research/COAL", { body: t });
        const claims = golden("claims-COAL");
        claims.items = claims.items.map((i: any) => ({ ...i, statement: long, preview: spaced }));
        await overrideApi(page, "/api/research/COAL/claims", { body: claims });
        const j = golden("journal-COAL");
        j.items = Array.from({ length: 6 }, (_, i) => ({ ...j.items[i % j.items.length], id: `j-${i}`, text: i % 2 ? long.repeat(6).slice(0, 1900) : spaced }));
        await overrideApi(page, "/api/research/COAL/journal", { body: j });
      }

      test("Perusahaan list does not widen", async ({ page }) => {
        await longMocks(page);
        await ready(page, "/perusahaan", () => page.getByRole("heading", { name: "Perusahaan", exact: true }).waitFor());
        expect(await horizontalOverflow(page), "gulir horizontal halaman").toMatchObject({ overflow: false });
        expect(await offscreen(page), "elemen di luar layar").toEqual([]);
      });

      for (const section of ["struktur", "klaim", "jurnal"]) {
        test(`company page ${section} does not widen`, async ({ page }) => {
          await longMocks(page);
          await ready(page, `/perusahaan/COAL?section=${section}`, () => page.locator(`section[data-block="${section}"]`).waitFor());
          await page.waitForTimeout(300);
          expect(await horizontalOverflow(page), "gulir horizontal halaman").toMatchObject({ overflow: false });
          expect(await offscreen(page), "elemen di luar layar").toEqual([]);
        });
      }

      test("claim sheet does not widen", async ({ page }) => {
        await longMocks(page);
        await ready(page, "/perusahaan/COAL?section=klaim", () => page.locator("button[data-claim-status]").first().waitFor());
        await page.locator("button[data-claim-status]").first().click();
        const dialog = page.getByRole("dialog");
        await dialog.waitFor();
        await page.waitForTimeout(300);
        expect(await horizontalOverflow(page), "gulir horizontal halaman").toMatchObject({ overflow: false });
        const box = await dialog.evaluate((e) => {
          const r = e.getBoundingClientRect();
          return { left: r.left, right: r.right, top: r.top, scrollW: e.scrollWidth, clientW: e.clientWidth };
        });
        expect(box.left, "sheet keluar di kiri").toBeGreaterThanOrEqual(-0.5);
        expect(box.right, "sheet keluar di kanan").toBeLessThanOrEqual(width + 0.5);
        expect(box.top, "sheet terpotong di atas").toBeGreaterThanOrEqual(0);
        expect(box.scrollW, "isi sheet lebih lebar dari panelnya").toBeLessThanOrEqual(box.clientW + 1);
      });
    });
  }
}

test("company page COAL screenshot: top of the page (viewport with the tab bar)", async ({ page }) => {
  await mockApi(page);
  await company(page, "COAL");
  await page.screenshot({ path: `${SHOTS}perusahaan-COAL-atas.png`, scale: "css" });
});
