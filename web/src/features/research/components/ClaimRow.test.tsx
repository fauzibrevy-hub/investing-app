import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { golden } from "../testing/canon";
import type { ClaimsResponse } from "../types";
import { ClaimRow } from "./ClaimRow";

const CLAIMS = golden<ClaimsResponse>("claims-COAL").items;
const byStatus = (s: string) => CLAIMS.find((c) => c.status === s)!;

describe("ClaimRow (design.md §5.2, compact ClaimCard)", () => {
  it("is one 44 px button: status dot + word, statement, due date and last value", () => {
    render(<ClaimRow claim={byStatus("holding")} currency="USD" onOpen={() => {}} />);
    const row = screen.getByRole("button", { name: new RegExp(`${byStatus("holding").statement}`) });
    expect(row.className).toContain("min-h-11");
    expect(row).toHaveAttribute("aria-haspopup", "dialog");
    expect(row.querySelector("[data-status='holding'] > [aria-hidden='true']")).not.toBeNull();
    expect(row).toHaveTextContent("Berlaku");
    expect(row).toHaveTextContent(/jatuh tempo \d{1,2} \w{3} \d{4} · Terakhir /);
  });

  it("each status keeps its own word beside the dot (never colour alone)", () => {
    for (const [status, word] of [["broken", "Patah"], ["not_testable", "Belum teruji"], ["holding", "Berlaku"], ["expired", "Kedaluwarsa"]]) {
      const { unmount } = render(<ClaimRow claim={byStatus(status)} onOpen={() => {}} />);
      expect(screen.getByRole("button")).toHaveTextContent(word);
      unmount();
    }
  });

  it("without a value or an evaluation it says so, and a closed claim is dashed and marked", () => {
    const { unmount } = render(<ClaimRow claim={byStatus("not_testable")} onOpen={() => {}} />);
    expect(screen.getByRole("button")).toHaveTextContent("Belum ada nilai");
    unmount();
    render(<ClaimRow claim={{ ...byStatus("holding"), last_eval: null, closed_at: "2026-10-28T03:00:00+00:00" }} onOpen={() => {}} />);
    const row = screen.getByRole("button");
    expect(row).toHaveTextContent("Belum dievaluasi");
    expect(row).toHaveTextContent("ditutup");
    expect(row.className).toContain("border-dashed");
  });

  it("tapping hands the claim and the button (focus return target) to onOpen", () => {
    const onOpen = vi.fn();
    const claim = byStatus("broken");
    render(<ClaimRow claim={claim} onOpen={onOpen} />);
    const row = screen.getByRole("button");
    fireEvent.click(row);
    expect(onOpen).toHaveBeenCalledWith(claim, row);
  });
});
