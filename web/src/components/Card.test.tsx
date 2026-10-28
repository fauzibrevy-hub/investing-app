import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Card } from "./Card";
import { StatRow } from "./StatRow";

describe("Card", () => {
  it("loading: isi tetap ada tapi tersembunyi (tinggi sama) + aria-busy", () => {
    const { container } = render(
      <Card status="loading">
        <p>isi</p>
      </Card>,
    );
    expect(container.firstElementChild).toHaveAttribute("aria-busy", "true");
    expect(screen.getByText("isi").parentElement).toHaveClass("invisible");
    expect(container.querySelector("[data-skeleton]")).not.toBeNull();
  });

  it("empty: pesan default", () => {
    render(<Card status="empty" />);
    expect(screen.getByText("Belum ada data bulan ini")).toBeInTheDocument();
  });

  it("error: pesan + Coba lagi memanggil onRetry", () => {
    const onRetry = vi.fn();
    render(<Card status="error" onRetry={onRetry} />);
    expect(screen.getByRole("alert")).toHaveTextContent("Gagal memuat");
    fireEvent.click(screen.getByRole("button", { name: "Coba lagi" }));
    expect(onRetry).toHaveBeenCalled();
  });

  it("plain memakai outline, tint tidak", () => {
    const { container, rerender } = render(<Card tone="plain" />);
    expect(container.firstElementChild!.className).toContain("border-border-hairline");
    rerender(<Card tone="summary" />);
    expect(container.firstElementChild!.className).not.toContain("border");
  });
});

describe("StatRow", () => {
  it("default: label + nilai tabular", () => {
    render(<StatRow label="Hari ini" value="Rp312 rb" />);
    expect(screen.getByText("Rp312 rb")).toHaveClass("num", "text-stat");
  });

  it("empty: em dash text-3; onTint: text-2", () => {
    const { rerender } = render(<StatRow label="Kas" status="empty" />);
    expect(screen.getByText("—")).toHaveClass("text-text-3");
    rerender(<StatRow label="Kas" status="empty" onTint />);
    expect(screen.getByText("—")).toHaveClass("text-text-2");
  });

  it("error: em dash dengan title penyebab", () => {
    render(<StatRow label="Kas" status="error" errorReason="Sheets timeout" />);
    expect(screen.getByText("—")).toHaveAttribute("title", "Sheets timeout");
  });
});
