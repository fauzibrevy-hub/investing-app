import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { json, mockApi } from "../../../test/mockApi";
import { FactChip } from "./FactChip";
import { FactDrawer, FactDrawerBody, FactDrawerProvider } from "./FactDrawer";
import type { FactDetail } from "../types";
import { useFactDrawer } from "./factDrawerContext";
import { FACT_DETAIL, OPERAND_TARGET, TARGET } from "./fixtures";

const realMatchMedia = window.matchMedia;

function setDesktop(desktop: boolean) {
  window.matchMedia = ((query: string) =>
    ({ matches: desktop && query.includes("min-width: 1024px"), media: query, addEventListener() {}, removeEventListener() {} }) as unknown as MediaQueryList) as typeof window.matchMedia;
}

const factPath = (t: { filing_id: string; element: string }) => `/api/research/facts/${encodeURIComponent(t.filing_id)}/${encodeURIComponent(t.element)}`;

beforeEach(() => setDesktop(true));
afterEach(() => {
  window.matchMedia = realMatchMedia;
});

describe("FactDrawer", () => {
  it("closed without a target: renders nothing and requests nothing", () => {
    const fetchMock = mockApi({});
    const { container } = render(<FactDrawer target={null} onClose={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("loading: skeleton blocks in a busy status region inside the dialog", async () => {
    mockApi({ [factPath(TARGET)]: () => new Promise<Response>(() => {}) });
    render(<FactDrawer target={TARGET} onClose={vi.fn()} />);
    const dialog = await screen.findByRole("dialog", { name: "Asal angka" });
    const status = within(dialog).getByRole("status");
    expect(status).toHaveAttribute("aria-busy", "true");
    expect(status.querySelectorAll("[data-skeleton]").length).toBeGreaterThan(0);
  });

  it("default (desktop): a 380 px modal dialog with every design.md §6 field", async () => {
    const fetchMock = mockApi({ [factPath(TARGET)]: FACT_DETAIL });
    render(<FactDrawer target={TARGET} onClose={vi.fn()} />);
    const dialog = await screen.findByRole("dialog", { name: "Asal angka" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog).toHaveClass("w-[380px]", "right-0");
    await within(dialog).findByText("Nilai sebagaimana dilaporkan");

    const url = new URL(String(fetchMock.mock.calls[0][0]), "http://localhost");
    expect(url.searchParams.get("ctx")).toBe(TARGET.ctx);

    const text = dialog.textContent ?? "";
    expect(text).toContain("250.000.000.000");
    expect(text).toContain("IDR");
    expect(text).toContain("-6 (pembulatan Jutaan)");
    expect(text).toContain("idx-cor:ProfitLoss");
    expect(text).toContain("Laba (rugi)");
    expect(text).toContain("Profit (loss)");
    expect(text).toContain("1 Jan 2026 s.d. 30 Jun 2026");
    expect(text).toContain("Tanpa dimensi");
    expect(text).toContain("AAAA 2026 Q2");
    expect(text).toContain("perkiraan 29 Agu 2026");
    expect(text).toContain("Belum diaudit");
    expect(text).toContain("general");
    expect(text).toContain("data/XBRL/AAAA/2026/Q2/instance.xbrl");
    expect(text).toContain("Berkas PDF: tautan belum tersedia");
  });

  it("derivation lists the formula with operand FactChips; restatement history shows original and restated", async () => {
    mockApi({ [factPath(TARGET)]: FACT_DETAIL });
    render(<FactDrawer target={TARGET} onClose={vi.fn()} />);
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByText("ytd_q2 − ytd_q1");
    expect(within(dialog).getByRole("button", { name: /Asal angka ytd_q1/ })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: /Asal angka ytd_q2/ })).toBeInTheDocument();
    expect(dialog.textContent).toContain("terdeteksi 30 Nov 2026");
    expect(dialog.textContent).toContain("262.000.000.000");
    expect(within(dialog).getByText(/· asli/)).toBeInTheDocument();
  });

  it("a fact that was never restated and feeds nothing says so", () => {
    render(<FactDrawerBody status="ready" detail={{ ...FACT_DETAIL, derived: [], restatements: [], history: [FACT_DETAIL.history[0]] }} />);
    expect(screen.getByText("Tidak pernah disajikan kembali.")).toBeInTheDocument();
    expect(screen.getByText(/tidak ada sel turunan/)).toBeInTheDocument();
  });

  it("error: 'Gagal memuat asal angka' with a retry that loads again", async () => {
    let attempts = 0;
    mockApi({
      [factPath(TARGET)]: () => {
        attempts += 1;
        return attempts === 1 ? json({ detail: "store down" }, 500) : json(FACT_DETAIL);
      },
    });
    render(<FactDrawer target={TARGET} onClose={vi.fn()} />);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Gagal memuat asal angka");
    fireEvent.click(within(alert).getByRole("button", { name: "Coba lagi" }));
    await screen.findByText("Nilai sebagaimana dilaporkan");
    expect(attempts).toBe(2);
  });

  it("Esc closes and focus returns to the trigger", async () => {
    mockApi({ [factPath(TARGET)]: FACT_DETAIL });
    const onClose = vi.fn();
    const trigger = document.createElement("button");
    document.body.append(trigger);
    const { unmount } = render(<FactDrawer target={TARGET} onClose={onClose} returnFocusTo={trigger} />);
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveFocus();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
    unmount();
    expect(trigger).toHaveFocus();
    trigger.remove();
  });

  it("traps Tab and Shift+Tab inside the dialog", async () => {
    mockApi({ [factPath(TARGET)]: FACT_DETAIL });
    render(
      <>
        <button>di luar</button>
        <FactDrawer target={TARGET} onClose={vi.fn()} />
      </>,
    );
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByText("ytd_q2 − ytd_q1");
    const focusable = Array.from(dialog.querySelectorAll<HTMLElement>("button"));
    const last = focusable[focusable.length - 1];
    last.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(focusable[0]).toHaveFocus();
    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(last).toHaveFocus();
    screen.getByRole("button", { name: "di luar" }).focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(dialog.contains(document.activeElement)).toBe(true);
  });

  it("mobile: the same content in a Sheet", async () => {
    setDesktop(false);
    mockApi({ [factPath(TARGET)]: FACT_DETAIL });
    render(<FactDrawer target={TARGET} onClose={vi.fn()} />);
    const dialog = await screen.findByRole("dialog", { name: "Asal angka" });
    expect(dialog).not.toHaveClass("w-[380px]");
    expect(dialog).toHaveClass("max-w-mobile");
    await within(dialog).findByText("Nilai sebagaimana dilaporkan");
  });
});

/** Nilai di bawah label `<dt>` tertentu, supaya angka yang sama di tempat lain tidak menutupi kegagalan. */
function field(root: HTMLElement, label: string): string {
  const dt = within(root).getByText(label, { selector: "dt" });
  return dt.nextElementSibling?.textContent ?? "";
}

const OPERAND_DETAIL: FactDetail = {
  ...FACT_DETAIL,
  fact: { ...FACT_DETAIL.fact, filing_id: OPERAND_TARGET.filing_id, end: "2026-03-31", value: 100_000_000_000 },
  filing: { ...FACT_DETAIL.filing, id: OPERAND_TARGET.filing_id, period: "Q1", ticker: "BBBB" },
  derived: [],
  restatements: [],
  history: [],
};

describe("FactDrawer: a fact read from the original PDF", () => {
  const PDF_DETAIL: FactDetail = {
    ...FACT_DETAIL,
    fact: { ...FACT_DETAIL.fact, filing_id: "BUMI_2024_Q1", cite: { kind: "pdf", page: 6, label: "Ekuitas - Neto / Equity - Net" } },
    filing: { ...FACT_DETAIL.filing, id: "BUMI_2024_Q1", source_kind: "pdf", source_path: "pdf-filings/BUMI_2024_Q1.json" },
    history: [{ filing_id: "BUMI_2024_Q1", value: 2_845_548_053, decimals: 0, source_kind: "pdf", cite: { kind: "pdf", page: 6, label: "Ekuitas - Neto / Equity - Net" }, available_from: "2024-04-23", original: true }],
    derived: [
      {
        ...FACT_DETAIL.derived[0],
        inputs: [
          { role: "ytd_q2", value: 250_000_000_000, fact: TARGET, cite: null },
          { role: "ytd_q1", value: 100_000_000_000, fact: OPERAND_TARGET, cite: { kind: "pdf", page: 7, label: "PENDAPATAN / REVENUES" } },
        ],
      },
    ],
  };

  it("shows a PDF badge with the page and the printed label, and never an XBRL badge or link", () => {
    render(<FactDrawerBody status="ready" detail={PDF_DETAIL} />);
    const source = screen.getByText("Sumber").closest("div")!;
    expect(within(source).getByText("PDF · hal. 6")).toHaveAttribute("data-provenance", "pdf");
    expect(within(source).getByText("Ekuitas - Neto / Equity - Net")).toBeInTheDocument();
    expect(within(source).getByText("Dibaca dari laporan asli (PDF), bukan XBRL")).toBeVisible();
    expect(document.querySelector('[data-provenance="xbrl"]')).toBeNull();
    expect(screen.queryByText(/Berkas XBRL/)).toBeNull();
    expect(screen.getByText(/Berkas data hasil baca PDF/)).toBeInTheDocument();
  });

  it("an operand read from the PDF carries its own page in the derivation list", () => {
    render(<FactDrawerBody status="ready" detail={PDF_DETAIL} />);
    const q1 = screen.getByText("ytd_q1").closest("li")!;
    expect(within(q1).getByText("PDF · hal. 7")).toHaveAttribute("data-provenance", "pdf");
    const q2 = screen.getByText("ytd_q2").closest("li")!;
    expect(q2.querySelector("[data-provenance]")).toBeNull();
  });

  it("an XBRL fact has no PDF badge and keeps its XBRL file line", () => {
    render(<FactDrawerBody status="ready" detail={FACT_DETAIL} />);
    expect(document.querySelector('[data-provenance="pdf"]')).toBeNull();
    expect(screen.queryByText("Sumber")).toBeNull();
    expect(screen.getByText(/Berkas XBRL/)).toBeInTheDocument();
  });

  it("an old payload without cite or source_kind does not throw and does not claim XBRL", () => {
    const { cite: _c, ...fact } = FACT_DETAIL.fact;
    const { source_kind: _s, ...filing } = FACT_DETAIL.filing;
    const old = { ...FACT_DETAIL, fact, filing, history: FACT_DETAIL.history.map(({ cite: _h, source_kind: _k, ...h }) => h), derived: FACT_DETAIL.derived.map((d) => ({ ...d, inputs: d.inputs.map(({ cite: _i, ...i }) => i) })) } as unknown as FactDetail;
    render(<FactDrawerBody status="ready" detail={old} />);
    expect(document.querySelector('[data-provenance="pdf"]')).toBeNull();
    expect(screen.queryByText(/Berkas XBRL/)).toBeNull();
    expect(screen.getByText(/Asal laporan tidak diketahui/)).toBeInTheDocument();
  });
});

describe("FactDrawer content branches", () => {
  const render0 = (detail: FactDetail) => render(<FactDrawerBody status="ready" detail={detail} />);

  it("an instant fact reads 'Posisi pada', dimensions are listed, an exact value keeps its fraction", () => {
    const { container } = render0({
      ...FACT_DETAIL,
      fact: { ...FACT_DETAIL.fact, ctx_kind: "instant", start: null, end: "2026-06-30", dims: "axis=member;axis2=m2", value: 12.5, decimals: null },
    });
    const root = container as HTMLElement;
    expect(field(root, "Konteks")).toBe("Posisi pada 30 Jun 2026");
    expect(field(root, "Dimensi")).toBe("axis = member, axis2 = m2");
    expect(field(root, "Nilai sebagaimana dilaporkan")).toBe("12,5");
    expect(field(root, "Desimal")).toBe("tidak dinyatakan");
  });

  it("positive decimals fix the shown precision", () => {
    const { container } = render0({ ...FACT_DETAIL, fact: { ...FACT_DETAIL.fact, value: 0.1234, decimals: 4, unit: "pure" } });
    expect(field(container as HTMLElement, "Nilai sebagaimana dilaporkan")).toBe("0,1234");
    expect(field(container as HTMLElement, "Satuan")).toBe("pure");
  });

  it("a negative reported value uses U+2212", () => {
    const { container } = render0({ ...FACT_DETAIL, fact: { ...FACT_DETAIL.fact, value: -4_500, decimals: 0 } });
    expect(field(container as HTMLElement, "Nilai sebagaimana dilaporkan")).toBe("−4.500");
  });

  it("filing: a declared filing date and an audited filing", () => {
    const { container } = render0({ ...FACT_DETAIL, filing: { ...FACT_DETAIL.filing, filed_at: "2026-08-10", available_source: "declared", audited: true } });
    const root = container as HTMLElement;
    expect(field(root, "Diajukan")).toBe("10 Agu 2026");
    expect(field(root, "Audit")).toBe("Diaudit");
    expect(field(root, "Emiten dan periode")).toBe("AAAA 2026 Q2");
    expect(field(root, "Taksonomi")).toBe("general");
  });

  it("restatement figures keep their fraction too", () => {
    render0({ ...FACT_DETAIL, restatements: [{ ...FACT_DETAIL.restatements[0], original: 12.5, restated: 12.4 }] });
    expect(screen.getByText(/menjadi/).textContent).toContain("12,5");
    expect(screen.getByText(/menjadi/).textContent).toContain("12,4");
  });

  it("the English label is dropped when it equals the Indonesian one", () => {
    const { container } = render0({ ...FACT_DETAIL, fact: { ...FACT_DETAIL.fact, label: { id: "Kas", en: "Kas" } } });
    expect(field(container as HTMLElement, "Label")).toBe("Kas");
  });

  it("a ready body without detail is the error state, not an empty panel", () => {
    render(<FactDrawerBody status="ready" />);
    expect(screen.getByRole("alert")).toHaveTextContent("Gagal memuat asal angka");
  });

  it("loading announces itself to screen readers", () => {
    render(<FactDrawerBody status="loading" />);
    expect(screen.getByRole("status")).toHaveTextContent("Memuat asal angka");
  });
});

describe("FactDrawerProvider", () => {
  function Probe() {
    const drawer = useFactDrawer();
    return <output aria-label="terbuka">{String(drawer?.isOpen)}</output>;
  }
  function Page() {
    return (
      <FactDrawerProvider>
        <Probe />
        <FactChip value={250} text="250" target={TARGET} label="Laba bersih" period="Q2 26" />
      </FactDrawerProvider>
    );
  }
  const chipName = /^Asal angka Laba bersih Q2 26: /;

  it("an operand opens the other fact in the same drawer: it fetches that fact and the value field changes", async () => {
    const fetchMock = mockApi({ [factPath(TARGET)]: FACT_DETAIL, [factPath(OPERAND_TARGET)]: OPERAND_DETAIL });
    render(<Page />);
    fireEvent.click(screen.getByRole("button", { name: chipName }));
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByText("ytd_q1");
    expect(field(dialog, "Nilai sebagaimana dilaporkan")).toBe("250.000.000.000");

    fireEvent.click(within(dialog).getByRole("button", { name: /Asal angka ytd_q1/ }));
    await waitFor(() => expect(field(screen.getByRole("dialog"), "Nilai sebagaimana dilaporkan")).toBe("100.000.000.000"));
    const fetched = fetchMock.mock.calls.map(([u]) => new URL(String(u), "http://localhost").pathname);
    expect(fetched).toContain(factPath(OPERAND_TARGET));
    expect(field(screen.getByRole("dialog"), "Emiten dan periode")).toBe("BBBB 2026 Q1");
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
  });

  it("focus stays inside the dialog when the focused operand is replaced by the next fact", async () => {
    mockApi({ [factPath(TARGET)]: FACT_DETAIL, [factPath(OPERAND_TARGET)]: OPERAND_DETAIL });
    render(<Page />);
    fireEvent.click(screen.getByRole("button", { name: chipName }));
    const dialog = await screen.findByRole("dialog");
    const operand = await within(dialog).findByRole("button", { name: /Asal angka ytd_q1/ });
    operand.focus();
    fireEvent.click(operand);
    await waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));
    await waitFor(() => expect(field(dialog, "Emiten dan periode")).toBe("BBBB 2026 Q1"));
    expect(dialog.contains(document.activeElement)).toBe(true);
    expect(document.activeElement).not.toBe(document.body);
  });

  it("Esc closes and returns focus to the chip even when a click did not focus it (Safari)", async () => {
    mockApi({ [factPath(TARGET)]: FACT_DETAIL });
    render(<Page />);
    const chip = screen.getByRole("button", { name: chipName });
    fireEvent.click(chip); // no chip.focus(): the trigger passed by the chip is what restores focus
    await screen.findByRole("dialog");
    expect(screen.getByLabelText("terbuka")).toHaveTextContent("true");
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(chip).toHaveFocus();
    expect(screen.getByLabelText("terbuka")).toHaveTextContent("false");
  });

  it("the close button closes the drawer", async () => {
    mockApi({ [factPath(TARGET)]: FACT_DETAIL });
    render(<Page />);
    fireEvent.click(screen.getByRole("button", { name: chipName }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Tutup" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("focus falls back to the previously focused element when the trigger left the document", async () => {
    mockApi({ [factPath(TARGET)]: FACT_DETAIL });
    const fallback = document.createElement("button");
    document.body.append(fallback);
    fallback.focus();
    const gone = document.createElement("button");
    const { unmount } = render(<FactDrawer target={TARGET} onClose={vi.fn()} returnFocusTo={gone} />);
    await screen.findByRole("dialog");
    unmount();
    expect(fallback).toHaveFocus();
    fallback.remove();
  });

  it("Escape is left alone when something else already handled it", async () => {
    mockApi({ [factPath(TARGET)]: FACT_DETAIL });
    const onClose = vi.fn();
    render(<FactDrawer target={TARGET} onClose={onClose} />);
    await screen.findByRole("dialog");
    const handled = new KeyboardEvent("keydown", { key: "Escape", cancelable: true, bubbles: true });
    handled.preventDefault();
    document.dispatchEvent(handled);
    expect(onClose).not.toHaveBeenCalled();
  });
});

describe("FactDrawer on mobile (Sheet)", () => {
  beforeEach(() => setDesktop(false));

  it("Esc closes the sheet", async () => {
    mockApi({ [factPath(TARGET)]: FACT_DETAIL });
    const onClose = vi.fn();
    render(<FactDrawer target={TARGET} onClose={onClose} />);
    await screen.findByRole("dialog", { name: "Asal angka" });
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });

  it("Tab does not leave the sheet (the Sheet component alone would let it)", async () => {
    mockApi({ [factPath(TARGET)]: FACT_DETAIL });
    render(
      <>
        <button>di luar</button>
        <FactDrawer target={TARGET} onClose={vi.fn()} />
      </>,
    );
    const dialog = await screen.findByRole("dialog", { name: "Asal angka" });
    await within(dialog).findByText("ytd_q2 − ytd_q1");
    const items = Array.from(dialog.querySelectorAll<HTMLElement>("button"));
    items[items.length - 1].focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(items[0]).toHaveFocus();
    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(items[items.length - 1]).toHaveFocus();
  });

  it("closing returns focus to the chip, not to <body>", async () => {
    mockApi({ [factPath(TARGET)]: FACT_DETAIL });
    function Page() {
      return (
        <FactDrawerProvider>
          <FactChip value={250} text="250" target={TARGET} label="Laba bersih" period="Q2 26" />
        </FactDrawerProvider>
      );
    }
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      render(<Page />);
      const chip = screen.getByRole("button", { name: /^Asal angka Laba bersih Q2 26: / });
      fireEvent.click(chip);
      const dialog = await screen.findByRole("dialog");
      fireEvent.click(within(dialog).getByRole("button", { name: "Tutup" }));
      await act(async () => void vi.advanceTimersByTime(10));
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(chip).toHaveFocus();
    } finally {
      vi.useRealTimers();
    }
  });
});
