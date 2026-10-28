import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SourcedValue } from "./SourcedValue";

describe("SourcedValue", () => {
  it("is a button that discloses its source, date and derivation, and Esc closes it", () => {
    render(<SourcedValue text="2,9" label="Volatilitas 1 tahun" source="GET /AAAA/prices" date="2026-10-27" detail={["Simpangan baku log-return harian"]} />);
    const button = screen.getByRole("button", { name: "Asal angka Volatilitas 1 tahun: 2,9" });
    const panel = document.getElementById(button.getAttribute("aria-controls")!)!;
    expect(panel).not.toBeVisible();
    fireEvent.click(button);
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(panel).toBeVisible();
    expect(panel).toHaveTextContent("GET /AAAA/prices");
    expect(panel).toHaveTextContent("per 27 Okt 2026");
    expect(panel).toHaveTextContent("Simpangan baku log-return harian");
    fireEvent.keyDown(button, { key: "Escape" });
    expect(panel).not.toBeVisible();
    expect(button).toHaveFocus();
  });

  it("a gap is a dash with its reason, never a bare dash", () => {
    render(<SourcedValue text={null} label="P/E TTM" source="x" gapReason="Belum cukup data" />);
    expect(screen.getByText("P/E TTM: Belum cukup data")).toBeInTheDocument();
    expect(screen.getByTitle("Belum cukup data")).toBeInTheDocument();
  });

  it("a gap in an editable context is its action", () => {
    const onClick = vi.fn();
    render(<SourcedValue text={null} label="Jumlah saham" source="x" gapAction={{ label: "Isi", onClick }} />);
    fireEvent.click(screen.getByRole("button", { name: "Isi Jumlah saham" }));
    expect(onClick).toHaveBeenCalledOnce();
  });
});
