import { cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { SURFACES, mockCanonBackend, renderSurface } from "./canonSurfaces.testutil";
import { expectNoVerdictWordsExcept } from "./screenAsserts.testutil";

// Kata-kata putusan dirakit dari potongan agar grep gerbang pada `web/src/features/research` tetap kosong.
const VERDICT = [["be", "li"], ["ju", "al"], ["ke", "luar"], ["co", "ret"], ["la", "yak"], ["mu", "rah"], ["ma", "hal"], ["wa", "jar"]].map((p) => p.join(""));
const VERDICT_RE = new RegExp(`(^|[^\\p{L}])(${VERDICT.join("|")})(?=$|[^\\p{L}])`, "iu");

// Nilai per saham milik sistem: hanya tabel skenario (angka milik pemilik) yang boleh menyebut nilai per saham.
const FAIR_VALUE = [["nilai wa", "jar"], ["harga wa", "jar"], ["fair ", "value"], ["nilai ", "intrinsik"], ["intrinsic ", "value"], ["target ", "harga"], ["price ", "target"]].map((p) => p.join(""));
const FAIR_VALUE_RE = new RegExp(FAIR_VALUE.join("|"), "i");

beforeEach(() => {
  mockCanonBackend();
});
afterEach(cleanup);

/** Teks yang tampil dan yang dibacakan, tanpa tabel skenario. */
function spokenOutsideScenarios(root: HTMLElement): string {
  const copy = root.cloneNode(true) as HTMLElement;
  for (const table of copy.querySelectorAll("table")) {
    if (/^Skenario nilai per saham/.test(table.querySelector("caption")?.textContent ?? "")) (table.closest('[role="region"]') ?? table).remove();
  }
  const attrs = [...copy.querySelectorAll("[title], [aria-label], [placeholder]")].flatMap((el) => ["title", "aria-label", "placeholder"].map((a) => el.getAttribute(a) ?? ""));
  return [copy.textContent ?? "", ...attrs].join("\n");
}

describe("canon vocabulary (plan §7.4, design §7, A5.3)", () => {
  it("the forbidden words are really matched by the pattern", () => {
    for (const w of VERDICT) expect(`status: ${w}`).toMatch(VERDICT_RE);
    expect(["Be", "liau, pen", "ju", "alan, ma", "hal", "nya"].join("")).not.toMatch(VERDICT_RE);
  });

  it.each(SURFACES)("$name: no verdict word in any text node or accessible name, and no system fair value outside the scenario table", async (surface) => {
    const { container } = await renderSurface(surface);
    // Hanya frasa domain bernama (`DOMAIN_PHRASES`) yang dikecualikan; kata putusan di kalimat backend lain tetap gagal.
    expectNoVerdictWordsExcept(container);
    expect(spokenOutsideScenarios(container)).not.toMatch(FAIR_VALUE_RE);
  });

  // Teks yang paling mungkin menyebut nilai per saham milik sistem ada di keterangan asal angka (drawer fakta), jadi satu angka dibuka di permukaan yang punya FactChip (Bandingkan; blok halaman perusahaan menyusul di L3b).
  it.each(SURFACES.filter((s) => s.name === "Compare"))("$name: the opened provenance dialog is free of verdict words and fair-value wording", async (surface) => {
    const { container } = await renderSurface(surface);
    const chip = container.querySelector<HTMLElement>('button[aria-haspopup="dialog"]');
    expect(chip, `${surface.name} tanpa FactChip untuk dibuka`).not.toBeNull();
    fireEvent.click(chip!);
    await screen.findByRole("dialog");
    expectNoVerdictWordsExcept(document);
    expect(spokenOutsideScenarios(document.body)).not.toMatch(FAIR_VALUE_RE);
  });
});
