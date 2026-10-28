import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DossierView } from "./DossierView";
import type { ThesisDetail } from "./types";

const detail: ThesisDetail = {
  ticker: "BUMI",
  company: "Bumi Resources",
  status: "holdings",
  verdict: "Deleveraging",
  narrative_status: null,
  fundamental_status: null,
  growth_status: null,
  falsifiers: { count: 0, nearest: null },
  review: { last_date: "2026-10-01", days_ago: 19, due: false },
  evidence: { resolved: 0, doc_quoted: 0, missing: 0, total: 0 },
  tahap: null,
  errors: [],
  alasan_masuk: "Utang turun.",
  verdict_text: null,
  lens: { metrics: [], locked: null, approved_at: null },
  falsifier_items: [],
  review_log: [
    {
      date: "2026-10-01",
      title: "Q3",
      bullets: [
        { level: 0, text: "**Tindak lanjut:** tunggu LK." },
        { level: 1, text: "cek *margin* dan `net_debt`" },
      ],
    },
  ],
  doc_evidence: [],
  chain: [],
  trigger_tambah: [],
  trigger_buang: [],
};

describe("DossierView review log", () => {
  it("renders the inline emphasis of a bullet instead of its literal asterisks", () => {
    const { container } = render(<DossierView detail={detail} />);
    const items = [...container.querySelectorAll("li")].filter((li) => li.textContent?.includes("tunggu LK") || li.textContent?.includes("margin"));
    expect(items.map((li) => li.textContent)).toEqual(["Tindak lanjut: tunggu LK.", "cek margin dan net_debt"]);
    expect(items[0].querySelector("strong")).toHaveTextContent("Tindak lanjut:");
    expect(items[1].querySelector("em")).toHaveTextContent("margin");
    expect(items[1].querySelector("code")).toHaveTextContent("net_debt");
    expect(screen.queryByText(/\*\*/)).toBeNull();
  });
});
