import { fireEvent, render, screen, within } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { Sheet } from "./Sheet";

describe("Sheet safe area (C3.6)", () => {
  const area = () => screen.getByRole("dialog").querySelector<HTMLElement>(".overflow-y-auto")!;

  it("pads the content area by the bottom safe-area inset when there is no footer", () => {
    render(<Sheet open title="Filter" onClose={() => {}}>isi</Sheet>);
    expect(area().className).toContain("pb-[calc(16px+env(safe-area-inset-bottom))]");
  });

  it("leaves the inset to the footer when there is one", () => {
    render(<Sheet open title="Butir" onClose={() => {}} footer={<button>Simpan</button>}>isi</Sheet>);
    expect(area().className).toContain("pb-4");
    expect(area().className).not.toContain("safe-area-inset-bottom");
  });
});

describe("Sheet height on iOS", () => {
  it("is capped at 90vh with a 90dvh override where the browser knows dvh, so the header never rides above the screen", () => {
    render(<Sheet open title="Filter" onClose={() => {}}>isi</Sheet>);
    const panel = screen.getByRole("dialog");
    expect(panel).toHaveClass("max-h-[90vh]");
    expect(panel).toHaveClass("supports-[height:100dvh]:max-h-[90dvh]");
  });
});

function Harness() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button onClick={() => setOpen(true)}>Buka sheet</button>
      <main data-testid="latar">
        <a href="#latar">Tautan latar</a>
      </main>
      <Sheet open={open} title="Filter" onClose={() => setOpen(false)}>
        <button>Satu</button>
        <button>Dua</button>
      </Sheet>
    </>
  );
}

/** Dua sheet bersusun: yang kedua dibuka dari dalam yang pertama. */
function Stacked() {
  const [first, setFirst] = useState(true);
  const [second, setSecond] = useState(false);
  return (
    <>
      <Sheet open={first} title="Pertama" onClose={() => setFirst(false)}>
        <button onClick={() => setSecond(true)}>Buka kedua</button>
      </Sheet>
      <Sheet open={second} title="Kedua" onClose={() => setSecond(false)}>
        isi kedua
      </Sheet>
    </>
  );
}

describe("Sheet as a modal dialog (M1)", () => {
  it("is a named modal dialog; focus moves in and returns to the opener on Escape", () => {
    render(<Harness />);
    const opener = screen.getByRole("button", { name: "Buka sheet" });
    opener.focus();
    fireEvent.click(opener);
    const dialog = screen.getByRole("dialog", { name: "Filter" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog).toHaveFocus();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(opener).toHaveFocus();
  });

  it("returns focus to the opener even when an autofocused field inside took the focus first", () => {
    function WithField() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button onClick={() => setOpen(true)}>Isi</button>
          <Sheet open={open} title="Isian" onClose={() => setOpen(false)}>
            <input aria-label="Nama" autoFocus />
          </Sheet>
        </>
      );
    }
    render(<WithField />);
    const opener = screen.getByRole("button", { name: "Isi" });
    opener.focus();
    fireEvent.click(opener);
    fireEvent.click(screen.getByRole("button", { name: "Tutup" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(opener).toHaveFocus();
  });

  it("keeps Tab and Shift+Tab inside the sheet, and pulls focus back in when it is outside", () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "Buka sheet" }));
    const dialog = screen.getByRole("dialog", { name: "Filter" });
    const items = within(dialog).getAllByRole("button");
    expect(items.map((b) => b.textContent || b.getAttribute("aria-label"))).toEqual(["Tutup", "Satu", "Dua"]);
    items[items.length - 1].focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(items[0]).toHaveFocus();
    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(items[items.length - 1]).toHaveFocus();
    screen.getByRole("link", { name: "Tautan latar" }).focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(dialog.contains(document.activeElement)).toBe(true);
  });

  it("makes the page behind inert while open, keeps the sheet and its backdrop live, and restores everything on close", () => {
    render(<Harness />);
    const opener = screen.getByRole("button", { name: "Buka sheet" });
    expect(document.querySelector("[inert]")).toBeNull();
    fireEvent.click(opener);
    const dialog = screen.getByRole("dialog", { name: "Filter" });
    expect(screen.getByTestId("latar")).toHaveAttribute("inert");
    expect(opener).toHaveAttribute("inert");
    expect(dialog.closest("[inert]")).toBeNull();
    // The backdrop closes the sheet on tap, so it must not be part of the inert background.
    expect(document.querySelector("[data-sheet-backdrop]")!.closest("[inert]")).toBeNull();
    expect(document.body.style.overflow).toBe("hidden");

    fireEvent.click(document.querySelector("[data-sheet-backdrop]")!);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.querySelector("[inert]")).toBeNull();
    expect(document.body.style.overflow).toBe("");
  });

  it("does not take the focus again when a parent re-renders with a new onClose", () => {
    const view = (
      <Sheet open title="Isian" onClose={() => {}}>
        <input aria-label="Nama" />
      </Sheet>
    );
    const { rerender } = render(view);
    const input = screen.getByLabelText("Nama");
    input.focus();
    rerender(
      <Sheet open title="Isian" onClose={() => {}}>
        <input aria-label="Nama" />
      </Sheet>,
    );
    expect(input).toHaveFocus();
  });

  it("leaves Escape alone when something inside already handled it", () => {
    let closed = 0;
    render(
      <Sheet open title="Isian" onClose={() => (closed += 1)}>
        <input aria-label="Nama" />
      </Sheet>,
    );
    const handled = new KeyboardEvent("keydown", { key: "Escape", cancelable: true, bubbles: true });
    handled.preventDefault();
    document.dispatchEvent(handled);
    expect(closed).toBe(0);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(closed).toBe(1);
  });
});

describe("stacked sheets (N4)", () => {
  it("Escape closes only the top-most sheet, and the one below becomes live again", () => {
    render(<Stacked />);
    const first = screen.getByRole("dialog", { name: "Pertama" });
    expect(first.closest("[inert]")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Buka kedua" }));
    const second = screen.getByRole("dialog", { name: "Kedua" });
    // The second sheet is on top: the first is background now, and Tab is contained by the second.
    expect(first.closest("[inert]")).not.toBeNull();
    expect(second.closest("[inert]")).toBeNull();

    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "Kedua" })).toBeNull();
    expect(screen.getByRole("dialog", { name: "Pertama" })).toBe(first);
    expect(first.closest("[inert]")).toBeNull();
    expect(document.body.style.overflow).toBe("hidden");

    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.querySelector("[inert]")).toBeNull();
    expect(document.body.style.overflow).toBe("");
  });

  it("returns focus from the second sheet to the button in the first that opened it", () => {
    render(<Stacked />);
    const opener = screen.getByRole("button", { name: "Buka kedua" });
    opener.focus();
    fireEvent.click(opener);
    fireEvent.click(within(screen.getByRole("dialog", { name: "Kedua" })).getByRole("button", { name: "Tutup" }));
    expect(opener).toHaveFocus();
  });
});
