import { expect, test, type Page } from "@playwright/test";
import { mockApi, overrideApi } from "./mockApi";

/**
 * Navigation state (design.md §2.4, D8; plan §4 L2): rules 3 and 5 to 9 on the placeholder routes, and the sandbox
 * line of the unlock screen (design.md §3.6, D7). Screens are placeholders, so there is no in-page link to push a
 * screen: `push` does what a router `Link` does (a history entry carrying React Router's `idx`, then `popstate`).
 */
const NAV = 'nav[aria-label="Navigasi utama"]';
const hashOf = (page: Page) => new URL(page.url()).hash;

async function open(page: Page, hash: string) {
  await mockApi(page);
  await page.goto(`/#${hash}`);
  await expect(page.locator(NAV)).toBeVisible();
  await page.waitForLoadState("networkidle");
  // Placeholder screens are short; give the document something to scroll.
  await page.addStyleTag({ content: "body{min-height:3000px}" });
}

/** An in-app push: `history.state.idx` goes up by one, as with `navigate(to)`. */
async function push(page: Page, hash: string) {
  await page.evaluate((h) => {
    const idx = (window.history.state?.idx ?? 0) + 1;
    window.history.pushState({ usr: null, key: Math.random().toString(36).slice(2, 10), idx }, "", `#${h}`);
    window.dispatchEvent(new PopStateEvent("popstate", { state: window.history.state }));
  }, hash);
  await expect.poll(() => hashOf(page)).toBe(`#${hash}`);
}

/**
 * A user's scroll: resolves once the `scroll` event is delivered, as it is long before a finger can tap. The shell
 * tracks the position from that event (rule 5), and WebKit delivers it on the next frame, so a `push` in the same
 * frame as `window.scrollTo` would save the old position.
 */
const scrollTo = async (page: Page, y: number) => {
  await page.evaluate(
    (to) =>
      new Promise<void>((resolve) => {
        if (Math.round(window.scrollY) === to) return resolve();
        window.addEventListener("scroll", () => resolve(), { once: true });
        window.scrollTo(0, to);
      }),
    y,
  );
  await expect.poll(() => page.evaluate(() => Math.round(window.scrollY))).toBe(y);
};
const scrollY = (page: Page) => page.evaluate(() => Math.round(window.scrollY));
const tab = (page: Page, label: string) => page.locator(NAV).getByRole("link", { name: label });
const back = (page: Page, label: string) => page.getByRole("link", { name: `Kembali ke ${label}` });

test.describe("pemilik tab (aturan 3)", () => {
  for (const [hash, owner] of [
    ["/beranda", "Beranda"],
    ["/portofolio", "Portofolio"],
    ["/perusahaan", "Perusahaan"],
    ["/perusahaan/tambah", "Perusahaan"],
    ["/perusahaan/COAL", "Perusahaan"],
    ["/perusahaan/COAL?from=perusahaan", "Perusahaan"],
    ["/perusahaan/COAL?from=portofolio", "Portofolio"],
    ["/perusahaan/COAL?from=beranda", "Beranda"],
    ["/alat", "Alat"],
    ["/alat/kalibrasi", "Alat"],
    ["/alat/paket?from=beranda", "Beranda"],
  ] as const) {
    test(`${hash} menyorot ${owner}`, async ({ page }) => {
      await open(page, hash);
      await expect(page.locator(`${NAV} a[aria-current="page"]`)).toHaveText(owner);
    });
  }

  test("`from` yang bukan tab diabaikan: tab milik rutenya", async ({ page }) => {
    await open(page, "/perusahaan/COAL?from=riset");
    await expect(page.locator(`${NAV} a[aria-current="page"]`)).toHaveText("Perusahaan");
  });

  test("layar yang didorong dari Portofolio menulis ke memori Portofolio, bukan Perusahaan", async ({ page }) => {
    await open(page, "/portofolio");
    await push(page, "/perusahaan/COAL?from=portofolio");
    await tab(page, "Beranda").click();
    await expect.poll(() => hashOf(page)).toBe("#/beranda");
    await tab(page, "Portofolio").click();
    await expect.poll(() => hashOf(page)).toBe("#/perusahaan/COAL?from=portofolio");
    await tab(page, "Perusahaan").click();
    await expect.poll(() => hashOf(page), "memori Perusahaan kosong: akarnya").toBe("#/perusahaan");
  });
});

