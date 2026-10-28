import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { json, mockApi } from "../../../test/mockApi";
import { golden } from "../testing/canon";
import type { JournalEntry as JournalEntryData, JournalResponse } from "../types";
import { JournalEntry, JournalPanel, formatTimestamp } from "./JournalEntry";
import { JournalQuickAdd, OFFLINE_MESSAGE } from "./JournalQuickAdd";

const JOURNAL = golden<JournalResponse>("journal-COAL");

function capturePosts(path: string, answer: () => Response | Promise<Response> = () => json({ id: "j-new" }, 201)) {
  const posts: Array<{ body: Record<string, unknown>; key: string | null }> = [];
  const fetchMock = mockApi({
    [path]: (_u: URL, init: RequestInit) => {
      posts.push({ body: JSON.parse(String(init.body)), key: new Headers(init.headers).get("Idempotency-Key") });
      return answer();
    },
  });
  return { posts, fetchMock };
}

describe("JournalQuickAdd (design.md §3.5, §6)", () => {
  it("shows the current close as the price stamp placeholder, also on the level price", () => {
    render(<JournalQuickAdd ticker="COAL" close={1290} />);
    expect(screen.getByLabelText("Isi catatan")).toHaveAttribute("placeholder", "Tulis catatan · harga kini 1.290");
    expect(screen.getByText("Enter menyimpan · harga 1.290 dicatat otomatis")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "Level" }));
    expect(screen.getByLabelText("Harga level")).toHaveAttribute("placeholder", "1.290");
  });

  it("Enter saves through POST /{ticker}/journal and clears the line; Shift+Enter does not save", async () => {
    const { posts } = capturePosts("/api/research/COAL/journal");
    render(<JournalQuickAdd ticker="COAL" close={1290} />);
    const input = screen.getByLabelText("Isi catatan");
    fireEvent.change(input, { target: { value: "Manajemen menaikkan target" } });
    fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
    expect(posts).toHaveLength(0);
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(posts).toHaveLength(1));
    expect(posts[0].body).toEqual({ kind: "catatan", text: "Manajemen menaikkan target", level: null });
    expect(posts[0].key).toBeTruthy();
    await waitFor(() => expect(input).toHaveValue(""));
    expect(screen.getByRole("status")).toHaveTextContent("Tersimpan");
  });

  it("text typed while a save is in flight is kept; only the saved text is cleared", async () => {
    let release: (r: Response) => void = () => {};
    const { posts } = capturePosts("/api/research/COAL/journal", () => new Promise<Response>((r) => (release = r)));
    render(<JournalQuickAdd ticker="COAL" close={1290} />);
    const input = screen.getByLabelText("Isi catatan");
    fireEvent.change(input, { target: { value: "Catatan pertama" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(posts).toHaveLength(1));
    fireEvent.change(input, { target: { value: "Catatan kedua yang sedang diketik" } });
    release(json({ id: "j-new" }, 201));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Tersimpan"));
    expect(input).toHaveValue("Catatan kedua yang sedang diketik");
  });

  it("a level entry needs a price above zero and sends it", async () => {
    const { posts } = capturePosts("/api/research/COAL/journal");
    render(<JournalQuickAdd ticker="COAL" close={1290} />);
    fireEvent.click(screen.getByRole("radio", { name: "Level" }));
    const input = screen.getByLabelText("Isi catatan");
    fireEvent.change(input, { target: { value: "Harga incaran" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(screen.getByText("Isi harga level di atas nol")).toBeInTheDocument();
    expect(posts).toHaveLength(0);
    fireEvent.change(screen.getByLabelText("Harga level"), { target: { value: "1.300" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(posts).toHaveLength(1));
    expect(posts[0].body).toEqual({ kind: "level", text: "Harga incaran", level: 1300 });
  });

  it("offline: the design string, the draft kept, and the same key on retry", async () => {
    let fail = true;
    const { posts } = capturePosts("/api/research/COAL/journal", () => (fail ? Promise.reject(new TypeError("offline")) : json({ id: "j" }, 201)));
    render(<JournalQuickAdd ticker="COAL" close={null} />);
    const input = screen.getByLabelText("Isi catatan");
    expect(input).toHaveAttribute("placeholder", "Tulis catatan");
    fireEvent.change(input, { target: { value: "Catatan penting" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(await screen.findByRole("alert")).toHaveTextContent(OFFLINE_MESSAGE);
    expect(input).toHaveValue("Catatan penting");
    fail = false;
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(posts).toHaveLength(2));
    expect(posts[1].key).toBe(posts[0].key);
  });

  it("a 422 on text is shown under the line", async () => {
    capturePosts("/api/research/COAL/journal", () => json({ detail: [{ type: "value_error", loc: ["body", "text"], msg: "teks terlalu panjang", input: null }] }, 422));
    render(<JournalQuickAdd ticker="COAL" close={1290} />);
    const input = screen.getByLabelText("Isi catatan");
    fireEvent.change(input, { target: { value: "x" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(await screen.findByText("teks terlalu panjang")).toBeInTheDocument();
    expect(input).toHaveAttribute("aria-invalid", "true");
  });

  it("a focus request moves focus to the line", () => {
    const { rerender } = render(<JournalQuickAdd ticker="COAL" close={1290} focusRequest={0} />);
    expect(document.activeElement).not.toBe(screen.getByLabelText("Isi catatan"));
    rerender(<JournalQuickAdd ticker="COAL" close={1290} focusRequest={1} />);
    expect(document.activeElement).toBe(screen.getByLabelText("Isi catatan"));
  });
});

describe("JournalEntry immutability (design.md §7.10)", () => {
  it("renders time, kind tag, price at time, level and indented annotations, with no edit or delete control", () => {
    const level = JOURNAL.items.find((e) => e.kind === "level")!;
    render(<JournalEntry entry={level} />);
    const article = screen.getByRole("article");
    expect(within(article).getByText("level")).toBeInTheDocument();
    expect(within(article).getByText(`${formatTimestamp(level.at)} · harga 1.277 · level 1.300`)).toBeInTheDocument();
    expect(within(within(article).getByRole("list", { name: "Anotasi" })).getByText("Level digeser setelah laporan kuartal 2")).toBeInTheDocument();
    const buttons = within(article).getAllByRole("button").map((b) => b.textContent);
    expect(buttons).toEqual(["Tambah anotasi"]);
    expect(within(article).queryByRole("textbox")).toBeNull();
  });

  it("sistem entries use text-2", () => {
    const system: JournalEntryData = { ...JOURNAL.items[0], id: "j-sys", kind: "sistem", text: "Aturan diperbarui" };
    render(<JournalEntry entry={system} />);
    expect(screen.getByText("Aturan diperbarui")).toHaveClass("text-text-2");
  });

  it("an annotation is added below, never an edit of the entry", async () => {
    const { posts } = capturePosts(`/api/research/journal/${JOURNAL.items[0].id}/annotations`);
    render(<JournalEntry entry={JOURNAL.items[0]} />);
    fireEvent.click(screen.getByRole("button", { name: "Tambah anotasi" }));
    fireEvent.change(screen.getByLabelText("Isi anotasi"), { target: { value: "Ternyata ditunda" } });
    fireEvent.click(screen.getByRole("button", { name: "Simpan" }));
    await waitFor(() => expect(posts).toHaveLength(1));
    expect(posts[0].body).toEqual({ text: "Ternyata ditunda" });
  });

  it("formats timestamps on the WIB day and clock", () => {
    expect(formatTimestamp("2026-10-24T18:30:00Z")).toBe("25 Okt 2026 01.30");
  });
});

describe("JournalPanel", () => {
  it("after a save the list returns to the first page and shows the new entry", async () => {
    let saved = false;
    const fresh = { ...JOURNAL.items[0], id: "j-new", text: "Catatan yang baru disimpan" };
    mockApi({
      "/api/research/COAL/journal?limit=20&offset=0": () => json(saved ? { ...JOURNAL, total: 26, items: [fresh, ...JOURNAL.items] } : { ...JOURNAL, total: 25 }),
      "/api/research/COAL/journal?limit=20&offset=20": () => json({ ...JOURNAL, total: saved ? 26 : 25, offset: 20, items: JOURNAL.items.slice(0, 1) }),
      "/api/research/COAL/journal": () => {
        saved = true;
        return json({ id: "j-new" }, 201);
      },
    });
    render(<JournalPanel ticker="COAL" close={1290} />);
    await screen.findByText("1–20 dari 25");
    fireEvent.click(screen.getByRole("button", { name: "Lebih lama" }));
    await screen.findByText("21–25 dari 25");
    const input = screen.getByLabelText("Isi catatan");
    fireEvent.change(input, { target: { value: "Catatan yang baru disimpan" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(await screen.findByText("1–20 dari 26")).toBeInTheDocument();
    const list = screen.getByRole("region", { name: "Entri jurnal" });
    expect(within(list).getAllByRole("article")[0]).toHaveTextContent("Catatan yang baru disimpan");
  });

  it("lists entries newest first from the golden and pages by 20", async () => {
    const page2 = { ...JOURNAL, total: 25, offset: 20, items: JOURNAL.items.slice(0, 1) };
    mockApi({
      "/api/research/COAL/journal?limit=20&offset=0": { ...JOURNAL, total: 25 },
      "/api/research/COAL/journal?limit=20&offset=20": page2,
    });
    render(<JournalPanel ticker="COAL" close={1290} />);
    const list = await screen.findByRole("region", { name: "Entri jurnal" });
    const articles = within(list).getAllByRole("article");
    expect(articles).toHaveLength(JOURNAL.items.length);
    // The backend orders newest first; the list keeps that order.
    const times = JOURNAL.items.map((e) => e.at);
    expect([...times].sort().reverse()).toEqual(times);
    JOURNAL.items.forEach((e, i) => expect(articles[i]).toHaveTextContent(e.text));
    expect(screen.getByText("1–20 dari 25")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Lebih lama" }));
    expect(await screen.findByText("21–25 dari 25")).toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Entri jurnal" })).getAllByRole("article")).toHaveLength(1);
  });

  it("empty and error states", async () => {
    mockApi({ "/api/research/COAL/journal": { ...JOURNAL, items: [], total: 0 } });
    const { unmount } = render(<JournalPanel ticker="COAL" close={1290} />);
    expect(await screen.findByText("Belum ada catatan")).toBeInTheDocument();
    unmount();
    mockApi({ "/api/research/COAL/journal": () => json({ detail: "rusak" }, 500) });
    render(<JournalPanel ticker="COAL" close={1290} />);
    expect(await screen.findByText("rusak")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Coba lagi" })).toBeInTheDocument();
  });
});
