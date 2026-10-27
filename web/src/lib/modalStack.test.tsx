import { fireEvent, render, screen } from "@testing-library/react";
import { useRef } from "react";
import { describe, expect, it, vi } from "vitest";
import { pushModal, useModal } from "./modalStack";

function Overlay({ name, active = true, onClose }: { name: string; active?: boolean; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useModal(ref, active, onClose);
  if (!active) return null;
  return (
    <div ref={ref} role="dialog" aria-modal="true" aria-label={name} tabIndex={-1}>
      <button>{`Dalam ${name}`}</button>
    </div>
  );
}

describe("modal stack", () => {
  it("only the top-most dialog gets Escape", () => {
    const a = vi.fn();
    const b = vi.fn();
    render(
      <>
        <Overlay name="A" onClose={a} />
        <Overlay name="B" onClose={b} />
      </>,
    );
    fireEvent.keyDown(window, { key: "Escape" });
    expect(b).toHaveBeenCalledTimes(1);
    expect(a).not.toHaveBeenCalled();
  });

  it("releasing the top dialog hands Escape, Tab and the live area back to the one below", () => {
    const a = vi.fn();
    const b = vi.fn();
    const both = (bOpen: boolean) => (
      <>
        <Overlay name="A" onClose={a} />
        <Overlay name="B" active={bOpen} onClose={b} />
      </>
    );
    const { rerender } = render(both(true));
    const dialogA = screen.getByRole("dialog", { name: "A" });
    expect(dialogA.hasAttribute("inert")).toBe(true);
    rerender(both(false));
    expect(dialogA.hasAttribute("inert")).toBe(false);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(a).toHaveBeenCalledTimes(1);
    expect(b).not.toHaveBeenCalled();
    // Tab at the last control of A wraps inside A.
    const button = screen.getByRole("button", { name: "Dalam A" });
    button.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(button).toHaveFocus();
  });

  it("a dialog that closes under another one does not take focus away from the one on top", () => {
    const page = (aOpen: boolean, bOpen: boolean) => (
      <>
        <button>pembuka</button>
        <Overlay name="A" active={aOpen} onClose={() => {}} />
        <Overlay name="B" active={bOpen} onClose={() => {}} />
      </>
    );
    const { rerender } = render(page(false, false));
    screen.getByRole("button", { name: "pembuka" }).focus();
    // A opens from the button (the button is its opener), then B opens on top of it and takes focus.
    rerender(page(true, false));
    rerender(page(true, true));
    const dialogB = screen.getByRole("dialog", { name: "B" });
    expect(dialogB).toHaveFocus();
    rerender(page(false, true));
    expect(dialogB).toHaveFocus();
  });

  it("a dialog without an inert background on top keeps the background of the sheet below it inert", () => {
    const bg = document.createElement("div");
    const a = document.createElement("div");
    const b = document.createElement("div");
    document.body.append(bg, a, b);
    const releaseA = pushModal({ panel: a, scope: a, onEscape: null, inertBackground: true, lockScroll: false });
    expect(bg).toHaveAttribute("inert");
    expect(b).toHaveAttribute("inert");
    const releaseB = pushModal({ panel: b, scope: b, onEscape: null, inertBackground: false, lockScroll: false });
    expect(bg).toHaveAttribute("inert");
    expect(a).not.toHaveAttribute("inert");
    // Releasing out of order reports who was on top.
    expect(releaseA()).toBe(false);
    expect(bg).not.toHaveAttribute("inert");
    expect(releaseB()).toBe(true);
    expect(releaseB()).toBe(false);
    bg.remove();
    a.remove();
    b.remove();
  });

  it("an element that was already inert stays inert after the dialog releases", () => {
    const page = (active: boolean) => (
      <>
        <div data-testid="biasa">a</div>
        <div data-testid="sudah">b</div>
        <Overlay name="A" active={active} onClose={() => {}} />
      </>
    );
    const { rerender } = render(page(false));
    screen.getByTestId("sudah").setAttribute("inert", "");
    rerender(page(true));
    expect(screen.getByTestId("biasa")).toHaveAttribute("inert");
    rerender(page(false));
    expect(screen.getByTestId("biasa")).not.toHaveAttribute("inert");
    expect(screen.getByTestId("sudah")).toHaveAttribute("inert");
  });
});