test.describe("scroll dan memori (aturan 5, 6)", () => {
  test("berpindah tab menyimpan scroll yang ditinggalkan dan memulihkannya (aturan 5)", async ({ page }) => {
    await open(page, "/beranda");
    await scrollTo(page, 650);
    await tab(page, "Portofolio").click();
    await expect.poll(() => hashOf(page)).toBe("#/portofolio");
    await expect.poll(() => scrollY(page), "layar baru mulai di 0").toBe(0);
    await tab(page, "Beranda").click();
    await expect.poll(() => hashOf(page)).toBe("#/beranda");
    await expect.poll(() => scrollY(page), "scroll Beranda pulih").toBe(650);
  });

  test("layar yang didorong mulai di 0 dan scroll asalnya tersimpan (aturan 5)", async ({ page }) => {
    await open(page, "/alat");
    await scrollTo(page, 400);
    await push(page, "/alat/paket");
    await expect.poll(() => scrollY(page)).toBe(0);
  });

  test("tab lain membuka lastHash-nya (aturan 6)", async ({ page }) => {
    await open(page, "/perusahaan/COAL");
    await tab(page, "Portofolio").click();
    await expect.poll(() => hashOf(page)).toBe("#/portofolio");
    await tab(page, "Perusahaan").click();
    await expect.poll(() => hashOf(page)).toBe("#/perusahaan/COAL");
    await expect(page.locator(`${NAV} a[aria-current="page"]`)).toHaveText("Perusahaan");
  });

  test("tab yang belum pernah dikunjungi membuka akarnya (aturan 6)", async ({ page }) => {
    await open(page, "/beranda");
    for (const [label, hash] of [["Alat", "#/alat"], ["Perusahaan", "#/perusahaan"], ["Portofolio", "#/portofolio"]]) {
      await tab(page, label).click();
      await expect.poll(() => hashOf(page)).toBe(hash);
    }
  });

  test("tab-tab menjaga lastHash masing-masing saat berganti-ganti (aturan 6)", async ({ page }) => {
    await open(page, "/alat");
    await push(page, "/alat/kalibrasi");
    await tab(page, "Beranda").click();
    await expect.poll(() => hashOf(page)).toBe("#/beranda");
    await tab(page, "Alat").click();
    await expect.poll(() => hashOf(page)).toBe("#/alat/kalibrasi");
  });
});

test.describe("tab aktif (aturan 7)", () => {
  test("di akar tab: gulir ke 0 dan tetap di akar", async ({ page }) => {
    await open(page, "/alat");
    await scrollTo(page, 300);
    await tab(page, "Alat").click();
    await expect.poll(() => scrollY(page)).toBe(0);
    expect(hashOf(page)).toBe("#/alat");
  });

  test("di layar yang didorong: ke akar tab di posisi 0, scroll akar dilupakan", async ({ page }) => {
    await open(page, "/alat");
    await scrollTo(page, 400);
    await push(page, "/alat/paket");
    await tab(page, "Alat").click();
    await expect.poll(() => hashOf(page)).toBe("#/alat");
    await expect.poll(() => scrollY(page), "400 yang tersimpan dilupakan").toBe(0);
    // Dan tetap lupa: pergi dan kembali ke tab ini tidak memulihkan 400.
    await tab(page, "Beranda").click();
    await expect.poll(() => hashOf(page)).toBe("#/beranda");
    await tab(page, "Alat").click();
    await expect.poll(() => hashOf(page)).toBe("#/alat");
    await expect.poll(() => scrollY(page)).toBe(0);
  });
});

