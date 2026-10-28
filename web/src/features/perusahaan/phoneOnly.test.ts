import { describe, expect, it } from "vitest";

/**
 * D13 (design.md §1): the Mac window shows the same 480 px column as the phone, so the screens L3b owns carry no
 * `≥ 1024 px` variant: no `lg:`/`xl:`/`2xl:` utility, no viewport-width constant and no `min-width` media query.
 */
const SOURCES = {
  ...import.meta.glob("./**/*.{ts,tsx}", { query: "?raw", import: "default", eager: true }),
  ...import.meta.glob("../riset/**/*.{ts,tsx}", { query: "?raw", import: "default", eager: true }),
  ...import.meta.glob("../thesis/**/*.{ts,tsx}", { query: "?raw", import: "default", eager: true }),
  ...import.meta.glob("../research/components/**/*.{ts,tsx}", { query: "?raw", import: "default", eager: true }),
} as Record<string, string>;

const production = () => Object.entries(SOURCES).filter(([file]) => !/\.test(util)?\.tsx?$/.test(file));

const DESKTOP = [/(?:^|[\s"'`:])(?:lg|xl|2xl):[\w[-]/, /DESKTOP_MIN_WIDTH/, /min-width\s*:\s*\d{4}/, /useIsDesktop|useShell/];

describe("phone layout at every width (D13)", () => {
  it("scans the L3b screens, not an empty glob", () => {
    const files = production().map(([file]) => file);
    expect(files.length).toBeGreaterThan(40);
    expect(files).toContain("../riset/StructureMindMap.tsx");
    expect(files).toContain("../research/components/Financials.tsx");
    expect(files).toContain("./Perusahaan.tsx");
  });

  it("the pattern finds each spelling of a desktop branch", () => {
    for (const hit of ['className="flex lg:flex-row"', 'cn("a", "xl:p-4")', "const DESKTOP_MIN_WIDTH = 1024;", "window.matchMedia('(min-width: 1024px)')", "useIsDesktop()"]) {
      expect(
        DESKTOP.some((re) => re.test(hit)),
        hit,
      ).toBe(true);
    }
    for (const miss of ['className="text-title"', "rounded-lg", "var(--radius-lg)", "const small = 1"]) {
      expect(
        DESKTOP.some((re) => re.test(miss)),
        miss,
      ).toBe(false);
    }
  });

  it("no L3b screen branches on a window width of 1024 px or more", () => {
    const hits = production().flatMap(([file, source]) => source.split("\n").flatMap((line, i) => (DESKTOP.some((re) => re.test(line)) ? [`${file}:${i + 1}: ${line.trim().slice(0, 100)}`] : [])));
    expect(hits).toEqual([]);
  });
});
