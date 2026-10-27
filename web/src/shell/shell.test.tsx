import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../lib/api";
import { AuthGate } from "./AuthGate";
import { BottomNav } from "./BottomNav";
import { DesktopSidebar } from "./DesktopSidebar";
import { Fab } from "./Fab";
import { UnlockScreen } from "./UnlockScreen";
import { ROUTE_TITLES } from "./nav";

let fetchMock: ReturnType<typeof vi.fn>;
beforeEach(() => {
  fetchMock = vi.fn();
  vi.stubGlobal("fetch", fetchMock);
});
afterEach(() => vi.unstubAllGlobals());

describe("BottomNav", () => {
  it("nav berlabel, 4 item, item aktif aria-current + accent", () => {
    render(<BottomNav active="thesis" />);
    const nav = screen.getByRole("navigation", { name: "Navigasi utama" });
    const links = nav.querySelectorAll("a");
    expect(links).toHaveLength(4);
    const active = screen.getByRole("link", { name: "Investasi" });
    expect(active).toHaveAttribute("aria-current", "page");
    expect(active).toHaveClass("text-accent", "font-bold");
    expect(screen.getByRole("link", { name: "Beranda" })).toHaveClass("text-nav-inactive");
  });
});

describe("entri Riset di navigasi", () => {
  it("bottom nav dan sidebar menunjuk ke #/riset dan aktif di riset, research, dan idx", () => {
    for (const key of ["riset", "research", "idx"] as const) {
      const { unmount } = render(<BottomNav active={key} />);
      const link = screen.getByRole("link", { name: "Riset" });
      expect(link).toHaveAttribute("href", "#/riset");
      expect(link).toHaveAttribute("aria-current", "page");
      unmount();
    }
    const { unmount } = render(<DesktopSidebar active="riset" />);
    expect(screen.getByRole("link", { name: "Riset" })).toHaveAttribute("href", "#/riset");
    expect(screen.getByRole("link", { name: "Riset" })).toHaveAttribute("aria-current", "page");
    unmount();
    expect(ROUTE_TITLES.riset).toBe("Riset");
  });
});

describe("Fab", () => {
  it("aria-label + loading aria-busy", () => {
    const { rerender } = render(<Fab />);
    expect(screen.getByRole("button", { name: "Catat transaksi" })).toHaveClass("h-[52px]", "w-[52px]");
    rerender(<Fab loading />);
    expect(screen.getByRole("button", { name: "Catat transaksi" })).toHaveAttribute("aria-busy", "true");
  });
});

describe("DesktopSidebar", () => {
  it("5 entri tanpa Macro; item 44 px hit dengan visual 40 px; error menampilkan Coba lagi", () => {
    const onRefresh = vi.fn();
    const { rerender } = render(<DesktopSidebar active="home" />);
    const links = screen.getByRole("navigation", { name: "Navigasi utama" }).querySelectorAll("a");
    expect(links).toHaveLength(5);
    expect([...links].map((a) => a.getAttribute("href"))).not.toContain("#/macro");
    expect(links[0]).toHaveClass("h-11");
    expect(links[0].firstElementChild).toHaveClass("h-10", "bg-surface-icon", "text-accent");
    rerender(<DesktopSidebar active="home" dataStatus="error" onRefresh={onRefresh} />);
    fireEvent.click(screen.getByRole("button", { name: "Coba lagi" }));
    expect(onRefresh).toHaveBeenCalled();
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
    expect(await screen.findByRole("heading", { name: "Buka kunci Finance" })).toBeInTheDocument();
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
