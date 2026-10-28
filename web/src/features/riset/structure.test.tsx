import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import financials from "../../../../tests/fixtures/research/canon/financials-COAL-standalone.json";
import pack from "../../../../tests/fixtures/research/canon/pack-COAL.json";
import golden from "../../../../tests/fixtures/research/canon/structure-COAL.json";
import { json, mockApi } from "../../test/mockApi";
import type { StructureNode, StructureResponse } from "../research/types";
import { Struktur } from "./blocks/Struktur";
import draft from "../../../../tests/fixtures/research/structure-BUMI.draft.json";
import { READABLE_SCALE } from "./mindmapView";
import { buildTree, pageRef } from "./structureData";

/**
 * Payload backend asli (`GET /COAL/structure`, golden dari `tests/research_canon.rs`): n1 diterima, n2 dan n3 usulan,
 * n4 ditolak. Varian tes menyalin node golden dan mengganti field yang diuji.
 */
const GOLDEN = golden as StructureResponse;

function node(over: Partial<StructureNode> & Pick<StructureNode, "id" | "label">): StructureNode {
  return { ...GOLDEN.nodes[0], source: null, ...over };
}

const NODES: StructureNode[] = GOLDEN.nodes;
const PROPOSED = NODES.find((n) => n.id === "n2")!;

function structure(nodes: StructureNode[], available = false): StructureResponse {
  const count = (s: StructureNode["state"]) => nodes.filter((n) => n.state === s).length;
  return {
    ...GOLDEN,
    nodes,
    counts: { proposed: count("proposed"), accepted: count("accepted"), rejected: count("rejected") },
    draft: { ...GOLDEN.draft, available },
  };
}

function renderBlock() {
  return render(
    <MemoryRouter>
      <Struktur ticker="COAL" note={null} />
    </MemoryRouter>,
  );
}

/** Peta pikiran adalah tampilan awal; pohon (untuk pembaca layar dan penyuntingan) ada di balik "Lihat sebagai daftar". */
async function renderList() {
  renderBlock();
  fireEvent.click(await screen.findByRole("button", { name: "Lihat sebagai daftar" }));
}

const base = "/api/research/COAL";

/** A map card is named in visible order, kind first (WCAG 2.5.3): "Segmen Segmen batubara", "Pelanggan USULAN Pembeli di Asia", then the note or the metric value when the card shows one. The "+N cabang" chip starts with "+", so it never matches. */
const cardName = (label: string) => new RegExp(`^(?!\\+).* ${label}( .*)?$`);

describe("structure tree: proposed vs accepted, and the accept action", () => {
  let nodes: StructureNode[];
  let fetchMock: ReturnType<typeof mockApi>;

  beforeEach(() => {
    nodes = NODES.map((n) => ({ ...n }));
    fetchMock = mockApi({
      [`${base}/structure`]: () => json(structure(nodes)),
      [`${base}/structure/nodes/n2`]: (_url: URL, init: RequestInit) => {
        const body = JSON.parse(String(init.body)) as { state?: StructureNode["state"] };
        nodes = nodes.map((n) => (n.id === "n2" ? { ...n, state: body.state ?? n.state, state_label: body.state === "accepted" ? "diterima" : n.state_label } : n));
        return json(structure(nodes));
      },
      [`${base}/pack`]: pack,
      [`${base}/financials`]: financials,
    });
  });

  it("a note with English separators and (MT) renders in id-ID with Mt, in the list and on the map, while the stored text is untouched (N5)", async () => {
    nodes = nodes.map((n) => (n.id === "n1" ? { ...n, label: "Cadangan 52 MT", note: "Sumber daya 1,175,412,847 ton (MT) di Kalimantan" } : n));
    await renderList();
    expect(await screen.findByText("Cadangan 52 Mt")).toBeInTheDocument();
    expect(screen.getByText("Sumber daya 1.175.412.847 ton (Mt) di Kalimantan")).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/1,175|\(MT\)/);
    expect(nodes.find((n) => n.id === "n1")!.note).toBe("Sumber daya 1,175,412,847 ton (MT) di Kalimantan");
    fireEvent.click(screen.getByRole("button", { name: "Lihat sebagai peta" }));
    const card = (await screen.findByRole("button", { name: cardName("Cadangan 52 Mt") })) as HTMLElement;
    expect(card).toHaveTextContent("Sumber daya 1.175.412.847 ton (Mt) di Kalimantan");
  });

  it("an accepted node is plain; a proposed node is dashed, says usulan with its page, and offers Terima / Ubah / Tolak at 44 px", async () => {
    await renderList();
    const accepted = (await screen.findByText("Segmen batubara")).closest("[data-node]") as HTMLElement;
    expect(accepted).toHaveAttribute("data-state", "accepted");
    expect(accepted).not.toHaveClass("border-dashed");
    expect(within(accepted).queryByText("usulan")).toBeNull();
    expect(within(accepted).queryByRole("button", { name: /^Terima/ })).toBeNull();

    const proposed = screen.getByText("Batubara termal ekspor").closest("[data-node]") as HTMLElement;
    expect(proposed).toHaveAttribute("data-state", "proposed");
    expect(proposed).toHaveClass("border-dashed");
    expect(within(proposed).getByText("usulan")).toBeInTheDocument();
    expect(within(proposed).getByText(/hal\. 37/)).toBeInTheDocument();
    expect(within(proposed).getByText(/Penjualan batubara termal ke pembeli di Asia/)).toBeInTheDocument();
    for (const name of ["Terima Batubara termal ekspor", "Ubah Batubara termal ekspor", "Tolak Batubara termal ekspor"]) {
      expect(within(proposed).getByRole("button", { name })).toHaveClass("min-h-11");
    }
  });

  it("the tree nests children under their parent and a rejected node leaves the tree", async () => {
    await renderList();
    const child = (await screen.findByText("Pembeli di Asia")).closest("[data-node]") as HTMLElement;
    const parentItem = screen.getByText("Batubara termal ekspor").closest("li") as HTMLElement;
    expect(parentItem.contains(child)).toBe(true);
    const tree = screen.getByText("Segmen batubara").closest("ul") as HTMLElement;
    expect(within(tree).queryByText("Kontraktor tambang")).toBeNull();
    fireEvent.click(screen.getByText(/^Ditolak \(1\)/));
    expect(screen.getByRole("button", { name: "Terima Kontraktor tambang" })).toBeInTheDocument();
  });

  it("an owner node carries a Pemilik badge, and other nodes do not (N3)", async () => {
    nodes.push(node({ id: "n5", parent: "n1", label: "PT Contoh Tbk (pemilik)", kind: "pelanggan", kind_label: "Pelanggan", owner: true }));
    await renderList();
    const owner = (await screen.findByText("PT Contoh Tbk (pemilik)")).closest("[data-node]") as HTMLElement;
    expect(owner).toHaveAttribute("data-state", "accepted");
    expect(within(owner).getByText("Pemilik")).toBeInTheDocument();
    const plain = screen.getByText("Segmen batubara").closest("[data-node]") as HTMLElement;
    expect(within(plain).queryByText("Pemilik")).toBeNull();
  });

  it("a node linked to a metric shows its latest value as a tappable control", async () => {
    await renderList();
    const chip = await screen.findByRole("button", { name: /Produksi Q2 26: 6, ubah KPI/ });
    expect(chip).toHaveTextContent("6");
  });

  it("Terima writes state accepted through the API with an Idempotency-Key, and the node turns plain", async () => {
    await renderList();
    fireEvent.click(await screen.findByRole("button", { name: "Terima Batubara termal ekspor" }));
    await waitFor(() => expect(screen.getByText("Batubara termal ekspor").closest("[data-node]")).toHaveAttribute("data-state", "accepted"));
    const put = fetchMock.mock.calls.find(([input, init]) => String(input).endsWith("/structure/nodes/n2") && (init as RequestInit).method === "PUT")!;
    expect(JSON.parse(String((put[1] as RequestInit).body))).toEqual({ state: "accepted" });
    expect(new Headers((put[1] as RequestInit).headers).get("idempotency-key")).toBeTruthy();
    expect(within(screen.getByText("Batubara termal ekspor").closest("[data-node]") as HTMLElement).queryByText("usulan")).toBeNull();
  });
});

