import { expect, test, type Page } from "@playwright/test";
import { textCollisions } from "./audits";

/**
 * `textCollisions` on fixtures built with `page.setContent` (no app, no mocks). A multi-line inline element's bounding
 * box is the union of its line boxes, so it used to "overlap" an inline sibling sharing a line with it: the gate has to
 * compare line boxes (`getClientRects`), not bounding boxes. Out-of-flow siblings float over content by design and are
 * skipped, except when both are out-of-flow: two deliberately placed texts must not overlap each other.
 */
const show = (page: Page, body: string, css = "") =>
  page.setContent(`<!doctype html><meta name="viewport" content="width=device-width,initial-scale=1"><style>body{margin:0;font:16px/20px sans-serif}${css}</style>${body}`);

test.describe("textCollisions", () => {
  test("a wrapped inline sentence next to its bold lead-in is not a collision", async ({ page }) => {
    // The live "Data belum ada" row: <span bold>KPI belum diisi:</span> <span>Isi lewat tabel Kuartal terakhir.</span> in a narrow li.
    await show(
      page,
      `<ul style="margin:0;padding:0;list-style:none"><li style="width:170px"><span style="font-weight:600">KPI belum diisi:</span> <span>Isi lewat tabel Kuartal terakhir untuk produksi kuartal ini.</span></li></ul>`,
    );
    const lines = await page.evaluate(() => Array.from(document.querySelectorAll("li > span")).map((s) => s.getClientRects().length));
    expect(lines[1], "fixture must wrap the second span onto several lines").toBeGreaterThan(1);
    expect(await textCollisions(page)).toEqual([]);
  });

  test("two absolutely positioned spans overlapping by 10 px are reported", async ({ page }) => {
    await show(
      page,
      `<div style="position:relative;height:60px"><span style="position:absolute;left:0;top:0;width:100px;height:20px">Kiri</span><span style="position:absolute;left:90px;top:0;width:100px;height:20px">Kanan</span></div>`,
    );
    const found = await textCollisions(page);
    expect(found).toHaveLength(1);
    expect(found[0].a).toContain("Kiri");
    expect(found[0].b).toContain("Kanan");
  });

  test("absolutely positioned spans that only touch are not reported", async ({ page }) => {
    await show(
      page,
      `<div style="position:relative;height:60px"><span style="position:absolute;left:0;top:0;width:100px;height:20px">Kiri</span><span style="position:absolute;left:100px;top:0;width:100px;height:20px">Kanan</span></div>`,
    );
    expect(await textCollisions(page)).toEqual([]);
  });

  test("an out-of-flow element over in-flow content is still ignored (sticky bars, overlays)", async ({ page }) => {
    await show(page, `<div><p style="margin:0;height:40px">Isi halaman</p><span style="position:absolute;left:0;top:0;width:100px;height:20px">Melayang</span></div>`);
    expect(await textCollisions(page)).toEqual([]);
  });

  test("two blocks pulled over each other with a negative margin are reported", async ({ page }) => {
    await show(page, `<div><p style="margin:0;height:20px">Atas</p><p style="margin:-10px 0 0;height:20px">Bawah</p></div>`);
    expect(await textCollisions(page)).toHaveLength(1);
  });

  test("an inline-block shifted 8 px into its neighbour is reported", async ({ page }) => {
    await show(page, `<div><span style="display:inline-block;width:80px">Satu</span><span style="display:inline-block;width:80px;position:relative;left:-8px">Dua</span></div>`);
    expect(await textCollisions(page)).toHaveLength(1);
  });

  test("a wrapped inline element that really runs under its inline sibling is reported", async ({ page }) => {
    // The second span is lifted back onto the first span's first line with a relative offset: its line rect intersects.
    await show(page, `<p style="margin:0;width:120px"><span>aaaa bbbb cccc dddd eeee</span><span style="position:relative;top:-20px;left:-60px">XXXXXXXX</span></p>`);
    expect((await textCollisions(page)).length).toBeGreaterThan(0);
  });

  test("tolerance: 1 px of overlap is not reported, 2 px is", async ({ page }) => {
    await show(page, `<div><p style="margin:0;height:20px">Atas</p><p style="margin:-1px 0 0;height:20px">Bawah</p></div>`);
    expect(await textCollisions(page)).toEqual([]);
    await show(page, `<div><p style="margin:0;height:20px">Atas</p><p style="margin:-2px 0 0;height:20px">Bawah</p></div>`);
    expect(await textCollisions(page)).toHaveLength(1);
  });
});
