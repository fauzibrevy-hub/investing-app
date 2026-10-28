import { fireEvent, render, screen } from "@testing-library/react";
import { ShoppingBasket } from "lucide-react";
import { describe, expect, it, vi } from "vitest";
import { MINUS } from "../lib/money";
import { HideAmountsProvider } from "../lib/privacy";
import { SectionHeader } from "./SectionHeader";
import { ListState, TransactionRow } from "./TransactionRow";

describe("TransactionRow", () => {
  it("transaksi keluar: nominal penuh tanpa minus, warna money-out", () => {
    render(<TransactionRow title="Sayur & lauk pasar" subtitle="Makan & dapur" icon={ShoppingBasket} cat={2} amount={74_000} direction="out" />);
    const amount = screen.getByText("Rp74.000");
    expect(amount).toHaveClass("text-money-out", "num");
    expect(amount.textContent).not.toContain(MINUS);
  });

  it("pemasukan: money-in", () => {
    render(<TransactionRow title="Gaji" amount={6_300_000} direction="in" />);
    expect(screen.getByText("Rp6.300.000")).toHaveClass("text-money-in");
  });

  it("kategori: nominal + persen", () => {
    render(<TransactionRow variant="category" title="Kebutuhan anak" subtitle="14 catatan" amount={1_840_000} percent={28} />);
    expect(screen.getByText("Rp1.840.000")).toBeInTheDocument();
    expect(screen.getByText("28%")).toBeInTheDocument();
  });

  it("posisi: P/L bertanda dan berwarna", () => {
    render(<TransactionRow variant="position" title="BBCA" subtitle="200 lot · avg 9.150" value={190_000_000} plPercent={-2.3} />);
    expect(screen.getByText(`${MINUS}2,3%`)).toHaveClass("text-status-over");
  });

  it("dapat diketuk: seluruh baris satu tombol", () => {
    const onClick = vi.fn();
    render(<TransactionRow title="Bensin motor" amount={40_000} direction="out" onClick={onClick} />);
    fireEvent.click(screen.getByRole("button"));
    expect(onClick).toHaveBeenCalled();
  });

  it("mode sembunyi nominal", () => {
    render(
      <HideAmountsProvider initial>
        <TransactionRow title="Bensin motor" amount={40_000} direction="out" />
      </HideAmountsProvider>,
    );
    expect(screen.getByText("Rp•••")).toHaveAttribute("aria-label", "nominal disembunyikan");
  });

  it("ListState empty", () => {
    render(<ListState status="empty" />);
    expect(screen.getByText("Belum ada transaksi. Ketuk + untuk mencatat.")).toBeInTheDocument();
  });
});

describe("SectionHeader", () => {
  it("aksi tampil saat ready dan tersembunyi saat loading", () => {
    const { rerender } = render(<SectionHeader title="Terakhir dicatat" action={{ label: "Semua", onClick: () => {} }} />);
    expect(screen.getByRole("heading", { name: "Terakhir dicatat" }).tagName).toBe("H2");
    expect(screen.getByRole("button", { name: "Semua" })).toHaveClass("min-h-11");
    rerender(<SectionHeader title="Terakhir dicatat" status="loading" action={{ label: "Semua", onClick: () => {} }} />);
    expect(screen.queryByRole("button", { name: "Semua" })).toBeNull();
  });
});