describe("structure block empty state and draft import", () => {
  it("explains the flow in one sentence and offers Muat draf and Tambah cabang; a missing draft says what to run", async () => {
    mockApi({
      [`${base}/structure`]: structure([], false),
      [`${base}/structure/draft`]: () => json({ detail: "No draft for COAL: COAL.draft.json is not in the structure folder" }, 404),
      [`${base}/pack`]: pack,
      [`${base}/financials`]: financials,
    });
    renderBlock();
    expect(await screen.findByText(/Claude Code menyusun draf dari laporan tahunan/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Tambah cabang" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Muat draf" }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Belum ada draf untuk COAL");
    expect(alert).toHaveTextContent("/draft-structure COAL");
  });

  it("a rejected draft names every field and imports nothing", async () => {
    mockApi({
      [`${base}/structure`]: structure([], true),
      [`${base}/structure/draft`]: () => json({ detail: [{ loc: ["file", "nodes", 1, "source", "snippet"], msg: "Snippet is longer than 160 characters", type: "too_long" }] }, 422),
      [`${base}/pack`]: pack,
      [`${base}/financials`]: financials,
    });
    renderBlock();
    fireEvent.click(await screen.findByRole("button", { name: "Muat draf" }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("ditolak utuh");
    expect(alert).toHaveTextContent(/160/);
  });
});

describe("structure mind map: cards, sheet, gestures and the list fallback", () => {
  let nodes: StructureNode[];
  let fetchMock: ReturnType<typeof mockApi>;

  const MAP_NODES: StructureNode[] = [
    node({ id: "n1", label: "Segmen batubara", note: "Penambangan dan pemasaran batubara termal di Kalimantan." }),
    // Dua cabang kedalaman 1 (n1 dan n2), masing-masing dengan satu anak (n5 dan n3).
    { ...PROPOSED, parent: null },
    node({ id: "n3", parent: "n2", label: "Pembeli di Asia", kind: "pelanggan", kind_label: "Pelanggan", state: "proposed", state_label: "usulan", source: { document: "laporan-tahunan-2025.pdf", page: 38, snippet: "Pembeli utama berada di Asia" } }),
    node({ id: "n4", label: "Kontraktor tambang", kind: "biaya", kind_label: "Biaya", state: "rejected", state_label: "ditolak" }),
    node({ id: "n5", parent: "n1", label: "PT Contoh Tbk (pemilik)", kind: "kepemilikan", kind_label: "Kepemilikan", owner: true }),
  ];

  const viewport = () => document.querySelector("[data-mindmap-viewport]") as HTMLElement;
  const layer = () => document.querySelector("[data-mindmap-layer]") as HTMLElement;
  const card = (id: string) => viewport().querySelector(`[data-node="${id}"]`) as HTMLElement | null;
  /** `translate(x px, y px) scale(s)` dari lapisan kanvas. */
  const readView = () => {
    const m = /translate\((-?[\d.]+)px, (-?[\d.]+)px\) scale\(([\d.]+)\)/.exec(layer().style.transform);
    if (!m) throw new Error(`no view in "${layer().style.transform}"`);
    return { x: Number(m[1]), y: Number(m[2]), scale: Number(m[3]) };
  };
  /** Tampilan awal sudah dipasang (ukuran kanvas terukur): lapisan tak lagi di (0, 0); peta satu sisi memang mulai di x = 0, jadi y ikut dihitung. */
  const settle = () => waitFor(() => expect(readView().x !== 0 || readView().y !== 0).toBe(true));
  /** Id kartu yang tergambar, urut id (urutan DOM: sisi kiri dulu). */
  const shown = () => [...viewport().querySelectorAll("[data-node]")].map((el) => el.getAttribute("data-node")).sort();
  /** Tombol buka / tutup cabang di sisi kartu `label`: satu nama tetap "+N cabang di …", keadaannya hanya di `aria-expanded` (M3). */
  const chip = (label: string) => screen.getByRole("button", { name: new RegExp(`^\\+\\d+ cabang di ${label}$`) });
  /** Kotak kartu di layar (piksel), dari posisi kartu di kanvas dan transformasi lapisan. */
  const screenRect = (el: HTMLElement) => {
    const v = readView();
    const num = (p: string) => Number.parseFloat(el.style.getPropertyValue(p));
    return { left: num("left") * v.scale + v.x, top: num("top") * v.scale + v.y, right: (num("left") + num("width")) * v.scale + v.x, bottom: (num("top") + num("height")) * v.scale + v.y };
  };

  beforeEach(() => {
    nodes = MAP_NODES.map((n) => ({ ...n }));
    // jsdom tidak punya tata letak: kanvas 390 x 600 seperti iPhone supaya "pas layar" bermakna.
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, left: 0, top: 0, right: 390, bottom: 600, width: 390, height: 600, toJSON: () => ({}) } as DOMRect);
    fetchMock = mockApi({
      [`${base}/structure`]: () => json(structure(nodes)),
      [`${base}/structure/nodes/n2`]: (_url: URL, init: RequestInit) => {
        const body = JSON.parse(String(init.body)) as { state?: StructureNode["state"] };
        nodes = nodes.map((n) => (n.id === "n2" ? { ...n, state: body.state ?? n.state, state_label: body.state === "accepted" ? "diterima" : n.state_label } : n));
        return json(structure(nodes));
      },
      [`${base}/pack`]: pack,
      [`${base}/financials`]: financials,
    });
  });

  afterEach(() => vi.restoreAllMocks());

  it("opens as a map: rejected nodes stay off the canvas, proposed says USULAN with a dashed card, an owner node says PEMILIK", async () => {
    renderBlock();
    await screen.findByRole("button", { name: cardName("Segmen batubara") });
    expect(card("n4")).toBeNull();
    expect(screen.getByText(/^Ditolak \(1\)/)).toBeInTheDocument();
    expect(viewport().contains(screen.getByText(/^Ditolak \(1\)/))).toBe(false);

    const proposed = card("n2") as HTMLElement;
    expect(proposed).toHaveAttribute("data-state", "proposed");
    expect(proposed.style.borderStyle).toBe("dashed");
    expect(within(proposed).getByText("USULAN")).toBeInTheDocument();
    const accepted = card("n1") as HTMLElement;
    expect(accepted.style.borderStyle).toBe("solid");
    expect(within(accepted).queryByText("USULAN")).toBeNull();
    expect(within(accepted).queryByText("PEMILIK")).toBeNull();
    expect(card("n5")).toBeNull();
    fireEvent.click(chip("Segmen batubara"));
    expect(within(card("n5") as HTMLElement).getByText("PEMILIK")).toBeInTheDocument();
  });

  it("opens with only the company card and the depth-1 branches, at the readable scale, each branch with children carrying a +N chip", async () => {
    renderBlock();
    await screen.findByRole("button", { name: cardName("Segmen batubara") });
    await settle();
    expect(shown()).toEqual(["n1", "n2"]);
    expect(document.querySelector("[data-mindmap-centre]")).not.toBeNull();
    expect(card("n3")).toBeNull();
    expect(card("n5")).toBeNull();
    expect(readView().scale).toBeGreaterThanOrEqual(READABLE_SCALE);
    for (const label of ["Segmen batubara", "Batubara termal ekspor"]) {
      const toggle = chip(label);
      expect(toggle).toHaveTextContent("+1");
      expect(toggle).toHaveAttribute("aria-expanded", "false");
      expect(toggle).toHaveClass("min-h-11", "min-w-11");
    }
    expect(screen.getByRole("button", { name: "Buka semua" })).toHaveClass("min-h-11");
  });

  it("a canvas narrower than 600 px is one-sided: the company card at the left, every depth-1 card to its right and fully on screen without a pan", async () => {
    renderBlock();
    await screen.findByRole("button", { name: cardName("Segmen batubara") });
    await settle();
    expect(readView().scale).toBe(1);
    const c = screenRect(document.querySelector("[data-mindmap-centre]") as HTMLElement);
    expect(c.left).toBeGreaterThanOrEqual(0);
    for (const id of ["n1", "n2"]) {
      const r = screenRect(card(id) as HTMLElement);
      expect(r.left).toBeGreaterThanOrEqual(c.right);
      expect(r.right).toBeLessThanOrEqual(390);
    }
    // The +N chip sits beside the card and is on screen too.
    expect(screenRect(chip("Segmen batubara")).right).toBeLessThanOrEqual(390);
    // Phone canvas height follows the screen (min 420 px, desktop 560 px) and the frame stays clear of the fixed bottom nav
    // and of the "Catat transaksi" FAB that rises 26 px above it (+ 8 px air): 16 + 34 px less height, 34 px more room below,
    // so the legend at the bottom of the map is never under the FAB, whether the map is scrolled to the top or to its end.
    const frame = document.querySelector("[data-mindmap]") as HTMLElement;
    expect(frame).toHaveClass("h-[max(420px,calc(100svh-var(--nav-height)-var(--nav-bottom)-50px))]", "lg:h-[560px]");
    expect(frame.parentElement).toHaveClass("pb-[calc(var(--nav-height)+var(--nav-bottom)+34px)]", "lg:pb-0");
  });

  it("a canvas of 600 px or more is two-sided: depth-1 branches on both sides of the company card", async () => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, left: 0, top: 0, right: 800, bottom: 600, width: 800, height: 600, toJSON: () => ({}) } as DOMRect);
    renderBlock();
    await screen.findByRole("button", { name: cardName("Segmen batubara") });
    await settle();
    const centre = screenRect(document.querySelector("[data-mindmap-centre]") as HTMLElement);
    const mids = ["n1", "n2"].map((id) => {
      const r = screenRect(card(id) as HTMLElement);
      return (r.left + r.right) / 2;
    });
    const centreMid = (centre.left + centre.right) / 2;
    expect(mids.some((m) => m < centreMid)).toBe(true);
    expect(mids.some((m) => m > centreMid)).toBe(true);
  });

  it("the chip opens a branch without opening the sheet, keeps the branch on screen, and closes it again", async () => {
    renderBlock();
    await screen.findByRole("button", { name: cardName("Segmen batubara") });
    await settle();
    fireEvent.click(chip("Batubara termal ekspor"));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(card("n3")).not.toBeNull();
    const opened = chip("Batubara termal ekspor");
    expect(opened).toHaveAttribute("aria-expanded", "true");
    // The name does not flip between Buka and Tutup, and it contains the visible "+1" (WCAG 2.5.3, 4.1.2).
    expect(opened).toHaveAccessibleName("+1 cabang di Batubara termal ekspor");
    // The card is named in visible order (kind, USULAN, label), so a voice user can say any word they see (M-R2-3).
    expect(screen.getByRole("button", { name: cardName("Pembeli di Asia") })).toHaveAccessibleName("Pelanggan USULAN Pembeli di Asia");
    // The view settles (animated, or a jump) with the opened branch fully inside the 390 x 600 canvas and its child
    // starting on screen (the narrow one-sided map opens children further right, the canvas pans for the rest).
    await waitFor(() => {
      for (const id of ["n2", "n3"]) {
        const r = screenRect(card(id) as HTMLElement);
        expect(r.left).toBeGreaterThanOrEqual(0);
        expect(r.left).toBeLessThan(390);
        if (id === "n2") expect(r.right).toBeLessThanOrEqual(390);
        expect(r.top).toBeGreaterThanOrEqual(0);
        expect(r.bottom).toBeLessThanOrEqual(600);
      }
    });
    expect(card("n5")).toBeNull();
    fireEvent.click(opened);
    expect(card("n3")).toBeNull();
    expect(chip("Batubara termal ekspor")).toHaveAttribute("aria-expanded", "false");
  });

  it("Buka semua shows every non-rejected node and turns into Tutup semua, which goes back to the first view", async () => {
    renderBlock();
    await screen.findByRole("button", { name: cardName("Segmen batubara") });
    await settle();
    const first = readView();
    fireEvent.click(screen.getByRole("button", { name: "Buka semua" }));
    expect(shown()).toEqual(["n1", "n2", "n3", "n5"]);
    expect(card("n4")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Tutup semua" }));
    expect(shown()).toEqual(["n1", "n2"]);
    expect(screen.getByRole("button", { name: "Buka semua" })).toBeInTheDocument();
    await waitFor(() => expect(readView().scale).toBeCloseTo(first.scale, 9));
    // Pas layar after closing everything is the same readable first view.
    fireEvent.click(screen.getByRole("button", { name: "Pas layar" }));
    await waitFor(() => expect(readView().scale).toBeGreaterThanOrEqual(READABLE_SCALE));
  });

  it("the colour legend wraps onto as many lines as it needs instead of scrolling or clipping, and names every kind that is on the map", async () => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, left: 0, top: 0, right: 800, bottom: 600, width: 800, height: 600, toJSON: () => ({}) } as DOMRect);
    renderBlock();
    await screen.findByRole("button", { name: cardName("Segmen batubara") });
    const legend = screen.getByRole("list", { name: "Keterangan warna" });
    expect(legend).toHaveClass("flex-wrap");
    for (const clipped of ["overflow-x-auto", "overflow-hidden", "whitespace-nowrap", "lg:flex-wrap"]) expect(legend).not.toHaveClass(clipped);
    expect(legend.className).not.toMatch(/\b(w|min-w|max-w)-\[?\d/);
    // Kinds of closed branches are listed too, so the legend does not grow when a branch opens.
    for (const kind of ["Segmen", "Produk", "Pelanggan", "Kepemilikan", "Garis putus: usulan"]) expect(legend).toHaveTextContent(kind);
    expect(legend).not.toHaveTextContent("Biaya");
  });

  it("shows the centre card with the ticker and the business note, each node's note and a linked metric's latest value on its card", async () => {
    render(
      <MemoryRouter>
        <Struktur ticker="COAL" name="PT Contoh Batubara" note="Menjual batubara termal ke pembeli di Asia." />
      </MemoryRouter>,
    );
    await screen.findByRole("button", { name: cardName("Segmen batubara") });
    const centre = document.querySelector("[data-mindmap-centre]") as HTMLElement;
    expect(centre).toHaveTextContent("COAL");
    expect(centre).toHaveTextContent("PT Contoh Batubara");
    expect(centre).toHaveTextContent("Menjual batubara termal ke pembeli di Asia.");
    const accepted = card("n1") as HTMLElement;
    // 390 px is a narrow canvas: the compact card keeps one line under the label.
    expect(within(accepted).getByText(/Penambangan dan pemasaran batubara termal/)).toHaveClass("line-clamp-1");
    await waitFor(() => expect(card("n2") as HTMLElement).toHaveTextContent(/Produksi 6/));
    // On a phone the legend is closed behind the Warna toggle and opens as an overlay on the canvas.
    expect(screen.queryByRole("list", { name: "Keterangan warna" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Warna" }));
    expect(screen.getByRole("list", { name: "Keterangan warna" })).toHaveTextContent("Segmen");
  });

  it("each card is a focusable button named by its label, and the canvas stops the page from zooming", async () => {
    renderBlock();
    fireEvent.click(await screen.findByRole("button", { name: /^\+1 cabang di Batubara termal ekspor$/ }));
    const button = await screen.findByRole("button", { name: cardName("Pembeli di Asia") });
    expect(button.tagName).toBe("BUTTON");
    expect(button).toHaveAccessibleName("Pelanggan USULAN Pembeli di Asia");
    expect(viewport()).toHaveClass("touch-none");
    expect(viewport()).toHaveAttribute("tabindex", "0");
  });

  it("a card's accessible name covers every visible word of the card (WCAG 2.5.3, axe label-content-name-mismatch)", async () => {
    renderBlock();
    await screen.findByRole("button", { name: cardName("Segmen batubara") });
    await waitFor(() => expect(card("n2") as HTMLElement).toHaveTextContent(/Produksi 6/));
    fireEvent.click(screen.getByRole("button", { name: /^\+\d+ cabang di Segmen batubara$/ }));
    await waitFor(() => expect(card("n5")).not.toBeNull());
    const words = (t: string) => t.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, " ").trim().split(" ");
    const covers = (name: string, visible: string) => {
      const n = words(name);
      const v = words(visible);
      return n.some((_, i) => v.every((w, j) => n[i + j] === w));
    };
    // The note is part of the name on a card that shows it, the linked metric's value on one that shows that instead.
    const names: [string, string | RegExp][] = [
      ["n1", "Segmen Segmen batubara Penambangan dan pemasaran batubara termal di Kalimantan."],
      ["n2", /^Produk USULAN Batubara termal ekspor Produksi 6/],
      ["n5", "Kepemilikan PEMILIK PT Contoh Tbk (pemilik)"],
    ];
    for (const [id, name] of names) {
      const el = card(id) as HTMLElement;
      expect(el).toHaveAccessibleName(name);
      // Every visible word is a contiguous run of the name's words, so the kind never fuses with the label ("SegmenSegmen").
      if (typeof name === "string") expect(covers(name, el.textContent ?? ""), `${id}: "${el.textContent}" vs "${name}"`).toBe(true);
    }
  });

  it("tapping a card opens the sheet with its full note, source and actions; Terima saves and closes it", async () => {
    renderBlock();
    fireEvent.click(await screen.findByRole("button", { name: cardName("Segmen batubara") }));
    const sheet = await screen.findByRole("dialog", { name: "Segmen batubara" });
    expect(within(sheet).getByText("Penambangan dan pemasaran batubara termal di Kalimantan.")).toBeInTheDocument();
    expect(within(sheet).getByRole("button", { name: "Ubah" })).toBeInTheDocument();
    expect(within(sheet).getByRole("button", { name: "Tambah cabang" })).toBeInTheDocument();
    expect(within(sheet).queryByRole("button", { name: "Terima" })).toBeNull();
    fireEvent.click(within(sheet).getByRole("button", { name: "Tutup" }));
    expect(screen.queryByRole("dialog")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: cardName("Batubara termal ekspor") }));
    const proposed = await screen.findByRole("dialog", { name: "Batubara termal ekspor" });
    expect(within(proposed).getByText("USULAN")).toBeInTheDocument();
    expect(within(proposed).getByText("laporan-tahunan-2025.pdf")).toBeInTheDocument();
    expect(within(proposed).getByText(/hal\. 37: “Penjualan batubara termal ke pembeli di Asia”/)).toBeInTheDocument();
    expect(within(proposed).getByRole("button", { name: "Tolak" })).toBeInTheDocument();
    fireEvent.click(within(proposed).getByRole("button", { name: "Terima" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(card("n2")).toHaveAttribute("data-state", "accepted"));
    const put = fetchMock.mock.calls.find(([input, init]) => String(input).endsWith("/structure/nodes/n2") && (init as RequestInit).method === "PUT")!;
    expect(JSON.parse(String((put[1] as RequestInit).body))).toEqual({ state: "accepted" });
  });

  it("a drag that starts on a card pans the map and does not open the sheet", async () => {
    renderBlock();
    const button = await screen.findByRole("button", { name: cardName("Segmen batubara") });
    await settle();
    const before = readView();
    fireEvent.pointerDown(button, { pointerId: 1, pointerType: "touch", clientX: 100, clientY: 100 });
    fireEvent.pointerMove(button, { pointerId: 1, pointerType: "touch", clientX: 140, clientY: 120 });
    fireEvent.pointerUp(button, { pointerId: 1, pointerType: "touch", clientX: 140, clientY: 120 });
    fireEvent.click(button);
    expect(screen.queryByRole("dialog")).toBeNull();
    const after = readView();
    expect(after.scale).toBeCloseTo(before.scale, 9);
    expect(after.x - before.x).toBeCloseTo(40, 6);
    expect(after.y - before.y).toBeCloseTo(20, 6);
    // Ketukan berikutnya (tanpa geser) membuka sheet lagi.
    fireEvent.pointerDown(button, { pointerId: 2, pointerType: "touch", clientX: 100, clientY: 100 });
    fireEvent.pointerUp(button, { pointerId: 2, pointerType: "touch", clientX: 101, clientY: 100 });
    fireEvent.click(button);
    expect(await screen.findByRole("dialog", { name: "Segmen batubara" })).toBeInTheDocument();
  });

  it("starts at the readable scale centred on the company card, and a two-finger pinch zooms around the midpoint of the fingers", async () => {
    renderBlock();
    const area = (await screen.findByRole("button", { name: cardName("Segmen batubara") })).closest("[data-mindmap-viewport]") as HTMLElement;
    await settle();
    const start = readView();
    expect(start.scale).toBeGreaterThanOrEqual(READABLE_SCALE);
    const centre = document.querySelector("[data-mindmap-centre]") as HTMLElement;
    const c = screenRect(centre);
    // 390 px is a narrow canvas: one-sided map, the company card at the left edge, vertically centred.
    expect(c.left).toBeGreaterThanOrEqual(0);
    expect(c.right).toBeLessThan(195);
    expect((c.top + c.bottom) / 2).toBeCloseTo(300, 6);
    const startMid = { x: 150, y: 100 };
    const world = { x: (startMid.x - start.x) / start.scale, y: (startMid.y - start.y) / start.scale };

    fireEvent.pointerDown(area, { pointerId: 1, pointerType: "touch", clientX: 100, clientY: 100 });
    fireEvent.pointerDown(area, { pointerId: 2, pointerType: "touch", clientX: 200, clientY: 100 });
    fireEvent.pointerMove(area, { pointerId: 2, pointerType: "touch", clientX: 260, clientY: 100 });
    const pinched = readView();
    expect(pinched.scale).toBeCloseTo(start.scale * 1.6, 6);
    // Titik peta di bawah titik tengah jari ikut ke titik tengah yang baru (180, 100).
    expect(world.x * pinched.scale + pinched.x).toBeCloseTo(180, 6);
    expect(world.y * pinched.scale + pinched.y).toBeCloseTo(100, 6);
    fireEvent.pointerUp(area, { pointerId: 2, pointerType: "touch", clientX: 260, clientY: 100 });
    fireEvent.pointerUp(area, { pointerId: 1, pointerType: "touch", clientX: 100, clientY: 100 });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  /** WebKit `GestureEvent` tiruan: jsdom tidak punya kelasnya, jadi `Event` biasa dengan `scale` dan koordinat. */
  const gestureEvent = (type: string, scale: number, clientX = 120, clientY = 90) => Object.assign(new Event(type, { bubbles: true, cancelable: true }), { scale, clientX, clientY });

  it("Safari desktop trackpad pinch (gesturestart / gesturechange) zooms around the gesture point and stops the page from zooming (F3)", async () => {
    renderBlock();
    await screen.findByRole("button", { name: cardName("Segmen batubara") });
    await settle();
    const start = readView();
    const anchor = { x: 120, y: 90 };
    const world = { x: (anchor.x - start.x) / start.scale, y: (anchor.y - start.y) / start.scale };

    const began = gestureEvent("gesturestart", 1);
    fireEvent(viewport(), began);
    expect(began.defaultPrevented).toBe(true);
    const changed = gestureEvent("gesturechange", 1.5);
    fireEvent(viewport(), changed);
    expect(changed.defaultPrevented).toBe(true);
    const zoomed = readView();
    expect(zoomed.scale).toBeCloseTo(start.scale * 1.5, 6);
    expect(world.x * zoomed.scale + zoomed.x).toBeCloseTo(anchor.x, 6);
    expect(world.y * zoomed.scale + zoomed.y).toBeCloseTo(anchor.y, 6);

    // `scale` is relative to the start of the gesture, not to the last event: 1.5 again changes nothing.
    fireEvent(viewport(), gestureEvent("gesturechange", 1.5));
    expect(readView().scale).toBeCloseTo(zoomed.scale, 9);
    // Fingers close in: smaller than the zoomed view, clamped to the readable scale (text stays 12 px, chips 44 px).
    fireEvent(viewport(), gestureEvent("gesturechange", 0.0001));
    expect(readView().scale).toBeLessThan(zoomed.scale);
    expect(readView().scale).toBe(READABLE_SCALE);
    const ended = gestureEvent("gestureend", 1);
    fireEvent(viewport(), ended);
    expect(ended.defaultPrevented).toBe(true);
    // After the gesture ends a stray change does nothing.
    const settled = readView();
    fireEvent(viewport(), gestureEvent("gesturechange", 2));
    expect(readView()).toEqual(settled);
  });

  it("a gesture that starts while two fingers are down is left to the pointer pinch, so a touch pinch does not zoom twice (F3)", async () => {
    renderBlock();
    const area = (await screen.findByRole("button", { name: cardName("Segmen batubara") })).closest("[data-mindmap-viewport]") as HTMLElement;
    await settle();
    fireEvent.pointerDown(area, { pointerId: 1, pointerType: "touch", clientX: 100, clientY: 100 });
    fireEvent.pointerDown(area, { pointerId: 2, pointerType: "touch", clientX: 200, clientY: 100 });
    const before = readView();
    fireEvent(viewport(), gestureEvent("gesturestart", 1));
    fireEvent(viewport(), gestureEvent("gesturechange", 2));
    expect(readView()).toEqual(before);
    fireEvent.pointerUp(area, { pointerId: 2, pointerType: "touch", clientX: 200, clientY: 100 });
    fireEvent.pointerUp(area, { pointerId: 1, pointerType: "touch", clientX: 100, clientY: 100 });
  });

  it("the + − and Pas layar controls are 44 px targets that zoom and refit; ctrl + wheel zooms at the pointer, a plain wheel pans", async () => {
    renderBlock();
    await screen.findByRole("button", { name: cardName("Segmen batubara") });
    await settle();
    const fitted = readView();
    for (const name of ["Perbesar", "Perkecil", "Pas layar", "Layar penuh"]) expect(screen.getByRole("button", { name })).toHaveClass("min-h-11", "min-w-11");

    fireEvent.click(screen.getByRole("button", { name: "Perbesar" }));
    expect(readView().scale).toBeCloseTo(fitted.scale * 1.25, 9);
    fireEvent.click(screen.getByRole("button", { name: "Perkecil" }));
    expect(readView().scale).toBeCloseTo(fitted.scale, 9);

    fireEvent.wheel(viewport(), { ctrlKey: true, deltaY: -30, clientX: 120, clientY: 90 });
    const zoomed = readView();
    expect(zoomed.scale).toBeGreaterThan(fitted.scale);
    fireEvent.wheel(viewport(), { deltaX: 10, deltaY: 25 });
    const panned = readView();
    expect(panned.scale).toBeCloseTo(zoomed.scale, 9);
    expect(panned.x).toBeCloseTo(zoomed.x - 10, 6);
    expect(panned.y).toBeCloseTo(zoomed.y - 25, 6);

    fireEvent.click(screen.getByRole("button", { name: "Pas layar" }));
    const refit = readView();
    expect(refit.scale).toBeCloseTo(fitted.scale, 9);
    expect(refit.x).toBeCloseTo(fitted.x, 6);
  });

  it("Layar penuh turns the canvas into a full-screen overlay with a close button; Escape closes it", async () => {
    renderBlock();
    await screen.findByRole("button", { name: cardName("Segmen batubara") });
    const canvas = document.querySelector("[data-mindmap]") as HTMLElement;
    expect(canvas).not.toHaveAttribute("data-fullscreen");
    fireEvent.click(screen.getByRole("button", { name: "Layar penuh" }));
    expect(canvas).toHaveAttribute("data-fullscreen");
    expect(canvas).toHaveClass("fixed", "inset-0");
    expect(screen.getByRole("button", { name: "Tutup" })).toHaveClass("min-h-11", "min-w-11");
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(canvas).not.toHaveAttribute("data-fullscreen"));
    expect(screen.getByRole("button", { name: "Layar penuh" })).toBeInTheDocument();
  });

  it("Layar penuh is a modal dialog: named, focus moves in, Tab stays inside, the page behind is inert, and focus returns to the opener (H2)", async () => {
    renderBlock();
    await screen.findByRole("button", { name: cardName("Segmen batubara") });
    const opener = screen.getByRole("button", { name: "Layar penuh" });
    opener.focus();
    fireEvent.click(opener);
    const overlay = screen.getByRole("dialog", { name: "Peta struktur bisnis, layar penuh" });
    expect(overlay).toHaveAttribute("aria-modal", "true");
    expect(overlay).toBe(document.querySelector("[data-mindmap]"));
    expect(overlay).toHaveFocus();
    expect(document.body.style.overflow).toBe("hidden");

    // Everything outside the overlay (the rest of the page, the nav) is inert; the overlay and its ancestors are not.
    expect(overlay.closest("[inert]")).toBeNull();
    expect(screen.getByText(/^Ditolak \(1\)/).closest("[inert]")).not.toBeNull();
    // The Struktur block's own toolbar sits behind the overlay: it is inert too.
    expect(screen.getByRole("button", { name: "Tambah cabang" }).closest("[inert]")).not.toBeNull();

    // Tab wraps at both ends of the overlay instead of walking onto the hidden page.
    const inside = [...overlay.querySelectorAll<HTMLElement>("button, [tabindex='0']")].filter((el) => !el.closest("[hidden]"));
    inside[inside.length - 1].focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(inside[0]).toHaveFocus();
    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(inside[inside.length - 1]).toHaveFocus();
    screen.getByRole("button", { name: "Tambah cabang" }).focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(overlay.contains(document.activeElement)).toBe(true);

    fireEvent.click(within(overlay).getByRole("button", { name: "Tutup" }));
    await waitFor(() => expect(overlay).not.toHaveAttribute("data-fullscreen"));
    expect(overlay).not.toHaveAttribute("role");
    expect(document.querySelector("[inert]")).toBeNull();
    expect(document.body.style.overflow).toBe("");
    expect(opener).toHaveFocus();
  });

  it("Escape closes the top-most dialog only: a node sheet opened from Layar penuh closes first, then the full screen (N4)", async () => {
    renderBlock();
    fireEvent.click(await screen.findByRole("button", { name: "Layar penuh" }));
    const overlay = screen.getByRole("dialog", { name: "Peta struktur bisnis, layar penuh" });
    fireEvent.click(within(overlay).getByRole("button", { name: cardName("Segmen batubara") }));
    const sheet = await screen.findByRole("dialog", { name: "Segmen batubara" });
    // The sheet is on top: the overlay is now part of the inert background, and Tab is contained by the sheet.
    expect(overlay.closest("[inert]")).not.toBeNull();
    expect(sheet.closest("[inert]")).toBeNull();

    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Segmen batubara" })).toBeNull());
    expect(overlay).toHaveAttribute("data-fullscreen");
    // Back to the overlay as the only modal: it is interactive again.
    expect(overlay.closest("[inert]")).toBeNull();
    expect(document.body.style.overflow).toBe("hidden");

    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(overlay).not.toHaveAttribute("data-fullscreen"));
    expect(document.querySelector("[inert]")).toBeNull();
    expect(document.body.style.overflow).toBe("");
  });

  it("one polite status line announces expand, collapse, Buka semua, zoom and Pas layar (M5)", async () => {
    renderBlock();
    await screen.findByRole("button", { name: cardName("Segmen batubara") });
    await settle();
    // Outside full screen the announcements go to the Struktur block's own region, which stays mounted when the map is swapped for the list (M-R2-4).
    const status = document.querySelector("[data-struktur-status]") as HTMLElement;
    expect(document.querySelector("[data-mindmap]")!.contains(status)).toBe(false);
    expect(status).toHaveAttribute("role", "status");
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(status.textContent).toBe("");

    fireEvent.click(chip("Segmen batubara"));
    expect(status).toHaveTextContent("3 kartu tampil");
    fireEvent.click(chip("Segmen batubara"));
    expect(status).toHaveTextContent("2 kartu tampil");
    fireEvent.click(screen.getByRole("button", { name: "Buka semua" }));
    expect(status).toHaveTextContent("4 kartu tampil");
    fireEvent.click(screen.getByRole("button", { name: "Tutup semua" }));
    expect(status).toHaveTextContent("2 kartu tampil");

    fireEvent.click(screen.getByRole("button", { name: "Perbesar" }));
    expect(status).toHaveTextContent(/^Zoom \d+%/);
    const zoomed = status.textContent;
    fireEvent.click(screen.getByRole("button", { name: "Pas layar" }));
    expect(status).toHaveTextContent("Pas layar");
    expect(zoomed).not.toBe(status.textContent);
    // The same message twice in a row still changes the text node, so a screen reader says it again.
    const first = status.textContent;
    fireEvent.click(screen.getByRole("button", { name: "Pas layar" }));
    expect(status.textContent).not.toBe(first);
    expect(status.textContent?.trim()).toBe("Pas layar");
  });

  it("switching between the map and the list is announced both ways by a status line that stays mounted (M-R2-4, WCAG 4.1.3)", async () => {
    renderBlock();
    await screen.findByRole("button", { name: cardName("Segmen batubara") });
    const status = document.querySelector("[data-struktur-status]") as HTMLElement;
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(status.textContent).toBe("");

    fireEvent.click(screen.getByRole("button", { name: "Lihat sebagai daftar" }));
    expect(document.querySelector("[data-mindmap]")).toBeNull();
    expect(document.querySelector("[data-struktur-status]")).toBe(status);
    expect(status).toHaveTextContent("Daftar struktur ditampilkan");

    fireEvent.click(screen.getByRole("button", { name: "Lihat sebagai peta" }));
    expect(document.querySelector("[data-mindmap]")).not.toBeNull();
    expect(document.querySelector("[data-struktur-status]")).toBe(status);
    expect(status).toHaveTextContent("Peta struktur ditampilkan");

    // The map's own announcements still arrive through the same region.
    await settle();
    fireEvent.click(chip("Segmen batubara"));
    expect(status).toHaveTextContent("3 kartu tampil");
  });

  it("in full screen the page behind is inert, so the map announces inside the overlay instead (M5)", async () => {
    renderBlock();
    await screen.findByRole("button", { name: cardName("Segmen batubara") });
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Layar penuh" }));
    const overlay = screen.getByRole("dialog", { name: "Peta struktur bisnis, layar penuh" });
    const inside = overlay.querySelector("[role='status']") as HTMLElement;
    fireEvent.click(within(overlay).getByRole("button", { name: "Perbesar" }));
    expect(inside).toHaveTextContent(/^Zoom \d+%/);
    expect(document.querySelector("[data-struktur-status]")!.textContent).toBe("");
  });

  it("the list view's branch toggle has one name that does not flip, with the state in aria-expanded (M3)", async () => {
    await renderList();
    const toggle = await screen.findByRole("button", { name: "1 cabang di Segmen batubara" });
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    fireEvent.click(toggle);
    expect(screen.getByRole("button", { name: "1 cabang di Segmen batubara" })).toBe(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("PT Contoh Tbk (pemilik)")).toBeNull();
  });

  it("a decision that fails in full screen shows its error inside the overlay, not under it", async () => {
    mockApi({
      [`${base}/structure`]: () => json(structure(nodes)),
      [`${base}/structure/nodes/n2`]: () => json({ detail: "Keputusan ditolak server" }, 409),
      [`${base}/pack`]: pack,
      [`${base}/financials`]: financials,
    });
    renderBlock();
    fireEvent.click(await screen.findByRole("button", { name: "Layar penuh" }));
    const overlay = screen.getByRole("dialog", { name: "Peta struktur bisnis, layar penuh" });
    fireEvent.click(within(overlay).getByRole("button", { name: cardName("Batubara termal ekspor") }));
    const sheet = await screen.findByRole("dialog", { name: "Batubara termal ekspor" });
    fireEvent.click(within(sheet).getByRole("button", { name: "Tolak" }));
    expect(await within(overlay).findByRole("alert")).toHaveTextContent("Keputusan ditolak server");
  });

  it("Lihat sebagai daftar shows the old list with its inline actions, and Lihat sebagai peta brings the canvas back", async () => {
    renderBlock();
    fireEvent.click(await screen.findByRole("button", { name: "Lihat sebagai daftar" }));
    expect(document.querySelector("[data-mindmap]")).toBeNull();
    const item = screen.getByText("Batubara termal ekspor").closest("[data-node]") as HTMLElement;
    expect(item).toHaveClass("border-dashed");
    expect(within(item).getByRole("button", { name: "Terima Batubara termal ekspor" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Lihat sebagai peta" }));
    expect(document.querySelector("[data-mindmap]")).not.toBeNull();
  });
});

describe("structure mind map on the real BUMI structure", () => {
  const BUMI: StructureNode[] = draft.nodes.map((n) =>
    node({ id: n.id, parent: n.parent, label: n.label, kind: n.kind as StructureNode["kind"], kind_label: n.kind, note: n.note ?? null, state: "proposed", state_label: "usulan" }),
  );

  beforeEach(() => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, left: 0, top: 0, right: 393, bottom: 520, width: 393, height: 520, toJSON: () => ({}) } as DOMRect);
    mockApi({ [`${base}/structure`]: () => json(structure(BUMI)), [`${base}/pack`]: pack, [`${base}/financials`]: financials });
  });

  afterEach(() => vi.restoreAllMocks());

  const ids = () => [...document.querySelectorAll("[data-mindmap-viewport] [data-node]")].map((el) => el.getAttribute("data-node") as string).sort();

  it("first shows the six depth-1 branches at a readable scale, then Buka semua draws all 40 nodes", async () => {
    renderBlock();
    await screen.findByRole("button", { name: "Buka semua" });
    const roots = BUMI.filter((n) => n.parent === null).map((n) => n.id);
    expect(roots).toHaveLength(6);
    expect(ids()).toEqual([...roots].sort());
    await waitFor(() => expect(Number(/scale\(([\d.]+)\)/.exec((document.querySelector("[data-mindmap-layer]") as HTMLElement).style.transform)?.[1])).toBeGreaterThanOrEqual(READABLE_SCALE));
    fireEvent.click(screen.getByRole("button", { name: "Buka semua" }));
    expect(ids()).toHaveLength(40);
    expect(new Set(ids()).size).toBe(40);
  });
});

