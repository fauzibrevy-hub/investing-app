import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../lib/api";
import { AuthGate } from "./AuthGate";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { BackHeader } from "./BackHeader";
import { BottomNav } from "./BottomNav";
import { UnlockScreen } from "./UnlockScreen";

let fetchMock: ReturnType<typeof vi.fn>;
beforeEach(() => {
  fetchMock = vi.fn();
  vi.stubGlobal("fetch", fetchMock);
});
afterEach(() => vi.unstubAllGlobals());

describe("BottomNav", () => {
  it("nav berlabel, 4 item, item aktif aria-current + accent", () => {
    render(<BottomNav active="portofolio" />);
    const nav = screen.getByRole("navigation", { name: "Navigasi utama" });
    const links = nav.querySelectorAll("a");
    expect(links).toHaveLength(4);
    expect([...links].map((a) => a.getAttribute("href"))).toEqual(["#/beranda", "#/portofolio", "#/perusahaan", "#/alat"]);
    const active = screen.getByRole("link", { name: "Portofolio" });
    expect(active).toHaveAttribute("aria-current", "page");
    expect(active).toHaveClass("text-accent", "font-bold");
    expect(screen.getByRole("link", { name: "Beranda" })).toHaveClass("text-nav-inactive");
    expect(nav).toHaveClass("grid-cols-4");
    expect(screen.queryByRole("button", { name: "Catat transaksi" })).not.toBeInTheDocument();
  });

  it("tautan tab memakai target dari shell dan klik biasa diteruskan ke onSelect", () => {
    const onSelect = vi.fn();
    render(<BottomNav active="beranda" targets={{ perusahaan: "/perusahaan/BUMI" }} onSelect={onSelect} />);
    const link = screen.getByRole("link", { name: "Perusahaan" });
    expect(link).toHaveAttribute("href", "#/perusahaan/BUMI");
    fireEvent.click(link);
    expect(onSelect).toHaveBeenCalledWith("perusahaan", expect.anything());
    fireEvent.click(link, { metaKey: true });
    expect(onSelect).toHaveBeenCalledTimes(1);
  });
});

function Where() {
  const { pathname, search } = useLocation();
  return <p data-testid="where">{pathname + search}</p>;
}

function renderBack(entry: string) {
  return render(
    <MemoryRouter initialEntries={[entry]}>
      <Routes>
        <Route path="*" element={<><BackHeader /><Where /></>} />
      </Routes>
    </MemoryRouter>,
  );
}

describe("BackHeader", () => {
  it("label tab pemilik dari ?from=, nama aksesibel Kembali ke …", () => {
    renderBack("/perusahaan/BUMI?from=portofolio");
    const back = screen.getByRole("link", { name: "Kembali ke Portofolio" });
    expect(back).toHaveTextContent("Portofolio");
    expect(back).toHaveClass("h-11");
  });

  it("deep link dingin: tab rute sendiri, ke akar tab", () => {
    renderBack("/alat/kalibrasi");
    fireEvent.click(screen.getByRole("link", { name: "Kembali ke Alat" }));
    expect(screen.getByTestId("where")).toHaveTextContent(/^\/alat$/);
  });
});

describe("UnlockScreen", () => {
  it("mengirim token, mengosongkan input, tidak menyimpan ke storage", async () => {
    const onUnlock = vi.fn().mockResolvedValue(undefined);
    const onUnlocked = vi.fn();
    render(<UnlockScreen onUnlock={onUnlock} onUnlocked={onUnlocked} />);
    const input = screen.getByLabelText("Token akses") as HTMLInputElement;
    expect(input.type).toBe("password");
    fireEvent.change(input, { target: { value: "token-rahasia-0123456789-abcdefghij" } });
    fireEvent.click(screen.getByRole("button", { name: "Buka kunci" }));
    expect(input.value).toBe("");
    await waitFor(() => expect(onUnlocked).toHaveBeenCalled());
    expect(onUnlock).toHaveBeenCalledWith("token-rahasia-0123456789-abcdefghij");
    expect(window.localStorage.length).toBe(0);
    expect(window.sessionStorage.length).toBe(0);
  });

  it("token salah (401) → pesan error di field", async () => {
    fetchMock.mockResolvedValue(new Response("{}", { status: 401, headers: { "Content-Type": "application/json" } }));
    render(<UnlockScreen />);
    fireEvent.change(screen.getByLabelText("Token akses"), { target: { value: "salah" } });
    fireEvent.click(screen.getByRole("button", { name: "Buka kunci" }));
    expect(await screen.findByText(/Token salah/)).toBeInTheDocument();
    expect(screen.getByLabelText("Token akses")).toHaveAttribute("aria-invalid", "true");
  });
});