test.describe("tombol kembali (aturan 8, 9)", () => {
  test("setelah dorongan dalam app: label ‹ pemilik, nama 'Kembali ke …', navigate(-1) dan scroll pulih (aturan 8)", async ({ page }) => {
    await open(page, "/beranda");
    await scrollTo(page, 650);
    await push(page, "/perusahaan/COAL?from=beranda");
    const control = back(page, "Beranda");
    await expect(control).toBeVisible();
    await expect(control).toHaveText("Beranda");
    const box = await control.boundingBox();
    expect(box?.height, "tinggi kontrol kembali").toBeGreaterThanOrEqual(44);
    await expect(page.locator(`${NAV} a[aria-current="page"]`)).toHaveText("Beranda");
    await control.click();
    await expect.poll(() => hashOf(page)).toBe("#/beranda");
    await expect.poll(() => scrollY(page), "scroll Beranda pulih lewat aturan 5").toBe(650);
  });

  test("navigate(-1) memang mundur satu entri riwayat (aturan 8)", async ({ page }) => {
    await open(page, "/alat");
    await push(page, "/alat/bandingkan");
    const before = await page.evaluate(() => window.history.length);
    await back(page, "Alat").click();
    await expect.poll(() => hashOf(page)).toBe("#/alat");
    expect(await page.evaluate(() => window.history.length), "tidak menambah entri").toBe(before);
  });

  test("kembali lewat memori tab setelah pindah tab: ‹ Perusahaan ke akar Perusahaan, bukan ke tab lain (aturan 8)", async ({ page }) => {
    await open(page, "/perusahaan");
    await scrollTo(page, 500);
    await push(page, "/perusahaan/COAL");
    await tab(page, "Alat").click();
    await expect.poll(() => hashOf(page)).toBe("#/alat");
    await tab(page, "Perusahaan").click();
    await expect.poll(() => hashOf(page)).toBe("#/perusahaan/COAL");
    const before = await page.evaluate(() => window.history.length);
    await back(page, "Perusahaan").click();
    await expect.poll(() => hashOf(page), "entri sebelumnya milik Alat: tidak navigate(-1)").toBe("#/perusahaan");
    await expect.poll(() => scrollY(page), "scroll akar Perusahaan pulih lewat aturan 5").toBe(500);
    expect(await page.evaluate(() => window.history.length), "replace, bukan push").toBe(before);
    await expect(page.locator(`${NAV} a[aria-current="page"]`)).toHaveText("Perusahaan");
  });

  test("tautan dalam yang dibuka dingin dari Perusahaan: ‹ Perusahaan ke akarnya (aturan 9)", async ({ page }) => {
    await open(page, "/perusahaan/COAL");
    await expect(back(page, "Perusahaan")).toHaveText("Perusahaan");
    await back(page, "Perusahaan").click();
    await expect.poll(() => hashOf(page)).toBe("#/perusahaan");
  });

  test("alat dibuka dingin: ‹ Alat ke #/alat (aturan 9)", async ({ page }) => {
    await open(page, "/alat/kalibrasi");
    await back(page, "Alat").click();
    await expect.poll(() => hashOf(page)).toBe("#/alat");
  });

  test("Tambah emiten dibuka dingin: ‹ Perusahaan (aturan 9)", async ({ page }) => {
    await open(page, "/perusahaan/tambah");
    await back(page, "Perusahaan").click();
    await expect.poll(() => hashOf(page)).toBe("#/perusahaan");
  });

  test("dibuka dingin dengan from=portofolio: ‹ Portofolio ke akar Portofolio (aturan 3 dan 9)", async ({ page }) => {
    await open(page, "/perusahaan/COAL?from=portofolio");
    await back(page, "Portofolio").click();
    await expect.poll(() => hashOf(page)).toBe("#/portofolio");
  });

  test("layar akar tab tidak punya kontrol kembali", async ({ page }) => {
    await open(page, "/perusahaan");
    await expect(page.getByRole("link", { name: /^Kembali ke/ })).toHaveCount(0);
  });
});

test.describe("baris sandbox di layar buka kunci (design.md §3.6, D7)", () => {
  const LINE = "Uji coba: perubahan di sini tidak dibawa ke data asli.";

  async function locked(page: Page, health: { status?: number; body?: unknown }) {
    await mockApi(page);
    await overrideApi(page, "/api/session", { status: 401, body: { detail: "unauthorized" } });
    await overrideApi(page, "/api/health", health);
    await page.goto("/#/beranda");
    await expect(page.getByRole("heading", { name: "Buka kunci Investasi" })).toBeVisible();
    await page.waitForLoadState("networkidle");
  }

  test("ada bila /api/health mengatakan sandbox: true", async ({ page }) => {
    await locked(page, { body: { status: "ok", sandbox: true } });
    await expect(page.getByText(LINE)).toBeVisible();
  });

  test("tidak ada bila /api/health tanpa sandbox", async ({ page }) => {
    await locked(page, { body: { status: "ok" } });
    await expect(page.getByText(LINE)).toHaveCount(0);
  });

  test("tidak ada bila sandbox: false atau /api/health gagal", async ({ page }) => {
    await locked(page, { body: { status: "ok", sandbox: false } });
    await expect(page.getByText(LINE)).toHaveCount(0);
    const failing = await page.context().newPage();
    await mockApi(failing);
    await overrideApi(failing, "/api/session", { status: 401, body: { detail: "unauthorized" } });
    await overrideApi(failing, "/api/health", { status: 503, body: { sandbox: true } });
    await failing.goto("/#/beranda");
    await expect(failing.getByRole("heading", { name: "Buka kunci Investasi" })).toBeVisible();
    await failing.waitForLoadState("networkidle");
    await expect(failing.getByText(LINE)).toHaveCount(0);
  });
});