describe("structure mind map on a 361 px phone canvas (G4)", () => {
  const BUMI: StructureNode[] = draft.nodes.map((n) =>
    node({ id: n.id, parent: n.parent, label: n.label, kind: n.kind as StructureNode["kind"], kind_label: n.kind, note: n.note ?? null, state: "proposed", state_label: "usulan" }),
  );
  const WIDTH = 361;

  const canvas = (height: number) =>
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, left: 0, top: 0, right: WIDTH, bottom: height, width: WIDTH, height, toJSON: () => ({}) } as DOMRect);
  const layer = () => document.querySelector("[data-mindmap-layer]") as HTMLElement;
  const readView = () => {
    const m = /translate\((-?[\d.]+)px, (-?[\d.]+)px\) scale\(([\d.]+)\)/.exec(layer().style.transform);
    if (!m) throw new Error(`no view in "${layer().style.transform}"`);
    return { x: Number(m[1]), y: Number(m[2]), scale: Number(m[3]) };
  };
  const screenRect = (el: HTMLElement) => {
    const v = readView();
    const num = (p: string) => Number.parseFloat(el.style.getPropertyValue(p));
    return { left: num("left") * v.scale + v.x, top: num("top") * v.scale + v.y, right: (num("left") + num("width")) * v.scale + v.x, bottom: (num("top") + num("height")) * v.scale + v.y };
  };
  const roots = BUMI.filter((n) => n.parent === null).map((n) => n.id);
  const card = (id: string) => document.querySelector(`[data-mindmap-viewport] [data-node="${id}"]`) as HTMLElement;
  const ready = async () => {
    renderBlock();
    await screen.findByRole("button", { name: "Buka semua" });
    await waitFor(() => expect(document.querySelector("[data-mindmap-layer]")).not.toBeNull());
  };

  beforeEach(() => {
    canvas(540);
    mockApi({ [`${base}/structure`]: () => json(structure(BUMI)), [`${base}/pack`]: pack, [`${base}/financials`]: financials });
  });

  afterEach(() => vi.restoreAllMocks());

  it("the toolbar is one row of 44 px buttons: icon buttons with a name and a tooltip, one text button for Buka semua, no wrap class", async () => {
    await ready();
    const bar = screen.getByRole("button", { name: "Perbesar" }).parentElement as HTMLElement;
    expect(bar).not.toHaveClass("flex-wrap");
    const buttons = [...bar.querySelectorAll("button")];
    expect(buttons.map((b) => b.getAttribute("aria-label") ?? b.textContent)).toEqual(["Perbesar", "Perkecil", "Pas layar", "Buka semua", "Warna", "Layar penuh"]);
    for (const b of buttons) expect(b).toHaveClass("min-h-11", "min-w-11");
    for (const name of ["Perbesar", "Perkecil", "Pas layar", "Warna", "Layar penuh"]) {
      const b = screen.getByRole("button", { name });
      expect(b).toHaveAttribute("title", name);
      expect(b.textContent).toBe("");
    }
  });

  it("the first view shows the company card and all six depth-1 cards inside the canvas at scale 1, starting at the top of the stack", async () => {
    await ready();
    await waitFor(() => expect(readView().scale).toBe(1));
    expect(roots).toHaveLength(6);
    const centre = screenRect(document.querySelector("[data-mindmap-centre]") as HTMLElement);
    for (const r of [centre, ...roots.map((id) => screenRect(card(id)))]) {
      expect(r.left).toBeGreaterThanOrEqual(0);
      expect(r.top).toBeGreaterThanOrEqual(0);
      expect(r.right).toBeLessThanOrEqual(WIDTH);
      expect(r.bottom).toBeLessThanOrEqual(540);
    }
    for (const id of roots) expect(screenRect(card(id)).bottom - screenRect(card(id)).top).toBe(80);
  });

  it("the colour legend is closed behind the Warna toggle and opens as an overlay without shrinking the canvas", async () => {
    await ready();
    expect(screen.queryByRole("list", { name: "Keterangan warna" })).toBeNull();
    const toggle = screen.getByRole("button", { name: "Warna" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    const legend = screen.getByRole("list", { name: "Keterangan warna" });
    expect(legend).toHaveClass("absolute", "bottom-0");
    fireEvent.click(toggle);
    expect(screen.queryByRole("list", { name: "Keterangan warna" })).toBeNull();
  });

  it("pointer focus on a partly visible card does not change the view, keyboard focus does (M1)", async () => {
    canvas(300);
    await ready();
    await waitFor(() => expect(readView().scale).toBe(1));
    const low = card(roots[roots.length - 1]);
    expect(screenRect(low).bottom).toBeGreaterThan(300);
    const before = readView();

    // Chrome focuses a button on tap: the pointer goes down first, then focus arrives.
    fireEvent.pointerDown(low, { pointerId: 1, pointerType: "touch", clientX: 100, clientY: 280 });
    fireEvent.focus(low);
    fireEvent.pointerUp(low, { pointerId: 1, pointerType: "touch", clientX: 100, clientY: 280 });
    expect(readView()).toEqual(before);
    fireEvent.click(low);
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Tutup" }));

    // Tab (keyboard) then focus: the card is brought into view.
    fireEvent.keyDown(window, { key: "Tab" });
    fireEvent.focus(low);
    const after = readView();
    expect(after.y).toBeLessThan(before.y);
    expect(screenRect(low).bottom).toBeLessThanOrEqual(300);
  });

  /** Every card and +N chip drawn on the canvas, as on-screen rectangles. */
  const drawn = () => [...document.querySelectorAll<HTMLElement>("[data-mindmap-layer] [data-mm-node], [data-mindmap-layer] [data-mm-toggle], [data-mindmap-centre]")].map((el) => ({ id: el.getAttribute("data-node") ?? el.getAttribute("data-mm-toggle") ?? "centre", ...screenRect(el) }));
  const chipOf = (n: StructureNode) => screen.getByRole("button", { name: new RegExp(`^\\+\\d+ cabang di ${n.label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`) });

  it("in every expand state no card or chip is cut by the left or right canvas edge, at scale 1 (bug 1: n1 open cut 8 cards)", async () => {
    await ready();
    await waitFor(() => expect(readView().scale).toBe(1));
    const parents = BUMI.filter((n) => BUMI.some((c) => c.parent === n.id) && n.parent === null);
    expect(parents.length).toBeGreaterThan(0);
    for (const p of parents) {
      fireEvent.click(chipOf(p));
      await waitFor(() => expect(card(BUMI.find((c) => c.parent === p.id)!.id)).not.toBeNull());
      expect(readView().scale).toBe(1);
      for (const r of drawn()) {
        expect(r.left, `${p.id} open: ${r.id}`).toBeGreaterThanOrEqual(0);
        expect(r.right, `${p.id} open: ${r.id}`).toBeLessThanOrEqual(WIDTH);
      }
      fireEvent.click(chipOf(p));
    }
    fireEvent.click(screen.getByRole("button", { name: "Buka semua" }));
    await waitFor(() => expect(document.querySelectorAll("[data-mindmap-layer] [data-mm-node]")).toHaveLength(40));
    for (const r of drawn()) {
      expect(r.left).toBeGreaterThanOrEqual(0);
      expect(r.right).toBeLessThanOrEqual(WIDTH);
    }
  });

  it("opening a branch puts its card at the top of the canvas with its first children below it, not the card in the middle", async () => {
    await ready();
    await waitFor(() => expect(readView().scale).toBe(1));
    const parent = BUMI.filter((n) => n.parent === null).reduce((a, b) => (BUMI.filter((c) => c.parent === b.id).length > BUMI.filter((c) => c.parent === a.id).length ? b : a));
    fireEvent.click(chipOf(parent));
    const kids = BUMI.filter((c) => c.parent === parent.id);
    await waitFor(() => expect(screenRect(card(parent.id)).top).toBeCloseTo(12, 6));
    expect(screenRect(card(kids[0].id)).top).toBeGreaterThan(screenRect(card(parent.id)).bottom);
    expect(screenRect(card(kids[0].id)).bottom).toBeLessThanOrEqual(540);
    // The chip of an open branch points back up to the parent; a closed one points down to where the children appear.
    expect(chipOf(parent).querySelector("svg")?.getAttribute("class")).toMatch(/chevron-up/);
    const nested = kids.find((k) => BUMI.some((c) => c.parent === k.id)) as StructureNode;
    expect(chipOf(nested).querySelector("svg")?.getAttribute("class")).toMatch(/chevron-down/);
  });

  it("Buka semua frames the start of the tree: the company card and the first branch with its first children, at scale 1 (bug 4)", async () => {
    await ready();
    fireEvent.click(screen.getByRole("button", { name: "Buka semua" }));
    await waitFor(() => expect(readView().y).toBe(0));
    expect(readView().scale).toBe(1);
    const first = BUMI.find((n) => n.parent === null && BUMI.some((c) => c.parent === n.id)) as StructureNode;
    expect(first.id).toBe(roots[0]);
    const firstChild = BUMI.find((c) => c.parent === first.id) as StructureNode;
    for (const r of [screenRect(document.querySelector("[data-mindmap-centre]") as HTMLElement), screenRect(card(first.id)), screenRect(card(firstChild.id))]) {
      expect(r.left).toBeGreaterThanOrEqual(0);
      expect(r.top).toBeGreaterThanOrEqual(0);
      expect(r.right).toBeLessThanOrEqual(WIDTH);
      expect(r.bottom).toBeLessThanOrEqual(540);
    }
  });

  it("Pas layar and Perkecil never go below the readable scale; when the open map is taller, Pas layar fits the width, starts at the top and says the rest is panned (bug 2)", async () => {
    await ready();
    fireEvent.click(screen.getByRole("button", { name: "Buka semua" }));
    fireEvent.click(screen.getByRole("button", { name: "Perbesar" }));
    fireEvent.click(screen.getByRole("button", { name: "Pas layar" }));
    await waitFor(() => expect(readView().y).toBe(0));
    expect(readView()).toEqual({ scale: 1, x: 0, y: 0 });
    expect(document.querySelector("[data-struktur-status]")).toHaveTextContent("Pas layar, geser untuk melihat sisanya");
    const minus = screen.getByRole("button", { name: "Perkecil" });
    expect(minus).toHaveAttribute("aria-disabled", "true");
    expect(minus).toHaveClass("min-h-11", "min-w-11");
    fireEvent.click(minus);
    expect(readView().scale).toBe(1);
    fireEvent.click(screen.getByRole("button", { name: "Perbesar" }));
    expect(minus).not.toHaveAttribute("aria-disabled");
  });

  it("the node sheet title wraps instead of truncating, so a long label is readable in full (M3)", async () => {
    await ready();
    const longest = BUMI.filter((n) => n.parent === null).reduce((a, b) => (b.label.length > a.label.length ? b : a));
    fireEvent.click(card(longest.id));
    const dialog = await screen.findByRole("dialog");
    const title = within(dialog).getByRole("heading", { level: 2 });
    expect(title).toHaveTextContent(longest.label.slice(0, 20));
    expect(title).toHaveClass("break-words");
    expect(title).not.toHaveClass("truncate");
  });

  it("the cards of an opened branch sit below it, one indent in, with the same right edge (narrower than the depth-1 card)", async () => {
    await ready();
    const parent = BUMI.find((n) => n.parent === null && BUMI.some((c) => c.parent === n.id)) as StructureNode;
    fireEvent.click(chipOf(parent));
    const child = BUMI.find((n) => n.parent === parent.id) as StructureNode;
    const num = (el: HTMLElement, p: string) => Number.parseFloat(el.style.getPropertyValue(p));
    expect(num(card(child.id), "left")).toBe(num(card(parent.id), "left") + 12);
    expect(num(card(child.id), "left") + num(card(child.id), "width")).toBe(num(card(parent.id), "left") + num(card(parent.id), "width"));
    expect(num(card(child.id), "top")).toBeGreaterThan(num(card(parent.id), "top"));
  });
});

describe("structureData", () => {
  it("builds the tree top-down, drops rejected nodes, and lifts nodes whose parent is gone", () => {
    const orphan = node({ id: "n5", parent: "n4", label: "Anak dari yang ditolak" });
    const { roots, rejected } = buildTree([...NODES, orphan]);
    expect(rejected.map((n) => n.id)).toEqual(["n4"]);
    expect(roots.map((t) => t.node.id)).toEqual(["n1", "n5"]);
    expect(roots[0].children[0].children[0]).toMatchObject({ depth: 3 });
    expect(roots[1].depth).toBe(1);
  });

  it("references pages for PDFs and slides for presentations", () => {
    expect(pageRef(PROPOSED)).toBe("hal. 37");
    expect(pageRef({ ...PROPOSED, source: { document: "paparan.pptx", page: 12, snippet: "x" } })).toBe("slide 12");
    expect(pageRef(node({ id: "x", label: "x" }))).toBeNull();
  });
});
