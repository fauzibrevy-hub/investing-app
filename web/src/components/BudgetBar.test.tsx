import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { AccountCard } from "./AccountCard";
import { BudgetBar, budgetStatus, budgetStatusLabel, fillRatio } from "./BudgetBar";

describe("budgetStatus (§5.3)", () => {
  it.each([
    [0, 1000, "safe"],
    [849, 1000, "safe"],
    [850, 1000, "warn"],
    [920, 1000, "warn"],
    [1000, 1000, "warn"],
    [1001, 1000, "over"],
  ] as const)("%d / %d → %s", (spent, limit, expected) => {
    expect(budgetStatus(spent, limit)).toBe(expected);
  });

  it("teks status selalu memakai kata, tidak hanya warna", () => {
    expect(budgetStatusLabel(460_000, 1_000_000)).toBe("sisa Rp540 rb");
    expect(budgetStatusLabel(920_000, 1_000_000)).toBe("92% terpakai");
    expect(budgetStatusLabel(1_035_000, 1_000_000)).toBe("lewat Rp35 rb");
  });

  it("lewat: bar penuh", () => {
    expect(fillRatio(1_200, 1_000)).toBe(1);
    expect(fillRatio(500, 1_000)).toBe(0.5);
  });
});

describe("BudgetBar", () => {
  it("total: label kiri/kanan, penanda hari di 24/31", () => {
    const { container } = render(<BudgetBar variant="total" spent={6_480_000} limit={8_000_000} day={24} daysInMonth={31} />);
    expect(container.textContent).toContain("Rp6,48 jt");
    expect(container.textContent).toContain("dari Rp8 jt");
    // (8.000.000 − 6.480.000) / 8 hari tersisa = 190.000
    expect(container.textContent).toContain("sisa Rp190 rb/hari");
    const marker = container.querySelector<HTMLElement>("[data-marker]")!;
    expect(parseFloat(marker.style.left)).toBeCloseTo((24 / 31) * 100, 3);
    expect(marker.style.height).toBe("16px");
    const fill = container.querySelector<HTMLElement>("[data-fill]")!;
    expect(fill.className).toContain("bg-accent");
    expect(fill.style.width).toBe("81%");
  });

  it("baris warning: warna status-warn-fill dan teks status-warn", () => {
    const { container } = render(<BudgetBar name="Makan & dapur" cat={2} spent={920} limit={1000} />);
    expect(screen.getByText("92% terpakai")).toHaveClass("text-status-warn");
    expect(container.querySelector("[data-fill]")!.className).toContain("bg-status-warn-fill");
  });

  it("loading: track tampil, isi 0", () => {
    const { container } = render(<BudgetBar name="Makan" cat={1} spent={500} limit={1000} status="loading" />);
    expect(container.querySelector("[data-fill]")).toBeNull();
    expect(container.querySelector("[data-skeleton]")).not.toBeNull();
  });

  it("empty & error", () => {
    const { rerender } = render(<BudgetBar name="x" cat={1} status="empty" />);
    expect(screen.getByText("Anggaran belum diatur di Sheets")).toBeInTheDocument();
    rerender(<BudgetBar name="x" cat={1} status="error" />);
    expect(screen.getByText("Gagal memuat anggaran")).toHaveClass("text-status-over");
  });
});

describe("AccountCard", () => {
  it("latar bergilir surface-account-1..3", () => {
    const { container } = render(
      <>
        <AccountCard name="A" balance={1} index={0} />
        <AccountCard name="B" balance={1} index={1} />
        <AccountCard name="C" balance={1} index={2} />
        <AccountCard name="D" balance={1} index={3} />
      </>,
    );
    const cls = [...container.children].map((c) => c.className.match(/bg-surface-account-\d/)![0]);
    expect(cls).toEqual(["bg-surface-account-1", "bg-surface-account-2", "bg-surface-account-3", "bg-surface-account-1"]);
  });

  it("saldo penuh; error menampilkan em dash", () => {
    const { rerender } = render(<AccountCard name="BCA Ayu" balance={4_210_000} />);
    expect(screen.getByText("Rp4.210.000")).toBeInTheDocument();
    rerender(<AccountCard name="BCA Ayu" status="error" />);
    expect(screen.getByText("—")).toBeInTheDocument();
    expect(screen.getByText("BCA Ayu")).toBeInTheDocument();
  });
});
