import type { Page } from "@playwright/test";

/**
 * Screenshot of the whole page. A full-page capture stitches the page into one tall image, and the `position: fixed`
 * tab bar then freezes at the bottom of the first viewport, on top of whatever content sits there. Hiding the bar for
 * the capture shows the content as it really lays out; a screenshot that must show the bar is a plain viewport
 * screenshot, where the bar sits at the bottom edge as on the phone.
 */
export async function fullPageShot(page: Page, path: string) {
  const hide = await page.addStyleTag({ content: 'nav[aria-label="Navigasi utama"]{display:none !important}' });
  try {
    await page.screenshot({ path, fullPage: true, scale: "css" });
  } finally {
    await hide.evaluate((el) => el.remove());
  }
}
