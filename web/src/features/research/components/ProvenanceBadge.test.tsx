import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ProvenanceBadge, badgesForFlags } from "./ProvenanceBadge";

describe("ProvenanceBadge", () => {
  it("default: an 11 px uppercase tag in text-2 on surface-sunken, never coloured", () => {
    render(<ProvenanceBadge kind="xbrl" />);
    const badge = screen.getByText("XBRL");
    expect(badge).toHaveClass("uppercase", "text-table-head", "text-text-2", "bg-surface-sunken", "rounded-sm");
    expect(badge.className).not.toMatch(/status-|accent|cat-/);
  });

  it("uses the design.md §3.2 and §3.6 words and a tooltip", () => {
    render(
      <>
        <ProvenanceBadge kind="turunan" />
        <ProvenanceBadge kind="pembanding" />
        <ProvenanceBadge kind="disajikan" />
        <ProvenanceBadge kind="manual" title="Paparan 1 Agu 2026" />
      </>,
    );
    expect(screen.getByText("turunan FY")).toBeInTheDocument();
    expect(screen.getByText("dari pembanding")).toBeInTheDocument();
    expect(screen.getByText("disajikan kembali")).toBeInTheDocument();
    expect(screen.getByText("manual")).toHaveAttribute("title", "Paparan 1 Agu 2026");
    expect(screen.getByText("turunan FY")).toHaveAttribute("title");
  });

  it("pdf says PDF, never XBRL, and its tooltip says why; the page rides in the text", () => {
    render(
      <>
        <ProvenanceBadge kind="pdf" />
        <ProvenanceBadge kind="pdf" text="PDF · hal. 6" />
      </>,
    );
    const plain = screen.getByText("PDF");
    expect(plain).toHaveAttribute("data-provenance", "pdf");
    expect(plain.getAttribute("title")).toMatch(/bukan XBRL/);
    expect(plain.className).not.toMatch(/status-|accent|cat-/);
    expect(screen.getByText("PDF · hal. 6")).toHaveAttribute("data-provenance", "pdf");
    expect(screen.queryByText("XBRL")).toBeNull();
  });

  it("usulan marks a value read from a document that the owner has not accepted yet", () => {
    render(<ProvenanceBadge kind="usulan" />);
    expect(screen.getByText("usulan")).toHaveAttribute("data-provenance", "usulan");
    expect(screen.getByText("usulan").getAttribute("title")).toMatch(/belum menjadi nilai KPI/);
  });

  it("text overrides the label (the Q4 header badge says only 'turunan')", () => {
    render(<ProvenanceBadge kind="turunan" text="turunan" />);
    expect(screen.getByText("turunan")).toHaveAttribute("data-provenance", "turunan");
  });

  it("has no loading, empty or error variant: it renders only when its source exists", () => {
    const { container } = render(<ProvenanceBadge kind="manual" />);
    expect(container.querySelectorAll("[data-skeleton]")).toHaveLength(0);
    expect(container.querySelector("[role=alert]")).toBeNull();
    // Caller shows no badge when there is nothing to attribute.
    expect(badgesForFlags([])).toEqual([]);
    expect(badgesForFlags(undefined)).toEqual([]);
  });

  it("maps cell flags to badges and ignores flags without one", () => {
    expect(badgesForFlags(["derived_fy", "from_comparative", "restated"])).toEqual(["turunan", "pembanding", "disajikan"]);
    expect(badgesForFlags(["gap", "instant", "reconcile_failed"])).toEqual([]);
  });

  it("surfaces basis_break, sign_conflict and per_share_approx with their own words", () => {
    expect(badgesForFlags(["basis_break", "sign_conflict", "per_share_approx"])).toEqual(["basis", "tanda", "per_saham"]);
    render(
      <>
        <ProvenanceBadge kind="basis" />
        <ProvenanceBadge kind="tanda" />
        <ProvenanceBadge kind="per_saham" />
      </>,
    );
    expect(screen.getByText("Basis berubah").getAttribute("title")).toMatch(/basis/);
    expect(screen.getByText("Tanda bertentangan").getAttribute("title")).toMatch(/tanda/);
    expect(screen.getByText("Per saham, perkiraan").getAttribute("title")).toMatch(/perkiraan/);
  });
});
