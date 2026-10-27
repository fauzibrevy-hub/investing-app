import { describe, expect, it } from "vitest";
import golden from "../../../../tests/fixtures/research/canon/prices-COAL-1y.json";
import {
  dateOfDay,
  dayIndex,
  eventMarkers,
  indexOnPriceAxis,
  levelLines,
  linePath,
  nearestIndex,
  niceAxis,
  rebaseTo100,
  tableRows,
  timeScale,
  timeTicks,
  valueAsOf,
  valueScale,
  type DatedPoint,
} from "./driverGeometry";

const price = golden.price as unknown as DatedPoint[];
const ma200 = golden.ma200 as unknown as DatedPoint[];

describe("dayIndex / dateOfDay", () => {
  it.each([
    ["1970-01-01", 0],
    ["1970-01-02", 1],
    ["2000-03-01", 11017],
    ["2026-10-27", 20753],
  ])("%s is day %i", (date, day) => {
    expect(dayIndex(date)).toBe(day);
    expect(dateOfDay(day)).toBe(date);
  });

  it("reads only the calendar day of a timestamp and refuses text", () => {
    expect(dayIndex("2026-10-27T23:00:00+07:00")).toBe(20753);
    expect(Number.isNaN(dayIndex("kemarin"))).toBe(true);
  });

  it("refuses a calendar day that does not exist instead of rolling it over", () => {
    expect(Number.isNaN(dayIndex("2026-13-45"))).toBe(true);
    expect(Number.isNaN(dayIndex("2026-02-30"))).toBe(true);
    expect(dayIndex("2024-02-29")).toBe(19782);
  });

  it("dateOfDay of a non-finite day is empty, not a thrown RangeError", () => {
    expect(dateOfDay(Number.NaN)).toBe("");
  });
});

describe("timeScale", () => {
  const s = timeScale("2025-10-28", "2026-10-27", 48, 648);

  it("maps the range start and end to the plot edges", () => {
    expect(s.x("2025-10-28")).toBeCloseTo(48);
    expect(s.x("2026-10-27")).toBeCloseTo(648);
  });

  it("is linear in calendar days (364 days over 600 px)", () => {
    // 2026-04-28 is 182 days after the start: half way.
    expect(s.x("2026-04-28")).toBeCloseTo(348);
  });

  it("inverts a pixel back to the day under it", () => {
    expect(dateOfDay(Math.round(s.dayAt(348)))).toBe("2026-04-28");
    expect(s.dayAt(48)).toBeCloseTo(dayIndex("2025-10-28"));
  });

  it("survives a single-day range without dividing by zero", () => {
    const one = timeScale("2026-10-27", "2026-10-27", 0, 100);
    expect(Number.isFinite(one.x("2026-10-27"))).toBe(true);
  });
});

describe("niceAxis", () => {
  it.each<[string, number[], { min: number; max: number; step: number; ticks: number[] }]>([
    ["golden COAL closes (1020 to 1310)", [1020, 1310], { min: 1000, max: 1400, step: 100, ticks: [1000, 1100, 1200, 1300, 1400] }],
    ["a driver in USD/t", [96.4, 141.2], { min: 80, max: 160, step: 20, ticks: [80, 100, 120, 140, 160] }],
    ["values that cross zero", [-3.2, 7.9], { min: -5, max: 10, step: 5, ticks: [-5, 0, 5, 10] }],
    ["sub-unit ratios", [0.12, 0.47], { min: 0.1, max: 0.5, step: 0.1, ticks: [0.1, 0.2, 0.3, 0.4, 0.5] }],
  ])("%s", (_name, values, expected) => {
    const axis = niceAxis(values)!;
    expect(axis.min).toBeCloseTo(expected.min);
    expect(axis.max).toBeCloseTo(expected.max);
    expect(axis.step).toBeCloseTo(expected.step);
    expect(axis.ticks).toHaveLength(expected.ticks.length);
    axis.ticks.forEach((t, i) => expect(t).toBeCloseTo(expected.ticks[i]));
  });

  it("covers every value of the real golden price series", () => {
    const axis = niceAxis(price.map((p) => p[1]))!;
    expect(axis.min).toBeLessThanOrEqual(Math.min(...price.map((p) => p[1])));
    expect(axis.max).toBeGreaterThanOrEqual(Math.max(...price.map((p) => p[1])));
    expect(axis.ticks.length).toBeLessThanOrEqual(6);
  });

  it("pads a flat series around its value", () => {
    const axis = niceAxis([1290, 1290])!;
    expect(axis.min).toBeLessThan(1290);
    expect(axis.max).toBeGreaterThan(1290);
  });

  it("ignores non-finite values and gives null without any", () => {
    expect(niceAxis([Number.NaN, Infinity])).toBeNull();
    expect(niceAxis([])).toBeNull();
    expect(niceAxis([Number.NaN, 5, 15])!.min).toBeCloseTo(5);
  });
});

