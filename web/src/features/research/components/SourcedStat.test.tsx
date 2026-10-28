import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SourcedStat } from "./SourcedStat";

/** WCAG 2.5.3 as axe `label-content-name-mismatch` checks it: the words of the visible text are a contiguous run of the name's words. */
const words = (s: string) => s.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, " ").trim().split(" ");
const contains = (name: string, visible: string) => {
  const n = words(name);
  const v = words(visible);
  return n.some((_, i) => v.every((w, j) => n[i + j] === w));
};

describe("SourcedStat accessible name (WCAG 2.5.3)", () => {
  it.each([
    ["Kap. pasar", "Kapitalisasi pasar", "Rp645.000"],
    ["Utang bersih Q2 26", "Utang bersih", "US$86,0 jt"],
    ["Laba bersih Q2 26 (YoY)", "Laba bersih kuartal terakhir", "US$21,8 jt · +7,9%"],
  ])("%s: the label-first name starts with the visible label and value, then adds the provenance", (label, name, value) => {
    render(<SourcedStat labelFirst label={label} name={name} value={value} source="GET /COAL" gapReason="Belum ada" />);
    const button = screen.getByRole("button");
    expect(button).toHaveAccessibleName(`${label} ${value}, asal angka ${name}`);
    // The visible text keeps a real space between label and value, so its words do not fuse ("pasarRp645").
    expect(contains(`${label} ${value}, asal angka ${name}`, button.textContent ?? "")).toBe(true);
    expect(words(button.textContent ?? "")).toEqual(words(`${label} ${value}`));
  });

  it("the stacked grid cell keeps the same rule", () => {
    render(<SourcedStat stacked labelFirst label="Tertinggi 52 minggu (Rp)" name="Tertinggi 52 minggu" value="1.310" source="GET /COAL" gapReason="Belum ada" />);
    expect(screen.getByRole("button")).toHaveAccessibleName("Tertinggi 52 minggu (Rp) 1.310, asal angka Tertinggi 52 minggu");
  });

  it("a gap is an em dash without a button", () => {
    render(<SourcedStat label="Kap. pasar" name="Kapitalisasi pasar" value={null} source="GET /COAL" gapReason="Jumlah saham belum ada" />);
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getByText("Jumlah saham belum ada")).toBeInTheDocument();
  });
});
