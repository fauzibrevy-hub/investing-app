import { fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import desk from "../../../../../tests/fixtures/research/canon/desk.json";
import { AppRoutes } from "../../../App";
import { mockApi } from "../../../test/mockApi";
import { MACRO } from "../../research/desk/macro.testutil";
import { setViewport } from "../../research/testing/viewport";
import type { CoverageRow, DeskResponse } from "../../research/types";
import { groupCoverage, latestReportQuarter, previousQuarter } from "./landingData";

/**
 * What is left of finance-app's Riset screens test (plan.md §3.3 "Tests"): the Landing cases moved to
 * `beranda/Beranda.test.tsx` and `perusahaan/CoverageList.test.tsx`, the Bandingkan, Kalibrasi and Paket cases to
 * `tools.test.tsx` (on `/alat/*`), and the Tambah case belongs to L3b. This file keeps `landingData` and the one
 * inbox case the new tests do not repeat, now on `#/beranda`.
 */
const DESK = desk as unknown as DeskResponse;

beforeEach(() => {
  setViewport(393);
  mockApi({ "/api/research/desk": DESK, "/api/macro/observations": MACRO });
});
afterEach(() => setViewport(1280));

describe("Beranda inbox count wording (C8, M7)", () => {
  it("says how many of how many when the inbox list is shorter than the chip count", async () => {
    const total = 974;
    mockApi({
      "/api/research/desk": { ...DESK, filters: DESK.filters.map((f) => (f.group === "semua" ? { ...f, count: total } : f)) },
      "/api/macro/observations": MACRO,
    });
    render(
      <MemoryRouter initialEntries={["/beranda"]}>
        <AppRoutes />
      </MemoryRouter>,
    );
    const region = await screen.findByRole("group", { name: "Kotak masuk" });
    expect(screen.getByRole("button", { name: `Semua ${total}` })).toBeInTheDocument();
    const more = screen.getByRole("button", { name: `Lihat semua (${DESK.inbox.length} terbaru dari ${total})` });
    // At 200% text zoom (197 px) the long label wraps inside the button instead of running past the screen edge (M7).
    expect(more).toHaveClass("max-w-full", "!whitespace-normal", "min-h-11");
    expect(more).not.toHaveClass("h-11");
    fireEvent.click(more);
    expect(within(region).getAllByRole("listitem")).toHaveLength(DESK.inbox.length);
    expect(screen.getByText(`Menampilkan ${DESK.inbox.length} terbaru dari ${total}`)).toBeInTheDocument();
  });
});

describe("landingData", () => {
  it("derives the latest report quarter from the next one, across the year boundary", () => {
    expect(previousQuarter("2026Q3")).toBe("2026Q2");
    expect(previousQuarter("2027Q1")).toBe("2026Q4");
    expect(previousQuarter("nonsense")).toBeNull();
    expect(latestReportQuarter({ next_report: null })).toBeNull();
    expect(latestReportQuarter({ next_report: { label: "2026Q3", deadline: "2026-10-30", overdue: false } })).toBe("2026Q2");
  });

  it("groups held, manual and peer rows in backend order", () => {
    const g = groupCoverage(DESK.coverage as CoverageRow[]);
    expect(g.held.map((r) => r.ticker)).toEqual(["BANK", "COAL", "NONE"]);
    expect(g.manual.map((r) => r.ticker)).toEqual(["BUST", "REST"]);
    expect(g.peers.map((r) => r.ticker)).toEqual(["COPR"]);
  });
});
