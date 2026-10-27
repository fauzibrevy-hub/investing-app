import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ErrorBoundary } from "./ErrorBoundary";

function Boom(): JSX.Element {
  throw new Error("boom");
}

describe("ErrorBoundary", () => {
  it("screen yang melempar menampilkan pesan + Muat ulang, nav tetap tampil", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <div>
        <nav aria-label="Navigasi utama">nav</nav>
        <ErrorBoundary>
          <Boom />
        </ErrorBoundary>
      </div>,
    );
    expect(screen.getByRole("navigation")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain("gagal dimuat");
    expect(screen.getByRole("button", { name: "Muat ulang" })).toBeTruthy();
  });

  it("pulih saat resetKey berubah", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    let boom = true;
    const Maybe = () => (boom ? <Boom /> : <p>ok</p>);
    const { rerender } = render(<ErrorBoundary resetKey="a"><Maybe /></ErrorBoundary>);
    boom = false;
    rerender(<ErrorBoundary resetKey="b"><Maybe /></ErrorBoundary>);
    expect(screen.getByText("ok")).toBeTruthy();
  });
});