test.describe("jangkauan S4 (prd.md §5 S4)", () => {
  test("setiap tempat di §2 satu ketukan dari bar bawah", async ({ page }) => {
    await open(page, "/beranda");
    for (const [label, hash, heading] of [
      ["Portofolio", "#/portofolio", "Portofolio"],
      ["Perusahaan", "#/perusahaan", "Perusahaan"],
      ["Alat", "#/alat", "Alat"],
      ["Beranda", "#/beranda", "Beranda"],
    ] as const) {
      await tab(page, label).click();
      await expect.poll(() => hashOf(page)).toBe(hash);
      await expect(page.getByRole("heading", { level: 1, name: heading, exact: true })).toBeVisible();
      await expect(page.locator(`${NAV} a[aria-current="page"]`)).toHaveText(label);
    }
  });

  test("halaman perusahaan <= 2 ketukan dari Portofolio (posisi -> Buka halaman perusahaan -> ‹ Portofolio)", async ({ page }) => {
    await open(page, "/portofolio");
    await expect(page.getByRole("heading", { level: 1, name: "Portofolio", exact: true })).toBeVisible();
    // Ketukan 1: baris posisi membuka Sheet posisi.
    await page.getByRole("button", { name: /^COAL\s.*\blot\b/ }).click();
    const dialog = page.getByRole("dialog", { name: "COAL" });
    await expect(dialog).toBeVisible();
    // Ketukan 2: Buka halaman perusahaan membuka rute perusahaan dengan BackHeader ‹ Portofolio.
    await dialog.getByRole("link", { name: "Buka halaman perusahaan" }).click();
    await expect.poll(() => hashOf(page)).toBe("#/perusahaan/COAL?from=portofolio");
    await expect(back(page, "Portofolio")).toHaveText("Portofolio");
    await expect(page.locator(`${NAV} a[aria-current="page"]`)).toHaveText("Portofolio");
  });

  test("halaman perusahaan <= 2 ketukan dari item kotak masuk Beranda (item -> Buka halaman perusahaan -> ‹ Beranda)", async ({ page }) => {
    await open(page, "/beranda");
    // Ketukan 1: baris kotak masuk membuka Sheet item.
    const firstInbox = page.locator('section[data-block="perlu-perhatian"] [data-event-id] button').first();
    await firstInbox.waitFor();
    await firstInbox.click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    // Ketukan 2: Buka halaman perusahaan membuka rute perusahaan dengan BackHeader ‹ Beranda.
    await dialog.getByRole("button", { name: "Buka halaman perusahaan" }).click();
    await expect.poll(() => hashOf(page)).toMatch(/^#\/perusahaan\/[A-Z]+\?.*from=beranda/);
    await expect(back(page, "Beranda")).toHaveText("Beranda");
    await expect(page.locator(`${NAV} a[aria-current="page"]`)).toHaveText("Beranda");
  });

  test("halaman perusahaan <= 2 ketukan dari Perusahaan (1 ketukan ke emiten; masih placeholder di cabang L3c)", async ({ page }) => {
    await open(page, "/perusahaan");
    const companyLink = page.locator("[data-company] a").first();
    if ((await companyLink.count()) > 0) {
      await companyLink.click();
      await expect.poll(() => hashOf(page)).toMatch(/^#\/perusahaan\/[A-Z]+$/);
    } else {
      await push(page, "/perusahaan/COAL");
    }
    await expect(back(page, "Perusahaan")).toHaveText("Perusahaan");
    await expect(page.locator(`${NAV} a[aria-current="page"]`)).toHaveText("Perusahaan");
  });
});
