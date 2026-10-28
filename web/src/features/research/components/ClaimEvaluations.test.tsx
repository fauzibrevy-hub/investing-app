import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { json, mockApi } from "../../../test/mockApi";
import { golden } from "../testing/canon";
import type { ClaimEvalsResponse, ClaimsResponse } from "../types";
import { ClaimEvaluations } from "./ClaimEvaluations";

const CLAIM = golden<ClaimsResponse>("claims-COAL").items.find((c) => c.id === "c-e904edb8bb5c5152")!;
const URL_EVALS = "/api/research/claims/c-e904edb8bb5c5152/evals";

describe("ClaimEvaluations", () => {
  it("lists the evaluation history, newest first, with status word and value", async () => {
    const evals = golden<ClaimEvalsResponse>("claim-evals-c1");
    evals.items = [evals.items[0], { ...evals.items[0], status: "broken", label: "Patah", evaluated_at: "2026-10-25T09:00:00+00:00", value: -5 }];
    mockApi({ [URL_EVALS]: evals });
    render(<ClaimEvaluations claim={CLAIM} currency="USD" />);
    const list = await screen.findByRole("list");
    const items = within(list).getAllByRole("listitem");
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveTextContent("Patah");
    expect(items[1]).toHaveTextContent("Berlaku");
    expect(items[0].querySelector(".num")).toHaveTextContent(/^−5/);
    expect(items[0].className).toContain("min-h-11");
  });

  it("loading, empty and error with retry", async () => {
    mockApi({ [URL_EVALS]: () => new Promise<Response>(() => {}) });
    const first = render(<ClaimEvaluations claim={CLAIM} />);
    expect(screen.getByText("Memuat evaluasi…")).toBeInTheDocument();
    first.unmount();

    mockApi({ [URL_EVALS]: { ...golden<ClaimEvalsResponse>("claim-evals-c1"), items: [] } });
    const second = render(<ClaimEvaluations claim={CLAIM} />);
    expect(await screen.findByText("Belum dievaluasi")).toBeInTheDocument();
    second.unmount();

    let fail = true;
    mockApi({ [URL_EVALS]: () => (fail ? json({ detail: "gagal" }, 500) : json(golden<ClaimEvalsResponse>("claim-evals-c1"))) });
    render(<ClaimEvaluations claim={CLAIM} />);
    const retry = await screen.findByRole("button", { name: "Coba lagi" });
    fail = false;
    fireEvent.click(retry);
    await waitFor(() => expect(screen.getByRole("list")).toBeInTheDocument());
  });
});
