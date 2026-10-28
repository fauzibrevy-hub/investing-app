import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { CoverageMeter } from "./CoverageMeter";

describe("CoverageMeter", () => {
  it("default: a 64 × 6 bar filled with accent and the caption 9/10", () => {
    render(<CoverageMeter populated={9} total={10} label="Cakupan XBRL" />);
    const meter = screen.getByRole("meter", { name: "Cakupan XBRL" });
    expect(meter).toHaveClass("w-16", "h-1.5");
    expect(meter).toHaveAttribute("aria-valuenow", "9");
    expect(meter).toHaveAttribute("aria-valuemax", "10");
    expect(meter).toHaveAttribute("aria-valuetext", "9 dari 10");
    expect(meter.firstElementChild).toHaveClass("bg-accent");
    expect(meter.firstElementChild).toHaveStyle({ width: "90%" });
    expect(screen.getByText("9/10")).toHaveClass("text-text-2");
  });

  it("below 50 % only the caption turns status-warn; the bar colour is unchanged", () => {
    render(<CoverageMeter populated={4} total={10} label="Cakupan KPI" />);
    expect(screen.getByText("4/10")).toHaveClass("text-status-warn");
    expect(screen.getByRole("meter").firstElementChild).toHaveClass("bg-accent");
    expect(screen.getByRole("meter").firstElementChild?.className).not.toMatch(/warn/);
  });

  it("exactly 50 % is not low", () => {
    render(<CoverageMeter populated={5} total={10} label="Cakupan" />);
    expect(screen.getByText("5/10")).not.toHaveClass("text-status-warn");
  });

  it("loading: a skeleton with aria-busy", () => {
    const { container } = render(<CoverageMeter populated={0} total={0} label="Cakupan" status="loading" />);
    expect(container.firstChild).toHaveAttribute("aria-busy", "true");
    expect(container.querySelectorAll("[data-skeleton]").length).toBeGreaterThan(0);
    expect(screen.queryByRole("meter")).toBeNull();
  });

  it("empty: 0/0 with 'tanpa paket' and no meter role", () => {
    render(<CoverageMeter populated={0} total={0} label="Cakupan XBRL" />);
    expect(screen.getByText("0/0 tanpa paket")).toBeInTheDocument();
    expect(screen.queryByRole("meter")).toBeNull();
  });

  it("error: an icon named for the failure, with the reason as a tooltip", () => {
    render(<CoverageMeter populated={1} total={2} label="Cakupan XBRL" status="error" errorMessage="timeout" />);
    const err = screen.getByRole("img", { name: "Cakupan XBRL: gagal memuat" });
    expect(err).toHaveAttribute("title", "timeout");
  });

  it("clamps an over-full count", () => {
    render(<CoverageMeter populated={12} total={10} label="Cakupan" />);
    expect(screen.getByRole("meter").firstElementChild).toHaveStyle({ width: "100%" });
  });
});