describe("valueScale", () => {
  it("puts the axis minimum at the bottom and the maximum at the top", () => {
    const y = valueScale({ min: 1000, max: 1400 }, 0, 300);
    expect(y(1000)).toBeCloseTo(300);
    expect(y(1400)).toBeCloseTo(0);
    expect(y(1290)).toBeCloseTo(82.5);
  });
});

describe("linePath", () => {
  const x = (d: string) => dayIndex(d) - dayIndex("2026-01-01");
  const y = (v: number) => 100 - v;

  it("draws M then L through every point", () => {
    expect(
      linePath(
        [
          ["2026-01-01", 10],
          ["2026-01-02", 20.125],
          ["2026-01-04", 15],
        ],
        x,
        y,
      ),
    ).toBe("M 0 90 L 1 79.88 L 3 85");
  });

  it("breaks the line at a non-finite value instead of joining across it", () => {
    expect(
      linePath(
        [
          ["2026-01-01", 10],
          ["2026-01-02", Number.NaN],
          ["2026-01-03", 30],
        ],
        x,
        y,
      ),
    ).toBe("M 0 90 M 2 70");
  });

  it("is empty for no points", () => {
    expect(linePath([], x, y)).toBe("");
  });
});

describe("rebaseTo100 and indexOnPriceAxis", () => {
  const series: DatedPoint[] = [
    ["2026-01-01", 50],
    ["2026-01-02", 55],
    ["2026-01-05", 45],
  ];

  it("rebases the first value to 100", () => {
    const r = rebaseTo100(series);
    expect(r[0][1]).toBeCloseTo(100);
    expect(r[1][1]).toBeCloseTo(110);
    expect(r[2][1]).toBeCloseTo(90);
    expect(r.map((p) => p[0])).toEqual(series.map((p) => p[0]));
  });

  it("leaves a series that already starts at 100 unchanged (the payload's index_relative)", () => {
    const already: DatedPoint[] = [
      ["2026-01-01", 100],
      ["2026-01-02", 103.5],
    ];
    expect(rebaseTo100(already)[1][1]).toBeCloseTo(103.5);
  });

  it("anchors 100 at the first close so the index line starts where the price line starts", () => {
    const mapped = indexOnPriceAxis(series, [["2026-01-01", 1020]]);
    expect(mapped[0][1]).toBeCloseTo(1020);
    expect(mapped[1][1]).toBeCloseTo(1122);
    expect(mapped[2][1]).toBeCloseTo(918);
  });

  it("an index that starts after the price starts at the close of its own first day", () => {
    const price: DatedPoint[] = [
      ["2026-01-01", 1000],
      ["2026-01-02", 1100],
      ["2026-01-05", 1050],
    ];
    const index: DatedPoint[] = [
      ["2026-01-02", 100],
      ["2026-01-05", 101],
    ];
    const mapped = indexOnPriceAxis(index, price);
    expect(mapped[0]).toEqual(["2026-01-02", 1100]);
    expect(mapped[1][1]).toBeCloseTo(1111);
  });

  it("gives nothing when the series has no usable base or no close on its first day", () => {
    expect(rebaseTo100([["2026-01-01", 0]])).toEqual([]);
    expect(rebaseTo100([["2026-01-01", -50], ["2026-01-02", -25]])).toEqual([]);
    expect(indexOnPriceAxis([], [["2026-01-01", 1020]])).toEqual([]);
    expect(indexOnPriceAxis(series, [["2026-02-01", 1020]])).toEqual([]);
  });

  it("a null inside the series stays a gap, never a drop to zero", () => {
    const withGap = [["2026-01-01", 50], ["2026-01-02", null], ["2026-01-05", 55]] as unknown as DatedPoint[];
    const r = rebaseTo100(withGap);
    expect(Number.isNaN(r[1][1])).toBe(true);
    expect(r[2][1]).toBeCloseTo(110);
  });
});

describe("levelLines", () => {
  it("draws each golden journal level as a horizontal line at its split-adjusted value", () => {
    const y = valueScale({ min: 1000, max: 1400 }, 0, 300);
    const lines = levelLines(golden.levels, y, 48, 648);
    expect(lines).toHaveLength(2);
    expect(lines[0]).toMatchObject({ id: "j-old", x1: 48, x2: 648 });
    expect(lines[0].y).toBeCloseTo(225);
    expect(lines[1].y).toBeCloseTo(75);
  });
});

