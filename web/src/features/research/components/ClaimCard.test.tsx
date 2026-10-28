import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { json, mockApi } from "../../../test/mockApi";
import { golden } from "../testing/canon";
import type { Claim, ClaimEvidenceResponse, ClaimsResponse, DossierResponse, FinancialsResponse } from "../types";
import { ClaimCard, ClaimsPanel, conditionText, sortClaims, unitWord, webUrl } from "./ClaimCard";

const CLAIMS = golden<ClaimsResponse>("claims-COAL");
const DOSSIER = golden<DossierResponse>("ticker-COAL");

const inRouter = (ui: React.ReactNode) => render(<MemoryRouter>{ui}</MemoryRouter>);

describe("ClaimCard (design.md §3.4, §6)", () => {
  it("sorts Patah, Belum teruji, Berlaku, Kedaluwarsa, closed claims last, stable within a status", () => {
    const closed: Claim = { ...CLAIMS.items[0], id: "c-closed", closed_at: "2026-10-25T00:00:00Z" };
    const order = sortClaims([closed, ...CLAIMS.items]).map((c) => (c.closed_at ? "closed" : c.status));
    expect(order).toEqual(["broken", "not_testable", "holding", "holding", "expired", "closed"]);
  });

  it("shows the status as dot plus word, the meta line and the last evaluation from the payload", () => {
    const broken = CLAIMS.items.find((c) => c.status === "broken")!;
    inRouter(<ClaimCard claim={broken} currency="USD" />);
    const card = screen.getByRole("article", { name: broken.statement });
    const mark = within(card).getByText("Patah");
    expect(mark.closest("[data-status]")).toHaveAttribute("data-status", "broken");
    expect(mark.querySelector("[aria-hidden='true']")).not.toBeNull();
    expect(within(card).getByText("Laba bersih < 0 USD · jatuh tempo 31 Des 2026")).toBeInTheDocument();
    expect(within(card).getByText(/^Terakhir 21\.800 ribu USD · 24 Okt 2026 · 0 bukti$/)).toBeInTheDocument();
  });

  it("Bukti lists the claim's evidence from GET /claims/{id}/evidence with a link to Sumber", async () => {
    const evidence = golden<ClaimEvidenceResponse>("claim-evidence-c3");
    const claim = CLAIMS.items.find((c) => c.id === evidence.claim_id) ?? { ...CLAIMS.items[0], id: evidence.claim_id };
    mockApi({ [`/api/research/claims/${evidence.claim_id}/evidence`]: evidence });
    inRouter(<ClaimCard claim={claim} />);
    fireEvent.click(screen.getByRole("button", { name: "Bukti" }));
    expect(await screen.findByRole("link", { name: "Buka di Sumber" })).toHaveAttribute("href", "/?section=sumber");
    expect(evidence.items.length).toBeGreaterThan(0);
    expect(screen.getByText(evidence.items[0].kind_label)).toBeInTheDocument();
  });

  it("evidence links open only http(s) addresses; other schemes stay plain text", () => {
    expect(webUrl("https://example.test/a")).toBe("https://example.test/a");
    expect(webUrl(["java", "script:alert(1)"].join(""))).toBeNull();
    expect(webUrl("data:text/html,x")).toBeNull();
    expect(webUrl("laporan tahunan")).toBeNull();
  });

  it("Tutup klaim keeps focus: Batal on confirm, back to Tutup klaim on cancel, the card after closing", async () => {
    mockApi({ [`/api/research/claims/${CLAIMS.items[0].id}`]: () => json({ ...CLAIMS.items[0], closed_at: "2026-10-25T00:00:00Z" }) });
    inRouter(<ClaimCard claim={CLAIMS.items[0]} />);
    fireEvent.click(screen.getByRole("button", { name: "Tutup klaim" }));
    expect(screen.getByRole("button", { name: "Batal" })).toHaveFocus();
    fireEvent.click(screen.getByRole("button", { name: "Batal" }));
    expect(screen.getByRole("button", { name: "Tutup klaim" })).toHaveFocus();
    fireEvent.click(screen.getByRole("button", { name: "Tutup klaim" }));
    fireEvent.click(screen.getByRole("button", { name: "Ya, tutup" }));
    await waitFor(() => expect(screen.getByRole("article", { name: CLAIMS.items[0].statement })).toHaveFocus());
    expect(screen.getByRole("status")).toHaveTextContent("Klaim ditutup");
  });

  it("Tutup klaim asks once, then sends PATCH {closed: true}", async () => {
    const patches: unknown[] = [];
    mockApi({
      [`/api/research/claims/${CLAIMS.items[0].id}`]: (_u: URL, init: RequestInit) => {
        patches.push({ method: init.method, body: JSON.parse(String(init.body)) });
        return json({ ...CLAIMS.items[0], closed_at: "2026-10-25T00:00:00Z" });
      },
    });
    inRouter(<ClaimCard claim={CLAIMS.items[0]} />);
    fireEvent.click(screen.getByRole("button", { name: "Tutup klaim" }));
    expect(screen.getByText("Klaim yang ditutup tidak bisa diedit atau dibuka lagi.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Ya, tutup" }));
    await waitFor(() => expect(patches).toEqual([{ method: "PATCH", body: { closed: true } }]));
  });

  it("names the reporting currency in sentences, never a guessed Rp", () => {
    expect(unitWord("ccy", "IDR")).toBe("Rp");
    expect(unitWord("ccy", "USD")).toBe("USD");
    expect(unitWord("ccy", null)).toBe("mata uang laporan");
    expect(conditionText("between", 10, 20, "%")).toBe("antara 10 dan 20 %");
  });
});

describe("ClaimsPanel", () => {
  const backend = (claims: unknown) =>
    mockApi({
      "/api/research/COAL/claims": claims,
      "/api/research/COAL/financials": golden<FinancialsResponse>("financials-COAL-standalone"),
    });

  it("lists the golden claims in status order; Tambah klaim opens the editor at the top and Batal returns focus", async () => {
    backend(CLAIMS);
    inRouter(<ClaimsPanel ticker="COAL" dossier={DOSSIER} />);
    await screen.findAllByRole("article");
    const words = screen.getAllByRole("article").map((a) => a.getAttribute("data-claim-status"));
    expect(words).toEqual(["broken", "not_testable", "holding", "holding", "expired"]);
    fireEvent.click(screen.getByRole("button", { name: "Tambah klaim" }));
    const form = screen.getByRole("form", { name: "Klaim baru" });
    expect(form.compareDocumentPosition(screen.getAllByRole("article")[0]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    fireEvent.click(within(form).getByRole("button", { name: "Batal" }));
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Tambah klaim" }));
  });

  it("empty: the gap names its action; error: retry", async () => {
    backend({ ...CLAIMS, items: [] });
    const { unmount } = inRouter(<ClaimsPanel ticker="COAL" dossier={DOSSIER} />);
    expect(await screen.findByText(/Belum ada klaim/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Tambah klaim" })).toBeInTheDocument();
    unmount();
    backend(() => json({ detail: "gagal" }, 500));
    inRouter(<ClaimsPanel ticker="COAL" dossier={DOSSIER} />);
    expect(await screen.findByText("gagal")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Coba lagi" })).toBeInTheDocument();
  });
});
