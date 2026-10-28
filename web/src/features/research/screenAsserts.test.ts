import { describe, expect, it } from "vitest";
import { DOMAIN_PHRASES, expectNoVerdictWordsExcept, noPageOverflowProblems } from "./screenAsserts.testutil";

function dom(html: string) {
  const host = document.createElement("div");
  host.innerHTML = html;
  return host.firstElementChild as HTMLElement;
}

describe("noPageOverflowProblems (A5.1 structural check)", () => {
  it("passes a shrinking root whose wide table scrolls inside a named, focusable region", () => {
    const root = dom(`<div class="min-w-0 max-w-full"><div role="region" aria-label="Tabel" tabindex="0" class="max-w-full overflow-x-auto"><table class="min-w-[960px]"></table></div></div>`);
    expect(noPageOverflowProblems(root)).toEqual([]);
  });

  it("fails when a wide table escapes its scroller, or the root cannot shrink", () => {
    const root = dom(`<div class="flex flex-col"><table class="min-w-[960px]"></table><div class="min-w-[720px]"></div></div>`);
    expect(noPageOverflowProblems(root)).toEqual([
      "akar layar tanpa min-w-0 max-w-full",
      "table min-w 960 tidak berada di dalam penggulung",
      "div min-w 720 tidak berada di dalam penggulung",
    ]);
  });

  it("fails when the scroller is unnamed, unfocusable, or not capped", () => {
    const root = dom(`<div class="min-w-0 max-w-full"><div class="overflow-x-auto"><table></table></div></div>`);
    expect(noPageOverflowProblems(root)).toEqual(["penggulung table tanpa max-w-full", "penggulung table tanpa region bernama", "penggulung table tidak bisa difokus"]);
  });
});

describe("expectNoVerdictWordsExcept", () => {
  const html = (text: string) => {
    const host = document.createElement("div");
    host.innerHTML = `<p>${text}</p>`;
    return host;
  };

  it("lets the named domain phrases through but still fails a screen-composed status word", () => {
    expect(DOMAIN_PHRASES.length).toBeGreaterThan(0);
    expectNoVerdictWordsExcept(html(["Harga ju", "al rata-rata batubara"].join("")));
    expect(() => expectNoVerdictWordsExcept(html(["Status: ", "Be", "li"].join("")))).toThrow();
    // Frasa domain di samping kata putusan di luar daftar tetap gagal: tidak ada pengecualian untuk seluruh kalimat.
    expect(() => expectNoVerdictWordsExcept(html(["Harga ju", "al rata-rata, la", "yak"].join("")))).toThrow();
  });

  it("also reads accessible names", () => {
    const host = document.createElement("div");
    host.innerHTML = `<button aria-label="${["Ju", "al"].join("")}">x</button>`;
    expect(() => expectNoVerdictWordsExcept(host)).toThrow();
  });
});