describe("UnlockScreen: baris sandbox (D7)", () => {
  const SANDBOX_LINE = "Uji coba: perubahan di sini tidak dibawa ke data asli.";
  const json = (body: unknown, headers: Record<string, string> = {}) =>
    new Response(JSON.stringify(body), { status: 200, headers: { "Content-Type": "application/json", ...headers } });

  it("tampil saat GET /api/health berisi sandbox: true", async () => {
    fetchMock.mockResolvedValue(json({ status: "ok", sandbox: true }));
    render(<UnlockScreen />);
    expect(screen.getByRole("heading", { name: "Buka kunci Investasi" })).toBeInTheDocument();
    expect(await screen.findByText(SANDBOX_LINE)).toBeInTheDocument();
    expect(fetchMock.mock.calls[0][0]).toBe("/api/health");
  });

  it("tersembunyi tanpa kunci sandbox, saat health gagal, dan untuk salinan cache SW", async () => {
    for (const reply of [
      () => fetchMock.mockResolvedValue(json({ status: "ok" })),
      () => fetchMock.mockRejectedValue(new TypeError("Failed to fetch")),
      () => fetchMock.mockResolvedValue(json({ status: "ok", sandbox: true }, { "X-Investing-Cache-Date": "2026-10-27T00:00:00Z" })),
    ]) {
      reply();
      const { unmount } = render(<UnlockScreen />);
      await waitFor(() => expect(fetchMock).toHaveBeenCalledWith("/api/health", expect.anything()));
      await act(async () => {});
      expect(screen.queryByText(SANDBOX_LINE)).not.toBeInTheDocument();
      unmount();
      fetchMock.mockReset();
    }
  });
});

describe("AuthGate", () => {
  it("sesi belum terjawab → navigasi sudah tampil dengan penanda memuat, tanpa isi app", async () => {
    fetchMock.mockReturnValue(new Promise(() => {}));
    render(<AuthGate>app</AuthGate>);
    expect(screen.getByRole("navigation", { name: "Navigasi utama" })).toBeInTheDocument();
    expect(screen.getByRole("status", { name: "Memeriksa sesi" })).toHaveAttribute("aria-busy", "true");
    expect(screen.queryByText("app")).not.toBeInTheDocument();
  });

  it("sesi valid → isi app", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 204 }));
    render(<AuthGate>app</AuthGate>);
    expect(await screen.findByText("app")).toBeInTheDocument();
    expect(fetchMock.mock.calls[0][0]).toBe("/api/session");
  });

  it("401 → layar buka kunci", async () => {
    fetchMock.mockResolvedValue(new Response("{}", { status: 401, headers: { "Content-Type": "application/json" } }));
    render(<AuthGate>app</AuthGate>);
    expect(await screen.findByRole("heading", { name: "Buka kunci Investasi" })).toBeInTheDocument();
  });

  it("offline → app tetap dibuka (cache SW); 401 berikutnya mengunci", async () => {
    fetchMock.mockRejectedValueOnce(new TypeError("Failed to fetch"));
    render(<AuthGate>app</AuthGate>);
    expect(await screen.findByText("app")).toBeInTheDocument();
    fetchMock.mockResolvedValueOnce(new Response("{}", { status: 401, headers: { "Content-Type": "application/json" } }));
    await act(async () => {
      await api.get("/api/finance/summary").catch(() => {});
    });
    expect(screen.getByText(/Sesi berakhir/)).toBeInTheDocument();
  });
});
