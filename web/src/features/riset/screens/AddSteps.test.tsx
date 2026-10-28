import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import packs from "../../../../../tests/fixtures/research/canon/packs.json";
import { json, mockApi } from "../../../test/mockApi";
import { expectNoVerdictWords } from "../../research/screenAsserts.testutil";
import type { LookupItem, Role } from "../../research/types";
import { Add } from "./Add";

function Where() {
  const l = useLocation();
  return <output data-testid="where">{`${l.pathname}${l.search}`}</output>;
}

function renderAdd() {
  return render(
    <MemoryRouter initialEntries={["/perusahaan/tambah"]}>
      <Routes>
        <Route path="/perusahaan/tambah" element={<Add />} />
        <Route path="/perusahaan/:ticker" element={<Where />} />
      </Routes>
    </MemoryRouter>,
  );
}

const COAL_PACK = { id: "coal_mining", label: "Tambang batubara" };
/** Bursa sintetis: kode, nama, kode industri, paket usulan, dan peran di universe. */
const LISTING: Array<{ ticker: string; name: string; code: string | null; pack: typeof COAL_PACK | null; role?: Role }> = [
  { ticker: "AAAA", name: "Aneka Contoh", code: "A121", pack: COAL_PACK },
  { ticker: "BBBB", name: "Batu Contoh", code: null, pack: null },
  { ticker: "CCCC", name: "Cipta Contoh", code: "A121", pack: COAL_PACK },
  { ticker: "DDDD", name: "Dunia Contoh", code: null, pack: null },
  { ticker: "EEEE", name: "Energi Contoh", code: null, pack: null },
  { ticker: "HELD", name: "Dipegang Contoh", code: "A121", pack: COAL_PACK, role: "held" },
  { ticker: "MANU", name: "Manual Contoh", code: null, pack: null, role: "manual" },
  { ticker: "PEER", name: "Pembanding Contoh", code: null, pack: null, role: "peer" },
];

const toItem = (l: (typeof LISTING)[number]): LookupItem => ({
  ticker: l.ticker,
  name: l.name,
  listed: true,
  industry_code: l.code,
  pack: l.pack,
  in_universe: l.role !== undefined,
  role: l.role ?? null,
});

interface Call {
  method: string;
  path: string;
  body: Record<string, unknown> | null;
  key: string | null;
}
let calls: Call[];
let listAvailable: boolean;
let failPeer: string | null;
let deleteStatus: number;

function backend() {
  const record = (u: URL, init: RequestInit): Call => {
    const headers = (init.headers ?? {}) as Record<string, string>;
    const c = { method: init.method ?? "GET", path: u.pathname, body: init.body ? JSON.parse(String(init.body)) : null, key: headers["Idempotency-Key"] ?? null };
    calls.push(c);
    return c;
  };
  return mockApi({
    "/api/research/packs": packs,
    "/api/research/lookup": (u: URL) => {
      const q = (u.searchParams.get("q") ?? "").toUpperCase();
      const limit = Number(u.searchParams.get("limit") ?? 8);
      const exact = LISTING.find((l) => l.ticker === q);
      const items = LISTING.filter((l) => l.ticker.startsWith(q) || l.name.toUpperCase().includes(q)).slice(0, limit).map(toItem);
      return json({
        as_of: "2026-10-22T09:00:00+00:00",
        query: q,
        list_available: listAvailable,
        exact: exact ? toItem(exact) : q.length === 4 && !listAvailable ? { ...toItem({ ticker: q, name: "", code: null, pack: null }), name: null, listed: null } : null,
        items,
      });
    },
    "/api/research/universe": (u: URL, init: RequestInit) => {
      const c = record(u, init);
      if (c.body?.ticker && c.body.ticker === failPeer) return json({ detail: "Gagal menulis" }, 500);
      return json({ ticker: c.body?.ticker }, 201);
    },
    "/api/research/universe/MANU": (u: URL, init: RequestInit) => {
      record(u, init);
      return deleteStatus === 204 ? new Response(null, { status: 204 }) : json({ detail: "MANU is held in the portfolio and cannot be removed" }, deleteStatus);
    },
    "/api/research/universe/HELD": (u: URL, init: RequestInit) => {
      record(u, init);
      return json({ detail: "HELD is held in the portfolio and cannot be removed" }, 409);
    },
  });
}