describe("eventMarkers", () => {
  const s = timeScale("2025-10-28", "2026-10-27", 48, 648);

  it("places dividends and splits on the x axis and drops events outside the range", () => {
    const markers = eventMarkers(
      [
        { date: "2026-04-28", kind: "dividend", amount: 45 },
        { date: "2026-06-01", kind: "split", ratio_num: 2, ratio_den: 1 },
        { date: "2024-01-01", kind: "dividend", amount: 30 },
      ],
      s,
    );
    expect(markers).toHaveLength(2);
    expect(markers[0]).toMatchObject({ kind: "dividend", date: "2026-04-28" });
    expect(markers[0].x).toBeCloseTo(348);
    expect(markers[1]).toMatchObject({ kind: "split", date: "2026-06-01" });
    expect(markers[1].x).toBeCloseTo(48 + (216 / 364) * 600);
  });
});

describe("timeTicks", () => {
  it("1 year: every second month, at most six, labelled month and year", () => {
    const ticks = timeTicks(timeScale("2025-10-28", "2026-10-27", 48, 648));
    expect(ticks.map((t) => t.label)).toEqual(["Nov 25", "Jan 26", "Mar 26", "Mei 26", "Jul 26", "Sep 26"]);
    expect(ticks[1].x).toBeCloseTo(48 + ((dayIndex("2026-01-01") - dayIndex("2025-10-28")) / 364) * 600);
  });

  it("3 years: every sixth month", () => {
    const ticks = timeTicks(timeScale("2023-10-28", "2026-10-27", 0, 600));
    expect(ticks.map((t) => t.label)).toEqual(["Jan 24", "Jul 24", "Jan 25", "Jul 25", "Jan 26", "Jul 26"]);
  });

  it("a range with a date that is not a date has no ticks and does not hang", () => {
    expect(timeTicks(timeScale("bukan tanggal", "2026-10-27", 0, 600))).toEqual([]);
    expect(timeTicks(timeScale("2026-01-01", "2026-13-45", 0, 600))).toEqual([]);
  });

  it("10 years: every second year, labelled by year", () => {
    const ticks = timeTicks(timeScale("2016-10-28", "2026-10-27", 0, 600));
    expect(ticks.map((t) => t.label)).toEqual(["2018", "2020", "2022", "2024", "2026"]);
  });
});

describe("nearestIndex and valueAsOf", () => {
  it("finds the golden close nearest to a day, and clamps outside the series", () => {
    const i = nearestIndex(price, dayIndex("2026-10-27"));
    expect(price[i]).toEqual(["2026-10-27", 1290]);
    expect(nearestIndex(price, dayIndex("2020-01-01"))).toBe(0);
    expect(nearestIndex(price, dayIndex("2030-01-01"))).toBe(price.length - 1);
    expect(nearestIndex([], 0)).toBe(-1);
  });

  it("snaps between two points to the closer one", () => {
    const pts: DatedPoint[] = [
      ["2026-01-01", 1],
      ["2026-01-05", 2],
    ];
    expect(nearestIndex(pts, dayIndex("2026-01-02"))).toBe(0);
    expect(nearestIndex(pts, dayIndex("2026-01-04"))).toBe(1);
  });

  it("reads a monthly driver as of a day: the newest observation on or before it", () => {
    const monthly: DatedPoint[] = [
      ["2026-08-01", 118.2],
      ["2026-09-01", 121.4],
      ["2026-10-01", 125.9],
    ];
    expect(valueAsOf(monthly, dayIndex("2026-09-17"))).toEqual(["2026-09-01", 121.4]);
    expect(valueAsOf(monthly, dayIndex("2026-10-01"))).toEqual(["2026-10-01", 125.9]);
    expect(valueAsOf(monthly, dayIndex("2026-07-31"))).toBeNull();
  });
});

describe("tableRows", () => {
  it("has one row per plotted close with MA200 on the same day and the driver as of the day", () => {
    const driver: DatedPoint[] = [["2026-10-01", 125.9]];
    const rows = tableRows({ price, ma200, driver, index: [] });
    expect(rows).toHaveLength(price.length);
    const last = rows[rows.length - 1];
    expect(last.date).toBe("2026-10-27");
    expect(last.price).toBeCloseTo(1290);
    expect(last.ma200).toBeCloseTo(1179.715);
    expect(last.driver).toBeCloseTo(125.9);
    expect(last.index).toBeNull();
    // Before the first MA200 point and the first driver observation both columns are empty.
    expect(rows[0].ma200).toBeNull();
    expect(rows[0].driver).toBeNull();
  });
});
