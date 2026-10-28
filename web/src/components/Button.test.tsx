import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Button } from "./Button";
import { FormField } from "./FormField";

describe("Button", () => {
  it("disabled: aria-disabled, tetap bisa difokus, klik ditahan", () => {
    const onClick = vi.fn();
    render(
      <Button disabled onClick={onClick}>
        Simpan
      </Button>,
    );
    const btn = screen.getByRole("button", { name: "Simpan" });
    expect(btn).toHaveAttribute("aria-disabled", "true");
    expect(btn).not.toHaveAttribute("disabled");
    fireEvent.click(btn);
    expect(onClick).not.toHaveBeenCalled();
  });

  it("loading: aria-busy, label tetap ada (lebar terkunci), klik ditahan", () => {
    const onClick = vi.fn();
    render(
      <Button loading onClick={onClick}>
        Simpan
      </Button>,
    );
    const btn = screen.getByRole("button", { name: "Simpan" });
    expect(btn).toHaveAttribute("aria-busy", "true");
    fireEvent.click(btn);
    expect(onClick).not.toHaveBeenCalled();
  });

  it("varian icon: hit area 44 px (h-11 w-11) dengan visual 32 px", () => {
    render(<Button variant="icon" aria-label="Muat ulang data" icon={<svg />} />);
    const btn = screen.getByRole("button", { name: "Muat ulang data" });
    expect(btn.className).toContain("h-11 w-11");
    expect(btn.firstElementChild!.className).toContain("h-8 w-8");
  });

  it("chip terpilih memakai aria-pressed", () => {
    render(
      <Button variant="chip" selected>
        Okt 2026
      </Button>,
    );
    expect(screen.getByRole("button", { name: "Okt 2026" })).toHaveAttribute("aria-pressed", "true");
  });
});

describe("FormField", () => {
  it("label terhubung ke input", () => {
    render(<FormField label="Merchant" hint="Opsional" />);
    const input = screen.getByLabelText("Merchant");
    expect(input.tagName).toBe("INPUT");
    expect(input.getAttribute("aria-describedby")).toContain("hint");
  });

  it("error: aria-invalid + aria-describedby menunjuk pesan", () => {
    render(<FormField label="Nominal" error="Nominal harus lebih dari 0" />);
    const input = screen.getByLabelText("Nominal");
    expect(input).toHaveAttribute("aria-invalid", "true");
    const describedBy = input.getAttribute("aria-describedby")!;
    expect(document.getElementById(describedBy.split(" ")[0])!.textContent).toBe("Nominal harus lebih dari 0");
  });

  it("loading: input disabled", () => {
    render(<FormField label="Akun" loading />);
    expect(screen.getByLabelText("Akun")).toBeDisabled();
  });

  it("picker kosong menampilkan pesan", () => {
    render(
      <FormField label="Kategori" emptyMessage="Belum ada kategori di Sheets">
        {(a11y) => <div {...a11y}>picker</div>}
      </FormField>,
    );
    expect(screen.getByText("Belum ada kategori di Sheets")).toBeInTheDocument();
    expect(screen.queryByText("picker")).toBeNull();
  });
});
