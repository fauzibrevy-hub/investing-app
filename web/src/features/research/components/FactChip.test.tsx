import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FactChip } from "./FactChip";
import { FactDrawerContext } from "./factDrawerContext";
import { TARGET } from "./fixtures";

const base = { label: "Laba bersih", period: "Q2 26", target: TARGET };

describe("FactChip", () => {
  it("default: a button with aria-haspopup=dialog and the §8 label with the visible number in its name (WCAG 2.5.3)", () => {
    render(<FactChip {...base} value={1234} text="1.234" />);
    const chip = screen.getByRole("button", { name: /^Asal angka Laba bersih Q2 26: / });
    expect(chip.tagName).toBe("BUTTON");
    expect(chip).toHaveAttribute("aria-haspopup", "dialog");
    expect(chip).toHaveAccessibleName("Asal angka Laba bersih Q2 26: 1.234");
    expect(chip).toHaveClass("num");
    expect(chip.parentElement).toHaveClass("justify-end");
  });

  it("formats with the shared formatter when no text is given, with U+2212 negatives and no units", () => {
    render(<FactChip {...base} value={-2500} />);
    const chip = screen.getByRole("button");
    expect(chip).toHaveTextContent("−2.500");
    expect(chip.textContent).not.toMatch(/Rp|%|×/);
  });

  it("puts the §3.2 badges after the number", () => {
    render(<FactChip {...base} value={5} flags={["derived_fy", "restated", "instant"]} badges={["manual"]} />);
    const badges = Array.from(document.querySelectorAll("[data-provenance]")).map((b) => b.getAttribute("data-provenance"));
    expect(badges).toEqual(["turunan", "disajikan", "manual"]);
    const chip = screen.getByRole("button");
    expect(chip.compareDocumentPosition(document.querySelector("[data-provenance]")!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("click opens the drawer for its target through the shared provider", () => {
    const open = vi.fn();
    render(
      <FactDrawerContext.Provider value={{ open, close: vi.fn(), isOpen: false }}>
        <FactChip {...base} value={1} />
      </FactDrawerContext.Provider>,
    );
    fireEvent.click(screen.getByRole("button"));
    expect(open).toHaveBeenCalledWith(TARGET, screen.getByRole("button"));
  });

  it("onOpen replaces the shared drawer", () => {
    const onOpen = vi.fn();
    const open = vi.fn();
    render(
      <FactDrawerContext.Provider value={{ open, close: vi.fn(), isOpen: false }}>
        <FactChip {...base} value={1} onOpen={onOpen} />
      </FactDrawerContext.Provider>,
    );
    fireEvent.click(screen.getByRole("button"));
    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(open).not.toHaveBeenCalled();
  });

  it("loading: a 56 × 14 skeleton and no button", () => {
    const { container } = render(<FactChip {...base} value={null} status="loading" />);
    expect(container.querySelector("[data-skeleton]")).toHaveStyle({ width: "56px", height: "14px" });
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("empty: the words Belum ada in text-3 with the tooltip 'Tidak ada di laporan' and a screen-reader reason, not a dash", () => {
    render(<FactChip {...base} value={null} target={null} flags={["gap"]} />);
    const gap = document.querySelector("[data-gap]")!;
    expect(gap).toHaveAttribute("title", "Tidak ada di laporan");
    expect(gap).toHaveClass("text-text-3");
    expect(gap).toHaveTextContent("Belum ada");
    expect(gap).not.toHaveTextContent("—");
    expect(screen.getByText("Laba bersih Q2 26: Tidak ada di laporan")).toHaveClass("sr-only");
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("empty in an editable context: the gap is the action (Isi), with a 44 px hit area", () => {
    const onClick = vi.fn();
    render(<FactChip {...base} value={null} target={null} gapAction={{ label: "Isi", onClick }} />);
    const action = screen.getByRole("button", { name: "Isi Laba bersih Q2 26" });
    expect(action).toHaveClass("min-h-11", "min-w-11");
    fireEvent.click(action);
    expect(onClick).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("—")).toBeNull();
  });

  it("a number without a target is a gap, never an untraceable figure", () => {
    render(<FactChip label="Laba bersih" period="Q2 26" value={99} target={null} />);
    expect(screen.queryByText("99")).toBeNull();
    expect(document.querySelector("[data-gap]")).toHaveAttribute("title", "Asal angka tidak tersedia");
  });

  it("error: a ! icon named for the failure with the reason as a tooltip", () => {
    render(<FactChip {...base} value={1} status="error" errorMessage="Gagal memuat" />);
    const err = screen.getByRole("img", { name: "Laba bersih Q2 26: gagal memuat" });
    expect(err).toHaveAttribute("title", "Gagal memuat");
    expect(screen.queryByRole("button")).toBeNull();
  });
});

describe("FactChip edge cases", () => {
  it("a real zero is a figure, not a gap", () => {
    render(<FactChip {...base} value={0} />);
    expect(screen.getByRole("button", { name: /^Asal angka Laba bersih Q2 26: / })).toHaveTextContent("0");
    expect(document.querySelector("[data-gap]")).toBeNull();
  });

  it("status empty wins over a value: the chip shows the gap, not the number", () => {
    render(<FactChip {...base} value={5} text="5" status="empty" />);
    expect(screen.queryByText("5")).toBeNull();
    expect(document.querySelector("[data-gap]")).toHaveAttribute("title", "Tidak ada di laporan");
  });

  it("status empty in an editable context offers the action", () => {
    render(<FactChip {...base} value={5} status="empty" gapAction={{ label: "Isi", onClick: () => {} }} />);
    expect(screen.getByRole("button", { name: "Isi Laba bersih Q2 26" })).toBeInTheDocument();
  });

  it("loading never leaks the value", () => {
    render(<FactChip {...base} value={1234} text="1.234" status="loading" />);
    expect(screen.queryByText("1.234")).toBeNull();
  });

  it("a badge named by both a flag and the badges prop appears once", () => {
    render(<FactChip {...base} value={5} flags={["derived_fy"]} badges={["turunan", "manual"]} />);
    expect(Array.from(document.querySelectorAll("[data-provenance]")).map((b) => b.getAttribute("data-provenance"))).toEqual(["turunan", "manual"]);
  });

  it("without a drawer provider and without onOpen, a click does nothing and throws nothing", () => {
    render(<FactChip {...base} value={5} />);
    expect(() => fireEvent.click(screen.getByRole("button"))).not.toThrow();
  });

  it("is a native button that is a Tab stop by default and honours a roving tabIndex; the hit area is 44 px on the button itself", () => {
    const { rerender } = render(<FactChip {...base} value={5} />);
    const button = screen.getByRole("button");
    expect(button).toHaveAttribute("type", "button");
    expect(button).not.toHaveAttribute("tabindex");
    expect(button).toHaveClass("min-h-11", "min-w-11");
    rerender(<FactChip {...base} value={5} tabIndex={-1} />);
    expect(screen.getByRole("button")).toHaveAttribute("tabindex", "-1");
  });

  it("the accessible name ends with the visible number, so label-in-name holds", () => {
    render(<FactChip {...base} value={-2500} />);
    expect(screen.getByRole("button")).toHaveAccessibleName("Asal angka Laba bersih Q2 26: −2.500");
  });
});