beforeEach(() => {
  calls = [];
  listAvailable = true;
  failPeer = null;
  deleteStatus = 204;
  backend();
});
afterEach(cleanup);

const type = (label: string | RegExp, value: string) => fireEvent.change(screen.getByLabelText(label), { target: { value } });
const click = (name: string | RegExp) => fireEvent.click(screen.getByRole("button", { name }));
const writes = () => calls.filter((c) => c.method !== "GET");

const STEP_NAMES = ["Emiten", "Pembanding", "Bisnis", "Ringkasan"];
/** Langkah yang tampil, dari judul "Langkah N dari 4: nama" (layar telepon tidak punya daftar langkah). */
const expectStep = (n: number) => expect(screen.getByRole("heading", { level: 2, name: `Langkah ${n} dari 4: ${STEP_NAMES[n - 1]}` })).toBeInTheDocument();

/** Isi kode, tunggu hasil cek, lanjut ke langkah 2. */
async function stepOne(ticker = "AAAA") {
  type("Kode emiten", ticker);
  await screen.findByText("Paket usulan");
  click("Lanjut");
  await screen.findByRole("heading", { name: /Langkah 2 dari 4/ });
}

describe("Tambah emiten: wizard langkah (design.md §3.4, A6.2), kasus langkah dari AddScreen.test.tsx", () => {
  // Kasus kartu 560 px di tengah (`max-w-[560px] mx-auto`) dan daftar "Langkah" milik cangkang desktop AddScreen tidak dibawa;
  // gulir horizontal halaman diperiksa di e2e (`perusahaan.e2e.ts`, `audit` di Tambah pada 320 dan 393 px).
  it("opens on step 1 of the four named steps", () => {
    renderAdd();
    expect(screen.getByRole("region", { name: "Tambah emiten" })).toBeInTheDocument();
    expectStep(1);
  });

  describe("step 1: ticker", () => {
    it("asks for a code when Lanjut is pressed empty, and stays on step 1", () => {
      renderAdd();
      click("Lanjut");
      expect(screen.getByText("Isi kode emiten")).toBeInTheDocument();
      expectStep(1);
    });

    it("rejects non-letters at once and short codes on Lanjut", () => {
      renderAdd();
      type("Kode emiten", "A1");
      expect(screen.getByText("Hanya huruf A sampai Z")).toBeInTheDocument();
      type("Kode emiten", "AB");
      expect(screen.queryByText("Hanya huruf A sampai Z")).toBeNull();
      click("Lanjut");
      expect(screen.getByText("Kode emiten terdiri dari empat huruf")).toBeInTheDocument();
    });

    it("upper-cases input and shows name, filing-derived industry code and proposed pack", async () => {
      renderAdd();
      type("Kode emiten", "aaaa");
      expect(screen.getByLabelText("Kode emiten")).toHaveValue("AAAA");
      expect(await screen.findByText("Aneka Contoh")).toBeInTheDocument();
      expect(screen.getByText("A121")).toBeInTheDocument();
      expect(screen.getByText("Tambang batubara")).toBeInTheDocument();
      expect(screen.getByRole("radio", { name: "Ikuti kode industri: Tambang batubara" })).toBeChecked();
    });

    it("a ticker without a filing says so and offers no proposed pack", async () => {
      renderAdd();
      type("Kode emiten", "BBBB");
      expect(await screen.findByText("Belum ada laporan")).toBeInTheDocument();
      expect(screen.getByText("Belum ada paket untuk kode ini")).toBeInTheDocument();
      expect(screen.getByRole("radio", { name: "Tanpa paket dulu" })).toBeChecked();
    });

    it("a ticker the exchange list does not know is refused", async () => {
      renderAdd();
      type("Kode emiten", "QQQQ");
      expect(await screen.findByText("QQQQ tidak ada di daftar bursa")).toBeInTheDocument();
      click("Lanjut");
      expectStep(1);
    });

    it("without an exchange list the ticker may continue, with the reason shown", async () => {
      listAvailable = false;
      renderAdd();
      type("Kode emiten", "QQQQ");
      expect(await screen.findByText(/Daftar bursa tidak tersedia/)).toBeInTheDocument();
      await waitFor(() => expect(screen.queryByText("Memeriksa kode…")).toBeNull());
      click("Lanjut");
      await screen.findByRole("heading", { name: /Langkah 2 dari 4/ });
    });

    it("a held ticker is explained as not removable, has no remove button, and blocks the wizard", async () => {
      renderAdd();
      type("Kode emiten", "HELD");
      expect(await screen.findByText(/dipegang di portofolio: sudah ada di Riset dan tidak bisa dihapus/)).toBeInTheDocument();
      expect(screen.getByRole("link", { name: "Buka dossier" })).toHaveAttribute("href", "/perusahaan/HELD");
      expect(screen.queryByRole("button", { name: /Hapus HELD/ })).toBeNull();
      click("Lanjut");
      expectStep(1);
      expect(writes()).toEqual([]);
    });

    it("a manual ticker can be removed: DELETE with an idempotency key", async () => {
      renderAdd();
      type("Kode emiten", "MANU");
      fireEvent.click(await screen.findByRole("button", { name: "Hapus MANU dari Riset" }));
      await waitFor(() => expect(writes()).toHaveLength(1));
      expect(writes()[0]).toMatchObject({ method: "DELETE", path: "/api/research/universe/MANU" });
      expect(writes()[0].key).toBeTruthy();
    });

    it("when the server refuses the removal (409) its message shows", async () => {
      deleteStatus = 409;
      renderAdd();
      type("Kode emiten", "MANU");
      fireEvent.click(await screen.findByRole("button", { name: "Hapus MANU dari Riset" }));
      expect(await screen.findByRole("alert")).toHaveTextContent("cannot be removed");
    });

    it("a ticker that exists only as a peer may continue and becomes manual", async () => {
      renderAdd();
      type("Kode emiten", "PEER");
      expect(await screen.findByText(/sudah ada sebagai pembanding/)).toBeInTheDocument();
      click("Lanjut");
      await screen.findByRole("heading", { name: /Langkah 2 dari 4/ });
    });
  });

  describe("step 2: peers", () => {
    it("searches by code or name, hides the ticker itself, and adds a peer", async () => {
      renderAdd();
      await stepOne("AAAA");
      type("Cari pembanding", "contoh");
      const list = await screen.findByRole("list", { name: "Hasil pencarian" });
      expect(within(list).queryByText("AAAA")).toBeNull();
      expect(within(list).getByText("BBBB")).toBeInTheDocument();
      fireEvent.click(within(list).getByRole("button", { name: "Tambah BBBB sebagai pembanding" }));
      const chosen = screen.getByRole("list", { name: "Pembanding terpilih" });
      expect(within(chosen).getByText("BBBB")).toBeInTheDocument();
      expect(within(list).getByRole("button", { name: "BBBB sudah dipilih" })).toHaveAttribute("aria-disabled", "true");
    });

    it("holds at most three peers: the fourth cannot be added until one is removed", async () => {
      renderAdd();
      await stepOne("AAAA");
      type("Cari pembanding", "contoh");
      const list = await screen.findByRole("list", { name: "Hasil pencarian" });
      for (const t of ["BBBB", "CCCC", "DDDD"]) fireEvent.click(within(list).getByRole("button", { name: `Tambah ${t} sebagai pembanding` }));
      expect(within(screen.getByRole("list", { name: "Pembanding terpilih" })).getAllByRole("listitem")).toHaveLength(3);
      expect(screen.getByText(/Sudah 3 pembanding/)).toBeInTheDocument();
      const fourth = within(list).getByRole("button", { name: "Tambah EEEE sebagai pembanding" });
      expect(fourth).toHaveAttribute("aria-disabled", "true");
      fireEvent.click(fourth);
      expect(within(screen.getByRole("list", { name: "Pembanding terpilih" })).getAllByRole("listitem")).toHaveLength(3);
      fireEvent.click(screen.getByRole("button", { name: "Hapus CCCC dari pembanding" }));
      expect(within(screen.getByRole("list", { name: "Pembanding terpilih" })).getAllByRole("listitem")).toHaveLength(2);
      expect(within(list).getByRole("button", { name: "Tambah EEEE sebagai pembanding" })).not.toHaveAttribute("aria-disabled");
    });

    it("says so when nothing matches, and peers are optional", async () => {
      renderAdd();
      await stepOne("AAAA");
      type("Cari pembanding", "zzzz");
      expect(await screen.findByText("Tidak ada hasil")).toBeInTheDocument();
      expect(screen.getByText("Belum ada pembanding")).toBeInTheDocument();
      click("Lanjut");
      await screen.findByRole("heading", { name: /Langkah 3 dari 4/ });
    });
  });

  describe("step 3: business one-liner", () => {
    async function toStepThree() {
      renderAdd();
      await stepOne("AAAA");
      click("Lanjut");
      await screen.findByRole("heading", { name: /Langkah 3 dari 4/ });
    }

    it("may stay empty, with a character counter", async () => {
      await toStepThree();
      expect(screen.getByText(/Opsional, 0\/600 karakter/)).toBeInTheDocument();
      click("Lanjut");
      await screen.findByRole("heading", { name: /Langkah 4 dari 4/ });
    });

    it("over 600 characters shows the limit and blocks Lanjut", async () => {
      await toStepThree();
      type("Bisnis dalam satu kalimat", "x".repeat(601));
      expect(screen.getByText("Paling banyak 600 karakter")).toBeInTheDocument();
      const next = screen.getByRole("button", { name: "Lanjut" });
      expect(next).toHaveAttribute("aria-disabled", "true");
      fireEvent.click(next);
      expect(screen.getByRole("heading", { name: /Langkah 3 dari 4/ })).toBeInTheDocument();
      type("Bisnis dalam satu kalimat", "x".repeat(600));
      expect(screen.queryByText("Paling banyak 600 karakter")).toBeNull();
    });
  });

  describe("step 4: summary and Mulai", () => {
    async function toSummary(opts: { peers?: string[]; note?: string; pack?: string } = {}) {
      renderAdd();
      type("Kode emiten", "AAAA");
      await screen.findByText("Paket usulan");
      if (opts.pack) fireEvent.click(await screen.findByRole("radio", { name: opts.pack }));
      click("Lanjut");
      await screen.findByRole("heading", { name: /Langkah 2 dari 4/ });
      if (opts.peers?.length) {
        type("Cari pembanding", "contoh");
        const list = await screen.findByRole("list", { name: "Hasil pencarian" });
        for (const t of opts.peers) fireEvent.click(within(list).getByRole("button", { name: `Tambah ${t} sebagai pembanding` }));
      }
      click("Lanjut");
      await screen.findByRole("heading", { name: /Langkah 3 dari 4/ });
      if (opts.note) type("Bisnis dalam satu kalimat", opts.note);
      click("Lanjut");
      await screen.findByRole("heading", { name: /Langkah 4 dari 4/ });
    }

    it("lists everything chosen", async () => {
      await toSummary({ peers: ["BBBB", "CCCC"], note: "Tambang contoh di Kalimantan" });
      const summary = screen.getByRole("heading", { name: /Ringkasan/ }).parentElement as HTMLElement;
      for (const text of ["AAAA · Aneka Contoh", "A121", "Tambang batubara (dari kode industri)", "BBBB, CCCC", "Tambang contoh di Kalimantan"]) expect(within(summary).getByText(text)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Mulai" })).toBeInTheDocument();
      expect(writes()).toEqual([]);
    });

    it("Kembali returns to the previous step with the choices kept", async () => {
      await toSummary({ note: "Catatan" });
      click("Kembali");
      expect(screen.getByLabelText("Bisnis dalam satu kalimat")).toHaveValue("Catatan");
    });

    it("Mulai adds the ticker, then each peer, then opens the company page at Kuartal terakhir", async () => {
      await toSummary({ peers: ["BBBB", "CCCC"], note: "Tambang contoh" });
      click("Mulai");
      await waitFor(() => expect(screen.getByTestId("where")).toHaveTextContent("/perusahaan/AAAA?section=kuartal"));
      expect(writes().map((c) => [c.method, c.path, c.body])).toEqual([
        ["POST", "/api/research/universe", { ticker: "AAAA", role: "manual", business_note: "Tambang contoh" }],
        ["POST", "/api/research/universe", { ticker: "BBBB", role: "peer", peer_of: "AAAA" }],
        ["POST", "/api/research/universe", { ticker: "CCCC", role: "peer", peer_of: "AAAA" }],
      ]);
      expect(new Set(writes().map((c) => c.key)).size).toBe(3);
      expect(writes().every((c) => c.key)).toBe(true);
    });

    it("without peers or a note only the ticker is written, with no pack override when the proposal stands", async () => {
      await toSummary();
      click("Mulai");
      await waitFor(() => expect(screen.getByTestId("where")).toHaveTextContent("/perusahaan/AAAA"));
      expect(writes().map((c) => c.body)).toEqual([{ ticker: "AAAA", role: "manual" }]);
    });

    it("a pack chosen by hand is sent as pack_override and named in the summary", async () => {
      await toSummary({ pack: "Pelayaran" });
      expect(screen.getByText("Pelayaran (pilihan sendiri)")).toBeInTheDocument();
      click("Mulai");
      await waitFor(() => expect(screen.getByTestId("where")).toHaveTextContent("/perusahaan/AAAA"));
      expect(writes()[0].body).toEqual({ ticker: "AAAA", role: "manual", pack_override: "shipping" });
    });

    it("a failed peer write shows the error and stays; Coba lagi replays the same keys and finishes", async () => {
      failPeer = "CCCC";
      await toSummary({ peers: ["BBBB", "CCCC"] });
      click("Mulai");
      expect(await screen.findByRole("alert")).toHaveTextContent("Gagal menulis");
      expect(screen.queryByTestId("where")).toBeNull();
      const first = writes().map((c) => c.key);
      expect(first).toHaveLength(3);
      failPeer = null;
      fireEvent.click(screen.getByRole("button", { name: "Coba lagi" }));
      await waitFor(() => expect(screen.getByTestId("where")).toHaveTextContent("/perusahaan/AAAA?section=kuartal"));
      const keys = writes().map((c) => c.key);
      expect(keys.slice(3)).toEqual([first[0], first[1], first[2]]);
    });
  });

  it("no verdict wording on any step", async () => {
    const { container } = renderAdd();
    expectNoVerdictWords(container);
    await stepOne("AAAA");
    expectNoVerdictWords(container);
    click("Lanjut");
    await screen.findByRole("heading", { name: /Langkah 3 dari 4/ });
    click("Lanjut");
    await screen.findByRole("heading", { name: /Langkah 4 dari 4/ });
    expectNoVerdictWords(container);
  });
});
